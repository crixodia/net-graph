use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, get_sockets_info};
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
    let mut ips: Vec<String> = Vec::new();
    let mut loopbacks: Vec<String> = Vec::new();

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

fn main() {
    let hostname = get_hostname();
    let (ips, loopbacks) = get_local_ips();

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP;

    let sockets = get_sockets_info(af_flags, proto_flags).unwrap();

    let mut connections = Vec::new();

    for entry in sockets {
        if let ProtocolSocketInfo::Tcp(tcp) = entry.protocol_socket_info {
            let remote_ip = tcp.remote_addr.to_string();
            let remote_port = tcp.remote_port;

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
            ips,
            loopbacks,
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
