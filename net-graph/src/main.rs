use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;

#[derive(Serialize, Deserialize)]
struct ServerInfo {
    hostname: String,
    ips: Vec<String>,
    loopbacks: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Connection {
    remote_ip: String,
    remote_port: u16,
}

#[derive(Serialize, Deserialize)]
struct ConnectionsFile {
    server: ServerInfo,
    connections: Vec<Connection>,
}

fn get_hostname() -> String {
    hostname::get().unwrap().to_string_lossy().to_string()
}

fn get_local_ips() -> (Vec<String>, Vec<String>) {
    let mut ips = Vec::new();
    let mut loopbacks = Vec::new();

    if let Ok(interfaces) = get_if_addrs::get_if_addrs() {
        for iface in interfaces {
            if iface.is_loopback() {
                loopbacks.push(iface.ip().to_string());
            } else {
                ips.push(iface.ip().to_string());
            }
        }
    }

    if ips.is_empty() {
        ips.push("127.0.0.1".to_string());
    }

    (ips, loopbacks)
}

// ── Implementación Linux ──────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
fn get_connections() -> Vec<Connection> {
    use procfs::net::{tcp, tcp6, TcpState};

    let mut connections = Vec::new();

    if let Ok(entries) = tcp() {
        for entry in entries {
            if entry.state == TcpState::Established && entry.remote_address.port() != 0 {
                connections.push(Connection {
                    remote_ip: entry.remote_address.ip().to_string(),
                    remote_port: entry.remote_address.port(),
                });
            }
        }
    }

    if let Ok(entries) = tcp6() {
        for entry in entries {
            if entry.state == TcpState::Established && entry.remote_address.port() != 0 {
                connections.push(Connection {
                    remote_ip: entry.remote_address.ip().to_string(),
                    remote_port: entry.remote_address.port(),
                });
            }
        }
    }

    connections
}

// ── Implementación Windows ────────────────────────────────────────────────────
#[cfg(windows)]
fn get_connections() -> Vec<Connection> {
    use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};

    let mut connections = Vec::new();

    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    if let Ok(sockets) = get_sockets_info(af, ProtocolFlags::TCP) {
        for entry in sockets {
            if let ProtocolSocketInfo::Tcp(tcp) = entry.protocol_socket_info {
                if tcp.remote_port != 0 {
                    connections.push(Connection {
                        remote_ip: tcp.remote_addr.to_string(),
                        remote_port: tcp.remote_port,
                    });
                }
            }
        }
    }

    connections
}

// ── Fallback para otros SO (macOS, etc.) ──────────────────────────────────────
#[cfg(not(any(windows, target_os = "linux")))]
fn get_connections() -> Vec<Connection> {
    eprintln!("Advertencia: plataforma no soportada, sin conexiones.");
    Vec::new()
}

fn main() {
    let hostname = get_hostname();
    let (ips, loopbacks) = get_local_ips();
    let connections = get_connections();

    println!("Hostname:   {}", hostname);
    println!("IPs:        {:?}", ips);
    println!("Loopbacks:  {:?}", loopbacks);
    println!("Conexiones: {}", connections.len());

    let data = ConnectionsFile {
        server: ServerInfo {
            hostname,
            ips,
            loopbacks,
        },
        connections,
    };

    let json = serde_json::to_string_pretty(&data).unwrap();
    let mut file = File::create("connections.json").unwrap();
    file.write_all(json.as_bytes()).unwrap();

    println!("Guardado en connections.json");
}
