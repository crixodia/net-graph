use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

// ── Estructuras de entrada ────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct ServerInfo {
    hostname: String,
    ips: Vec<String>,
    #[serde(default)]
    loopbacks: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct Connection {
    remote_ip: String,
    remote_port: u16,
}

#[derive(Deserialize, Debug)]
struct ConnectionsFile {
    server: ServerInfo,
    connections: Vec<Connection>,
}

// ── Estructuras intermedias (post-merge) ──────────────────────────────────────

struct MergedServer {
    hostname: String,
    ips: HashSet<String>,
    loopbacks: HashSet<String>,
    // (remote_ip, remote_port) — usamos HashSet para deduplicar capturas idénticas
    connections: HashSet<(String, u16)>,
    // cuántos archivos JSON aportaron datos a este servidor
    snapshot_count: usize,
}

// ── Estructuras de salida ─────────────────────────────────────────────────────

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

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_ip_map(servers: &HashMap<String, MergedServer>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (hostname, srv) in servers {
        for ip in srv.ips.iter().chain(srv.loopbacks.iter()) {
            map.insert(ip.clone(), hostname.clone());
        }
    }
    map
}

fn is_loopback_ip(ip: &str) -> bool {
    let ip = ip.trim();
    if ip.starts_with("127.") {
        return true;
    }
    if ip == "0.0.0.0" || ip == "::" {
        return true;
    }
    if ip == "::1" {
        return true;
    }
    if let Some(rest) = ip.strip_prefix("::ffff:") {
        if rest.starts_with("127.") {
            return true;
        }
    }
    false
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn print_help(prog: &str) {
    println!("Uso: {} [opciones] [carpeta]", prog);
    println!();
    println!("Argumentos:");
    println!("  carpeta              Carpeta con los JSON recolectados (default: .)");
    println!();
    println!("Opciones:");
    println!("  --trim-external      Elimina nodos externos con una sola arista");
    println!("  --help               Muestra esta ayuda");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let prog = &args[0];

    if args.iter().any(|a| a == "--help") {
        print_help(prog);
        std::process::exit(0);
    }

    let trim_external = args.iter().any(|a| a == "--trim-external");

    // El folder es el primer argumento que no empieza con --
    let folder = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .unwrap_or(".");

    println!("Leyendo JSONs desde: {}", folder);

    // ── 1. Cargar archivos ────────────────────────────────────────────────────
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

    // ── 2. Merge por hostname ─────────────────────────────────────────────────
    // Todos los archivos con el mismo hostname se fusionan en un único servidor.
    // Las IPs y loopbacks se acumulan (union de sets).
    // Las conexiones se deduplicán por (remote_ip, remote_port) — misma conexión
    // vista en múltiples snapshots cuenta una sola vez en el grafo, pero
    // connection_count refleja cuántas veces apareció en total.
    let mut merged: HashMap<String, MergedServer> = HashMap::new();
    // Mapa auxiliar: (hostname, remote_ip, remote_port) -> total de apariciones
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

    // ── 3. Construir mapa IP -> hostname ──────────────────────────────────────
    let ip_map = build_ip_map(&merged);

    // ── 4. Construir nodos internos ───────────────────────────────────────────
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

    // ── 5. Construir aristas ──────────────────────────────────────────────────
    // Clave: (source_hostname, target_id) -> (puertos, count_total, is_external, is_self_loop)
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
            entry.1 += appearances; // suma todas las apariciones entre snapshots
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

    // ── 6. Trim externos con una sola arista (opcional) ───────────────────────
    let (nodes, edges) = if trim_external {
        // Contar cuántas aristas tiene cada nodo externo como target.
        // Usamos String como clave para no tener referencias a `edges` vivo
        // cuando luego consumimos el Vec con into_iter().
        let mut ext_edge_count: HashMap<String, usize> = HashMap::new();
        for e in &edges {
            if e.is_external {
                *ext_edge_count.entry(e.target.clone()).or_insert(0) += 1;
            }
        }

        // Nodos externos a eliminar: los que aparecen en menos de 2 aristas
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

        let removed_count = remove.len();
        if removed_count > 0 {
            println!(
                "  -- trim-external: {} nodos externos eliminados (1 sola arista)",
                removed_count
            );
        }

        (trimmed_nodes, trimmed_edges)
    } else {
        (nodes, edges)
    };

    // ── 7. Serializar ─────────────────────────────────────────────────────────
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
