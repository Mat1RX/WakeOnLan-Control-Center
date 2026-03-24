use axum::{
    extract::{Path, State},
    http::{header, Method, StatusCode, Request},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{config::Config, wol};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    token: String,
}

#[derive(Serialize)]
pub struct StatusResponse {
    status: &'static str,
}

#[derive(Serialize)]
pub struct BulkStatusResponse {
    devices: HashMap<String, &'static str>,
}

#[derive(Serialize)]
pub struct DeviceListResponse {
    devices: Vec<String>,
}

pub fn api_router(config: Arc<Config>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let protected_routes = Router::new()
        .route("/devices", get(list_devices))
        .route("/status", get(status_all))
        .route("/status/:name", get(status_single))
        .route("/wake/:name", post(wake_device))
        .route_layer(middleware::from_fn_with_state(config.clone(), auth_middleware));

    Router::new()
        .route("/auth/login", post(login))
        .nest("/api", protected_routes)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(config)
}

async fn login(
    State(config): State<Arc<Config>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    if let Some(expected_pass) = config.users.get(&payload.username) {
        // Prevent timing attacks visually by using ConstantTimeEq
        let is_valid = expected_pass.as_bytes().ct_eq(payload.password.as_bytes());
        
        if is_valid.into() {
            tracing::info!("Security: Successful login for user '{}'", payload.username);
            
            let expiration = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600 * 24;

            let claims = Claims {
                sub: payload.username,
                exp: expiration as usize,
            };

            let token = encode(
                &Header::default(),
                &claims,
                &EncodingKey::from_secret(config.jwt_secret.as_ref()),
            )
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            return Ok(Json(LoginResponse { token }));
        }
    }
    
    // Antibruteforce mitigation
    tokio::time::sleep(Duration::from_secs(1)).await;
    tracing::warn!("Security: Failed login attempt for user '{}'", payload.username);
    Err(StatusCode::UNAUTHORIZED)
}

async fn auth_middleware(
    State(config): State<Arc<Config>>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req.headers().get(header::AUTHORIZATION);
    
    let token = auth_header
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_ref()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(token_data.claims);
    Ok(next.run(req).await)
}

async fn list_devices(
    State(config): State<Arc<Config>>,
    Extension(claims): Extension<Claims>,
) -> Json<DeviceListResponse> {
    tracing::info!("Action: User '{}' requested device list", claims.sub);
    let devices = config.devices.keys().cloned().collect();
    Json(DeviceListResponse { devices })
}

async fn status_all(
    State(config): State<Arc<Config>>,
    Extension(claims): Extension<Claims>,
) -> Json<BulkStatusResponse> {
    tracing::info!("Action: User '{}' requested bulk status check", claims.sub);
    let mut statuses = HashMap::new();
    for (name, (_, ip, _)) in &config.devices {
        let online = wol::is_device_online(ip).await;
        statuses.insert(name.clone(), if online { "online" } else { "offline" });
    }
    Json(BulkStatusResponse { devices: statuses })
}

async fn status_single(
    State(config): State<Arc<Config>>,
    Extension(claims): Extension<Claims>,
    Path(name): Path<String>,
) -> Result<Json<StatusResponse>, StatusCode> {
    tracing::info!("Action: User '{}' requested status for device '{}'", claims.sub, name);
    if let Some((_, ip, _)) = config.devices.get(&name) {
        let online = wol::is_device_online(ip).await;
        let status = if online { "online" } else { "offline" };
        Ok(Json(StatusResponse { status }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn wake_device(
    State(config): State<Arc<Config>>,
    Extension(claims): Extension<Claims>,
    Path(name): Path<String>,
) -> Result<Json<StatusResponse>, StatusCode> {
    if let Some((mac, ip, timeout_str)) = config.devices.get(&name) {
        tracing::info!("Action: User '{}' requested to wake device '{}' ({})", claims.sub, name, mac);
        let timeout_secs: u64 = timeout_str.parse().unwrap_or(30);

        let packet = wol::create_magic_packet(mac).map_err(|e| {
            tracing::error!("Magic packet error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        
        let socket = wol::create_wol_socket(config.interface.as_deref()).map_err(|e| {
            tracing::error!("Socket error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if let Err(e) = socket.send_to(&packet, "255.255.255.255:9") {
            tracing::error!("Failed to send WOL packet: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        tokio::time::sleep(Duration::from_secs(timeout_secs)).await;

        let online = wol::is_device_online(ip).await;
        let status = if online { "online" } else { "offline" };
        Ok(Json(StatusResponse { status }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
