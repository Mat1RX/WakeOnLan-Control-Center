use serde::Deserialize;
use std::collections::HashMap;

fn default_token_expiry_hours() -> u64 {
    24
}

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub users: HashMap<String, String>, // Username -> Password
    pub jwt_secret: String,
    #[serde(default = "default_token_expiry_hours")]
    pub token_expiry_hours: u64,
    pub tls_cert_path: String,
    pub tls_key_path: String,
    pub bind_address: String,                      // e.g., "0.0.0.0:8443"
    pub allowed_origin: String,                    // e.g., "https://user.github.io"
    pub interface: Option<String>,                 // e.g., "br-lan"
    pub devices: HashMap<String, (String, std::net::IpAddr, String)>, // Name -> (MAC, IP, Timeout)
}
