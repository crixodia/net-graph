use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

// ── Snapshot de conexiones activas ────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn snapshot_connections() -> HashSet<(String, u16)> {
    use procfs::net::{tcp, tcp6, TcpState};

    let mut seen = HashSet::new();

    if let Ok(entries) = tcp() {
        for entry in entries {
            if entry.state == TcpState::Established && entry.remote_address.port() != 0 {
                seen.insert((
                    entry.remote_address.ip().to_string(),
                    entry.remote_address.port(),
                ));
            }
        }
    }

    if let Ok(entries) = tcp6() {
        for entry in entries {
            if entry.state == TcpState::Established && entry.remote_address.port() != 0 {
                seen.insert((
                    entry.remote_address.ip().to_string(),
                    entry.remote_address.port(),
                ));
            }
        }
    }

    seen
}

#[cfg(windows)]
fn snapshot_connections() -> HashSet<(String, u16)> {
    use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};

    let mut seen = HashSet::new();

    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    if let Ok(sockets) = get_sockets_info(af, ProtocolFlags::TCP) {
        for entry in sockets {
            if let ProtocolSocketInfo::Tcp(tcp) = entry.protocol_socket_info {
                if tcp.remote_port != 0 {
                    seen.insert((tcp.remote_addr.to_string(), tcp.remote_port));
                }
            }
        }
    }

    seen
}

#[cfg(not(any(windows, target_os = "linux")))]
fn snapshot_connections() -> HashSet<(String, u16)> {
    eprintln!("Advertencia: plataforma no soportada.");
    HashSet::new()
}

// ── Guardar JSON ──────────────────────────────────────────────────────────────

fn save(hostname: &str, ips: Vec<String>, loopbacks: Vec<String>, seen: HashSet<(String, u16)>) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();

    let connections: Vec<Connection> = seen
        .into_iter()
        .map(|(ip, port)| Connection {
            remote_ip: ip,
            remote_port: port,
        })
        .collect();

    let total = connections.len();

    let data = ConnectionsFile {
        server: ServerInfo {
            hostname: hostname.to_string(),
            ips,
            loopbacks,
        },
        connections,
    };

    let filename = format!("{}_{}.json", hostname, timestamp);
    let json = serde_json::to_string_pretty(&data).unwrap();
    let mut file = File::create(&filename).unwrap();
    file.write_all(json.as_bytes()).unwrap();

    println!("\nGuardado: {} ({} conexiones unicas)", filename, total);
}

// ── Ayuda ─────────────────────────────────────────────────────────────────────

fn print_help(prog: &str) {
    println!("Uso:");
    println!(
        "  {} --once                   Captura una vez y guarda",
        prog
    );
    println!(
        "  {} --watch [--interval N]   Sensa continuamente (Ctrl+C para guardar)",
        prog
    );
    println!();
    println!("Opciones:");
    println!("  --once              Captura unica");
    println!("  --watch             Modo continuo hasta Ctrl+C");
    println!("  --interval N        Segundos entre cada scan (default: 2)");
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let prog = &args[0];

    let mode_once = args.iter().any(|a| a == "--once");
    let mode_watch = args.iter().any(|a| a == "--watch");

    if !mode_once && !mode_watch {
        print_help(prog);
        std::process::exit(1);
    }

    let interval_secs: u64 = args
        .windows(2)
        .find(|w| w[0] == "--interval")
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(2);

    let hostname = get_hostname();
    let (ips, loopbacks) = get_local_ips();

    // ── Modo único ────────────────────────────────────────────────────────────
    if mode_once {
        println!("Capturando conexiones (una vez)...");
        let seen = snapshot_connections();
        println!("Encontradas {} conexiones unicas.", seen.len());
        save(&hostname, ips, loopbacks, seen);
        return;
    }

    // ── Modo continuo ─────────────────────────────────────────────────────────
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error registrando Ctrl+C");

    println!(
        "Sensando cada {}s — Ctrl+C para detener y guardar...",
        interval_secs
    );

    let mut accumulated: HashSet<(String, u16)> = HashSet::new();
    let mut scans: u64 = 0;

    while running.load(Ordering::SeqCst) {
        let snap = snapshot_connections();
        let before = accumulated.len();
        accumulated.extend(snap);
        let new_conns = accumulated.len() - before;
        scans += 1;

        print!(
            "\rScan #{} | acumuladas: {} | nuevas: {}   ",
            scans,
            accumulated.len(),
            new_conns
        );
        let _ = std::io::stdout().flush();

        // Sleep en pasos de 100ms para responder rápido al Ctrl+C
        let steps = interval_secs * 10;
        for _ in 0..steps {
            if !running.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    println!("\nDetenido tras {} scans.", scans);
    save(&hostname, ips, loopbacks, accumulated);
}
