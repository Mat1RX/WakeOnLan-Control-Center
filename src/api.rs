use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use tower_http::{cors::CorsLayer, set_header::SetResponseHeaderLayer, trace::TraceLayer};

use crate::{config::Config, wol};

/// Monotonic counter for generating unique JWT IDs.
static JTI_COUNTER: AtomicU64 = AtomicU64::new(1);

// =============================================================================
// Types
// =============================================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub jti: u64,
}

/// Shared application state across all requests.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    /// Maps jti → (expiry_unix_secs, revoked_at_unix_secs)
    revoked_tokens: Arc<Mutex<HashMap<u64, (u64, u64)>>>,
    /// Maps username → last_refresh_unix_secs for rate limiting.
    last_refresh: Arc<Mutex<HashMap<String, u64>>>,
}

impl AppState {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            revoked_tokens: Arc::new(Mutex::new(HashMap::new())),
            last_refresh:   Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Marks a JTI as revoked. Expired entries are pruned on each call.
    fn revoke(&self, jti: u64, exp: u64) {
        let now = unix_now();
        let mut map = self.revoked_tokens.lock().unwrap();
        // Clean up expired tokens
        map.retain(|_, (exp_val, _)| *exp_val > now);
        map.insert(jti, (exp, now));
    }

    /// Returns true if the token is revoked AND the 10s grace period has passed.
    fn is_revoked(&self, jti: u64) -> bool {
        let now = unix_now();
        let mut map = self.revoked_tokens.lock().unwrap();
        if let Some(&(_exp, revoked_at)) = map.get(&jti) {
            // Allow 10 seconds grace period for parallel requests during refresh
            if now.saturating_sub(revoked_at) > 10 {
                return true;
            }
        }
        false
    }

    /// Returns false if the user has refreshed within the last 60 seconds.
    fn allow_refresh(&self, username: &str) -> bool {
        const MIN_INTERVAL_SECS: u64 = 60;
        let now = unix_now();
        let mut map = self.last_refresh.lock().unwrap();
        if let Some(&last) = map.get(username) {
            if now.saturating_sub(last) < MIN_INTERVAL_SECS {
                return false;
            }
        }
        map.insert(username.to_string(), now);
        true
    }
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

#[derive(Serialize)]
struct StatusResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct BulkStatusResponse {
    devices: HashMap<String, &'static str>,
}

#[derive(Serialize)]
struct DeviceListResponse {
    devices: Vec<String>,
}

// =============================================================================
// Helpers
// =============================================================================

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Creates a signed JWT with a unique JTI for the given username.
fn mint_token(config: &Config, username: String) -> Result<String, StatusCode> {
    let jti = JTI_COUNTER.fetch_add(1, Ordering::Relaxed);
    let exp = unix_now() + config.token_expiry_hours * 3600;
    let claims = Claims { sub: username, exp: exp as usize, jti };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_ref()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// =============================================================================
// Router
// =============================================================================

pub fn api_router(config: Arc<Config>) -> Router {
    let state = AppState::new(config.clone());

    let cors = CorsLayer::new()
        .allow_origin(config.allowed_origin.parse::<HeaderValue>().unwrap_or_else(|_| {
            tracing::error!("FATAL: Invalid allowed_origin in config: {}", config.allowed_origin);
            std::process::exit(1);
        }))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let protected_routes = Router::new()
        .route("/devices",      get(list_devices))
        .route("/status",       get(status_all))
        .route("/status/:name", get(status_single))
        .route("/wake/:name",   post(wake_device))
        .route("/refresh",      post(refresh_token))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    Router::new()
        .route("/auth/login", post(login))
        .nest("/api", protected_routes)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // Security response headers
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .with_state(state)
}

// =============================================================================
// Handlers
// =============================================================================

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    if let Some(expected_pass) = state.config.users.get(&payload.username) {
        // Timing-safe password comparison to prevent timing attacks
        if expected_pass.as_bytes().ct_eq(payload.password.as_bytes()).into() {
            tracing::info!("Security: Successful login for user '{}'", payload.username);
            let token = mint_token(&state.config, payload.username)?;
            return Ok(Json(LoginResponse { token }));
        }
    }

    // Brute-force mitigation: 1s artificial delay on failed attempts
    tokio::time::sleep(Duration::from_secs(1)).await;
    tracing::warn!("Security: Failed login attempt for user '{}'", payload.username);
    Err(StatusCode::UNAUTHORIZED)
}

async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.config.jwt_secret.as_ref()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Reject tokens that have been explicitly revoked (e.g., after refresh)
    if state.is_revoked(token_data.claims.jti) {
        tracing::warn!("Security: Rejected revoked token jti={}", token_data.claims.jti);
        return Err(StatusCode::UNAUTHORIZED);
    }

    req.extensions_mut().insert(token_data.claims);
    Ok(next.run(req).await)
}

async fn list_devices(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Json<DeviceListResponse> {
    tracing::info!("Action: User '{}' requested device list", claims.sub);
    let devices = state.config.devices.keys().cloned().collect();
    Json(DeviceListResponse { devices })
}

async fn status_all(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Json<BulkStatusResponse> {
    tracing::info!("Action: User '{}' requested bulk status check", claims.sub);
    let mut statuses = HashMap::new();
    for (name, (_, ip, _)) in &state.config.devices {
        let online = wol::is_device_online(ip).await;
        statuses.insert(name.clone(), if online { "online" } else { "offline" });
    }
    Json(BulkStatusResponse { devices: statuses })
}

async fn status_single(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(name): Path<String>,
) -> Result<Json<StatusResponse>, StatusCode> {
    tracing::info!("Action: User '{}' requested status for '{}'", claims.sub, name);
    if let Some((_, ip, _)) = state.config.devices.get(&name) {
        let online = wol::is_device_online(ip).await;
        Ok(Json(StatusResponse { status: if online { "online" } else { "offline" } }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn wake_device(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(name): Path<String>,
) -> Result<Json<StatusResponse>, StatusCode> {
    if let Some((mac, ip, timeout_str)) = state.config.devices.get(&name) {
        tracing::info!("Action: User '{}' requested to wake '{}' ({})", claims.sub, name, mac);
        let timeout_secs: u64 = timeout_str.parse().unwrap_or(30);

        let packet = wol::create_magic_packet(mac).map_err(|e| {
            tracing::error!("Magic packet error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let socket = wol::create_wol_socket(state.config.interface.as_deref()).map_err(|e| {
            tracing::error!("Socket error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if let Err(e) = socket.send_to(&packet, "255.255.255.255:9") {
            tracing::error!("Failed to send WOL packet: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        tokio::time::sleep(Duration::from_secs(timeout_secs)).await;

        let online = wol::is_device_online(ip).await;
        Ok(Json(StatusResponse { status: if online { "online" } else { "offline" } }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn refresh_token(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<LoginResponse>, StatusCode> {
    // Rate-limit: at most one refresh per 60 seconds per user
    if !state.allow_refresh(&claims.sub) {
        tracing::warn!("Security: Refresh rate limit reached for user '{}'", claims.sub);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    tracing::info!("Action: User '{}' refreshing token (jti={})", claims.sub, claims.jti);

    // Invalidate the current token before issuing a new one
    state.revoke(claims.jti, claims.exp as u64);

    let token = mint_token(&state.config, claims.sub)?;
    Ok(Json(LoginResponse { token }))
}
