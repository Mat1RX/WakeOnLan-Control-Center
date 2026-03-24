mod api;
mod config;
mod wol;

use axum_server::tls_rustls::RustlsConfig;
use std::{env, fs, sync::Arc};
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Initialize tracing with INFO level by default
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let args: Vec<String> = env::args().collect();
    let config_path = args.get(1).map(|s| s.as_str()).unwrap_or("config.toml");

    let content = fs::read_to_string(config_path).unwrap_or_else(|e| {
        tracing::error!("FATAL: Could not read config file {}: {}", config_path, e);
        std::process::exit(1);
    });

    let config: Arc<config::Config> = Arc::new(toml::from_str::<config::Config>(&content).unwrap_or_else(|e| {
        tracing::error!("FATAL: TOML parse error: {}", e);
        std::process::exit(1);
    }));

    // Security: enforce minimum JWT secret length to resist brute-force
    if config.jwt_secret.len() < 32 {
        tracing::error!(
            "FATAL: jwt_secret is too short ({} chars). Must be >= 32 characters.",
            config.jwt_secret.len()
        );
        std::process::exit(1);
    }

    let app = api::api_router(config.clone());

    let tls_config = RustlsConfig::from_pem_file(
        &config.tls_cert_path,
        &config.tls_key_path,
    )
    .await
    .unwrap_or_else(|e| {
        tracing::error!("FATAL: Failed to load TLS certificates: {}", e);
        std::process::exit(1);
    });

    tracing::info!("Starting HTTPS API Server on {}", config.bind_address);
    // Explicit single threaded performance for embedded constraints
    axum_server::bind_rustls(config.bind_address.parse().unwrap(), tls_config)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
