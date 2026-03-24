use socket2::{Domain, Protocol, Socket, Type};
use std::net::UdpSocket;
use std::process::Command;

pub fn create_magic_packet(mac: &str) -> Result<Vec<u8>, String> {
    let mac_bytes: Vec<u8> = mac
        .split(|c| c == ':' || c == '-')
        .filter(|s| !s.is_empty())
        .map(|b| u8::from_str_radix(b, 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|_| "Invalid MAC address format".to_string())?;

    if mac_bytes.len() != 6 {
        return Err("MAC address must be exactly 6 bytes".to_string());
    }

    let mut packet = vec![0xFF; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&mac_bytes);
    }
    Ok(packet)
}

pub fn create_wol_socket(interface: Option<&str>) -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_broadcast(true)?;

    if let Some(iface) = interface {
        #[cfg(target_os = "linux")]
        {
            if let Err(e) = socket.bind_device(Some(iface.as_bytes())) {
                tracing::error!("Failed to bind to interface {}: {}", iface, e);
            } else {
                tracing::info!("Socket successfully bound to interface: {}", iface);
            }
        }
    }
    Ok(socket.into())
}

pub async fn is_device_online(ip: &std::net::IpAddr) -> bool {
    let ip_str = ip.to_string();
    tracing::debug!("Pinging IP: {}...", ip_str);
    let status = Command::new("ping")
        .args(["-c", "1", "-W", "1", &ip_str])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    
    match status {
        Ok(s) => s.success(),
        Err(e) => {
            tracing::error!("Ping command failed for {}: {}", ip_str, e);
            false
        }
    }
}
