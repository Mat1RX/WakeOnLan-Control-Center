use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub users: HashMap<String, String>, // Username -> Password
    pub jwt_secret: String,
    pub tls_cert_path: String,
    pub tls_key_path: String,
    pub bind_address: String,                      // e.g., "0.0.0.0:8443"
    pub interface: Option<String>,                 // e.g., "br-lan"
    pub devices: HashMap<String, (String, std::net::IpAddr, String)>, // Name -> (MAC, IP, Timeout)
}
