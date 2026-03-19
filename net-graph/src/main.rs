use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, get_sockets_info};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;

#[derive(Serialize, Deserialize)]
struct ServerInfo {
    hostname: String,
    ips: Vec<String>,
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

fn get_local_ips() -> Vec<String> {
    let mut ips = Vec::new();

    // Método 1: via interfaces de red (más completo)
    if let Ok(interfaces) = get_if_addrs::get_if_addrs() {
        for iface in interfaces {
            // Excluir loopback (127.x.x.x y ::1)
            if !iface.is_loopback() {
                ips.push(iface.ip().to_string());
            }
        }
    }

    // Fallback si no se encontró nada
    if ips.is_empty() {
        ips.push("127.0.0.1".to_string());
    }

    ips
}

fn main() {
    let hostname = get_hostname();

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP;

    let sockets = get_sockets_info(af_flags, proto_flags).unwrap();

    let mut connections = Vec::new();

    for entry in sockets {
        if let ProtocolSocketInfo::Tcp(tcp) = entry.protocol_socket_info {
            let remote_ip = tcp.remote_addr.to_string();
            let remote_port = tcp.remote_port;

            // Filtrar conexiones triviales (puerto 0 = sin conexión activa)
            if remote_port != 0 {
                connections.push(Connection {
                    remote_ip,
                    remote_port,
                });
            }
        }
    }

    let data = ConnectionsFile {
        server: ServerInfo {
            hostname,
            ips: get_local_ips(),
        },
        connections,
    };

    let json = serde_json::to_string_pretty(&data).unwrap();
    let mut file = File::create("connections.json").unwrap();
    file.write_all(json.as_bytes()).unwrap();

    println!(
        "Guardado {} conexiones en connections.json",
        data.connections.len()
    );
}
