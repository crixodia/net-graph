use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ═══════════════════════════════════════════════════════════════════════════════
// Estructuras compartidas
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Serialize, Deserialize, Debug)]
struct ServerInfo {
    hostname: String,
    ips: Vec<String>,
    #[serde(default)]
    loopbacks: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Connection {
    remote_ip: String,
    remote_port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct ConnectionsFile {
    server: ServerInfo,
    connections: Vec<Connection>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Estructuras de salida (grafo)
// ═══════════════════════════════════════════════════════════════════════════════

struct MergedServer {
    hostname: String,
    ips: HashSet<String>,
    loopbacks: HashSet<String>,
    connections: HashSet<(String, u16)>,
    snapshot_count: usize,
}

#[derive(Serialize, Debug)]
struct GraphNode {
    id: String,
    hostname: String,
    ips: Vec<String>,
    loopbacks: Vec<String>,
    is_external: bool,
    snapshot_count: usize,
}

#[derive(Serialize, Debug)]
struct GraphEdge {
    source: String,
    target: String,
    ports: Vec<u16>,
    connection_count: usize,
    is_external: bool,
    is_self_loop: bool,
}

#[derive(Serialize, Debug)]
struct Graph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers — captura
// ═══════════════════════════════════════════════════════════════════════════════

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
    eprintln!("Advertencia: plataforma no soportada para captura de conexiones.");
    HashSet::new()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers — grafo
// ═══════════════════════════════════════════════════════════════════════════════

fn is_loopback_ip(ip: &str) -> bool {
    let ip = ip.trim();
    if ip.starts_with("127.") {
        return true;
    }
    if ip == "0.0.0.0" || ip == "::" || ip == "::1" {
        return true;
    }
    if let Some(rest) = ip.strip_prefix("::ffff:") {
        if rest.starts_with("127.") {
            return true;
        }
    }
    false
}

fn build_ip_map(servers: &HashMap<String, MergedServer>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (hostname, srv) in servers {
        for ip in srv.ips.iter().chain(srv.loopbacks.iter()) {
            map.insert(ip.clone(), hostname.clone());
        }
    }
    map
}

// ═══════════════════════════════════════════════════════════════════════════════
// Acción: guardar captura en JSON
// ═══════════════════════════════════════════════════════════════════════════════

fn save_capture(
    hostname: &str,
    ips: Vec<String>,
    loopbacks: Vec<String>,
    seen: HashSet<(String, u16)>,
    output_dir: &str,
) {
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
    let path = Path::new(output_dir).join(&filename);
    let json = serde_json::to_string_pretty(&data).unwrap();
    let mut file = File::create(&path).unwrap();
    file.write_all(json.as_bytes()).unwrap();

    println!("Guardado: {} ({} conexiones unicas)", path.display(), total);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Acción: generar grafo desde JSONs en una carpeta
// ═══════════════════════════════════════════════════════════════════════════════

fn generate_graph(folder: &str, trim_external: bool) {
    println!("Leyendo JSONs desde: {}", folder);

    // 1. Cargar archivos
    let mut raw_files: Vec<ConnectionsFile> = Vec::new();

    let entries = fs::read_dir(folder).expect("No se pudo leer la carpeta");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json")
            && path.file_name().and_then(|n| n.to_str()) != Some("graph.json")
        {
            let content =
                fs::read_to_string(&path).unwrap_or_else(|_| panic!("Error leyendo {:?}", path));
            match serde_json::from_str::<ConnectionsFile>(&content) {
                Ok(cf) => {
                    println!("  + {} ({})", cf.server.hostname, path.display());
                    raw_files.push(cf);
                }
                Err(e) => eprintln!("  ! Error parseando {:?}: {}", path, e),
            }
        }
    }

    if raw_files.is_empty() {
        eprintln!("No se encontraron archivos JSON validos en '{}'", folder);
        std::process::exit(1);
    }

    // 2. Merge por hostname
    let mut merged: HashMap<String, MergedServer> = HashMap::new();
    let mut conn_count_map: HashMap<(String, String, u16), usize> = HashMap::new();

    for file in &raw_files {
        let hostname = file.server.hostname.clone();

        let entry = merged
            .entry(hostname.clone())
            .or_insert_with(|| MergedServer {
                hostname: hostname.clone(),
                ips: HashSet::new(),
                loopbacks: HashSet::new(),
                connections: HashSet::new(),
                snapshot_count: 0,
            });

        entry.snapshot_count += 1;
        entry.ips.extend(file.server.ips.iter().cloned());
        entry
            .loopbacks
            .extend(file.server.loopbacks.iter().cloned());

        for conn in &file.connections {
            entry
                .connections
                .insert((conn.remote_ip.clone(), conn.remote_port));
            *conn_count_map
                .entry((hostname.clone(), conn.remote_ip.clone(), conn.remote_port))
                .or_insert(0) += 1;
        }
    }

    let total_files = raw_files.len();
    let unique_hosts = merged.len();
    let duplicate_msg = if total_files > unique_hosts {
        format!(
            " ({} archivos fusionados en {} servidores unicos)",
            total_files, unique_hosts
        )
    } else {
        String::new()
    };

    println!("\nServidores cargados: {}{}", unique_hosts, duplicate_msg);
    for (hostname, srv) in &merged {
        if srv.snapshot_count > 1 {
            println!(
                "  ~ {} -> {} snapshots fusionados",
                hostname, srv.snapshot_count
            );
        }
    }

    // 3. Mapa IP -> hostname
    let ip_map = build_ip_map(&merged);

    // 4. Nodos internos
    let mut nodes: Vec<GraphNode> = merged
        .values()
        .map(|srv| {
            let mut ips: Vec<String> = srv.ips.iter().cloned().collect();
            let mut lbs: Vec<String> = srv.loopbacks.iter().cloned().collect();
            ips.sort();
            lbs.sort();
            GraphNode {
                id: srv.hostname.clone(),
                hostname: srv.hostname.clone(),
                ips,
                loopbacks: lbs,
                is_external: false,
                snapshot_count: srv.snapshot_count,
            }
        })
        .collect();

    // 5. Aristas
    let mut edge_map: HashMap<(String, String), (HashSet<u16>, usize, bool, bool)> = HashMap::new();
    let mut external_nodes: HashMap<String, GraphNode> = HashMap::new();

    for (hostname, srv) in &merged {
        for (remote_ip, remote_port) in &srv.connections {
            let appearances = conn_count_map
                .get(&(hostname.clone(), remote_ip.clone(), *remote_port))
                .copied()
                .unwrap_or(1);

            let (target_id, is_external, is_self_loop) = if is_loopback_ip(remote_ip) {
                (hostname.clone(), false, true)
            } else if let Some(target_hostname) = ip_map.get(remote_ip) {
                let is_loop = target_hostname == hostname;
                (target_hostname.clone(), false, is_loop)
            } else {
                let ext_id = remote_ip.clone();
                external_nodes.entry(ext_id.clone()).or_insert(GraphNode {
                    id: ext_id.clone(),
                    hostname: ext_id.clone(),
                    ips: vec![ext_id.clone()],
                    loopbacks: vec![],
                    is_external: true,
                    snapshot_count: 0,
                });
                (ext_id, true, false)
            };

            let key = (hostname.clone(), target_id);
            let entry =
                edge_map
                    .entry(key)
                    .or_insert((HashSet::new(), 0, is_external, is_self_loop));
            entry.0.insert(*remote_port);
            entry.1 += appearances;
        }
    }

    nodes.extend(external_nodes.into_values());

    let mut edges: Vec<GraphEdge> = edge_map
        .into_iter()
        .map(|((source, target), (ports, count, is_ext, is_self))| {
            let mut port_vec: Vec<u16> = ports.into_iter().collect();
            port_vec.sort();
            GraphEdge {
                source,
                target,
                ports: port_vec,
                connection_count: count,
                is_external: is_ext,
                is_self_loop: is_self,
            }
        })
        .collect();

    edges.sort_by(|a, b| a.source.cmp(&b.source).then(a.target.cmp(&b.target)));

    // 6. Trim externos opcionales
    let (nodes, edges) = if trim_external {
        let mut ext_edge_count: HashMap<String, usize> = HashMap::new();
        for e in &edges {
            if e.is_external {
                *ext_edge_count.entry(e.target.clone()).or_insert(0) += 1;
            }
        }

        let remove: HashSet<String> = ext_edge_count
            .into_iter()
            .filter(|(_, count)| *count < 2)
            .map(|(id, _)| id)
            .collect();

        let trimmed_edges: Vec<GraphEdge> = edges
            .into_iter()
            .filter(|e| !e.is_external || !remove.contains(&e.target))
            .collect();

        let trimmed_nodes: Vec<GraphNode> = nodes
            .into_iter()
            .filter(|n| !n.is_external || !remove.contains(&n.id))
            .collect();

        if !remove.is_empty() {
            println!(
                "  -- trim-external: {} nodos externos eliminados (1 sola arista)",
                remove.len()
            );
        }

        (trimmed_nodes, trimmed_edges)
    } else {
        (nodes, edges)
    };

    // 7. Serializar
    let graph = Graph { nodes, edges };
    let json = serde_json::to_string_pretty(&graph).unwrap();

    let output_path = Path::new(folder).join("graph.json");
    fs::write(&output_path, &json).expect("Error escribiendo graph.json");

    println!("\ngraph.json generado en: {}", output_path.display());
    println!(
        "   {} nodos ({} internos, {} externos)",
        graph.nodes.len(),
        graph.nodes.iter().filter(|n| !n.is_external).count(),
        graph.nodes.iter().filter(|n| n.is_external).count(),
    );
    println!("   {} aristas", graph.edges.len());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Ayuda
// ═══════════════════════════════════════════════════════════════════════════════

fn print_help(prog: &str) {
    println!("Uso: {} <subcomando> [opciones]", prog);
    println!();
    println!("Subcomandos:");
    println!("  capture              Captura conexiones TCP activas de este host");
    println!("  graph                Genera graph.json a partir de JSONs capturados");
    println!("  run                  Captura y luego genera el grafo (todo en uno)");
    println!();
    println!("Opciones de 'capture' y 'run':");
    println!("  --once               Captura puntual y guarda (default si no se indica modo)");
    println!("  --watch              Acumula conexiones hasta Ctrl+C y luego guarda");
    println!("  --interval N         Segundos entre scans en modo --watch (default: 2)");
    println!("  --output-dir DIR     Carpeta donde guardar el JSON (default: .)");
    println!();
    println!("Opciones de 'graph' y 'run':");
    println!("  --folder DIR         Carpeta con los JSONs a procesar (default: .)");
    println!("  --trim-external      Elimina nodos externos con una sola arista");
    println!();
    println!("Ejemplos:");
    println!("  {} capture --once", prog);
    println!(
        "  {} capture --watch --interval 5 --output-dir /data/snapshots",
        prog
    );
    println!("  {} graph --folder /data/snapshots --trim-external", prog);
    println!(
        "  {} run --watch --folder /data/snapshots --trim-external",
        prog
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Main
// ═══════════════════════════════════════════════════════════════════════════════

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let prog = &args[0];

    // Sin argumentos o --help → ayuda
    if args.len() < 2 || args.iter().any(|a| a == "--help" || a == "-h") {
        print_help(prog);
        std::process::exit(0);
    }

    let subcommand = &args[1];

    // Flags compartidos
    let trim_external = args.iter().any(|a| a == "--trim-external");

    let folder = args
        .windows(2)
        .find(|w| w[0] == "--folder")
        .map(|w| w[1].as_str())
        .unwrap_or(".");

    let output_dir = args
        .windows(2)
        .find(|w| w[0] == "--output-dir")
        .map(|w| w[1].as_str())
        .unwrap_or(folder); // si no se indica, coincide con --folder

    let mode_watch = args.iter().any(|a| a == "--watch");
    // --once es el modo por defecto cuando no se usa --watch
    let mode_once = !mode_watch;

    let interval_secs: u64 = args
        .windows(2)
        .find(|w| w[0] == "--interval")
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(2);

    match subcommand.as_str() {
        // ── capture ───────────────────────────────────────────────────────────
        "capture" => {
            run_capture(mode_watch, mode_once, interval_secs, output_dir);
        }

        // ── graph ─────────────────────────────────────────────────────────────
        "graph" => {
            generate_graph(folder, trim_external);
        }

        // ── run (capture + graph) ─────────────────────────────────────────────
        "run" => {
            run_capture(mode_watch, mode_once, interval_secs, output_dir);
            println!("\nGenerando grafo...\n");
            // El folder para el grafo es el mismo output_dir de la captura
            generate_graph(output_dir, trim_external);
        }

        other => {
            eprintln!("Subcomando desconocido: '{}'\n", other);
            print_help(prog);
            std::process::exit(1);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Lógica de captura (extraída para reutilizarla desde 'run')
// ═══════════════════════════════════════════════════════════════════════════════

fn run_capture(mode_watch: bool, _mode_once: bool, interval_secs: u64, output_dir: &str) {
    let hostname = get_hostname();
    let (ips, loopbacks) = get_local_ips();

    if !mode_watch {
        // ── Modo único ────────────────────────────────────────────────────────
        println!("Capturando conexiones (una vez)...");
        let seen = snapshot_connections();
        println!("Encontradas {} conexiones unicas.", seen.len());
        save_capture(&hostname, ips, loopbacks, seen, output_dir);
    } else {
        // ── Modo continuo ─────────────────────────────────────────────────────
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

            let steps = interval_secs * 10;
            for _ in 0..steps {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }

        println!("\nDetenido tras {} scans.", scans);
        save_capture(&hostname, ips, loopbacks, accumulated, output_dir);
    }
}
