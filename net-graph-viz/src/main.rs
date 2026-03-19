use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

// ── Estructuras de entrada (formato del recolector) ──────────────────────────

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

// ── Estructuras de salida (formato del grafo) ────────────────────────────────

#[derive(Serialize, Debug)]
struct GraphNode {
    id: String,
    hostname: String,
    ips: Vec<String>,
    loopbacks: Vec<String>,
    is_external: bool,
}

#[derive(Serialize, Debug)]
struct GraphEdge {
    source: String,  // hostname origen
    target: String,  // hostname o IP destino
    ports: Vec<u16>, // puertos usados en esta conexión
    connection_count: usize,
    is_external: bool,
    is_self_loop: bool, // true cuando source == target (conexión vía loopback)
}

#[derive(Serialize, Debug)]
struct Graph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Construye un mapa de IP -> hostname a partir de todos los archivos cargados.
/// Incluye tanto IPs normales como loopbacks, ambas apuntan al mismo hostname.
fn build_ip_map(files: &[ConnectionsFile]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for f in files {
        for ip in f.server.ips.iter().chain(f.server.loopbacks.iter()) {
            map.insert(ip.clone(), f.server.hostname.clone());
        }
    }
    map
}

/// Detecta loopback por rango de IP, sin depender del mapa.
/// Cubre: 127.x.x.x, ::1, ::, 0.0.0.0 y variantes IPv6 mapeadas a IPv4.
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

fn main() {
    // Leer argumento: carpeta con los JSON
    let args: Vec<String> = std::env::args().collect();
    let folder = if args.len() > 1 { &args[1] } else { "." };

    println!("📂 Leyendo JSONs desde: {}", folder);

    // ── 1. Cargar todos los archivos JSON de la carpeta ──────────────────────
    let mut files: Vec<ConnectionsFile> = Vec::new();

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
                    println!("  ✓ {} ({})", cf.server.hostname, path.display());
                    files.push(cf);
                }
                Err(e) => eprintln!("  ✗ Error parseando {:?}: {}", path, e),
            }
        }
    }

    if files.is_empty() {
        eprintln!("No se encontraron archivos JSON válidos en '{}'", folder);
        std::process::exit(1);
    }

    // ── 2. Construir mapa IP -> hostname ─────────────────────────────────────
    let ip_map = build_ip_map(&files);
    let known_hostnames: HashSet<String> =
        files.iter().map(|f| f.server.hostname.clone()).collect();

    // ── 3. Construir nodos ───────────────────────────────────────────────────
    let mut nodes: Vec<GraphNode> = files
        .iter()
        .map(|f| GraphNode {
            id: f.server.hostname.clone(),
            hostname: f.server.hostname.clone(),
            ips: f.server.ips.clone(),
            loopbacks: f.server.loopbacks.clone(),
            is_external: false,
        })
        .collect();

    // ── 4. Construir aristas ─────────────────────────────────────────────────
    // Clave: (source_hostname, target_id) -> (puertos, count, is_external, is_self_loop)
    let mut edge_map: HashMap<(String, String), (HashSet<u16>, usize, bool, bool)> = HashMap::new();
    let mut external_nodes: HashMap<String, GraphNode> = HashMap::new();

    for file in &files {
        let source = &file.server.hostname;

        for conn in &file.connections {
            // 1. Loopback por rango de IP -> siempre es auto-conexion del nodo actual
            let (target_id, is_external, is_self_loop) = if is_loopback_ip(&conn.remote_ip) {
                (source.clone(), false, true)
            // 2. IP conocida en el mapa -> servidor del grupo (o auto si coincide hostname)
            } else if let Some(hostname) = ip_map.get(&conn.remote_ip) {
                let is_loop = hostname == source;
                (hostname.clone(), false, is_loop)
            // 3. Desconocida -> nodo externo
            } else {
                let ext_id = conn.remote_ip.clone();
                external_nodes.entry(ext_id.clone()).or_insert(GraphNode {
                    id: ext_id.clone(),
                    hostname: ext_id.clone(),
                    ips: vec![ext_id.clone()],
                    loopbacks: vec![],
                    is_external: true,
                });
                (ext_id, true, false)
            };

            let key = (source.clone(), target_id);
            let entry =
                edge_map
                    .entry(key)
                    .or_insert((HashSet::new(), 0, is_external, is_self_loop));
            entry.0.insert(conn.remote_port);
            entry.1 += 1;
        }
    }

    // Agregar nodos externos al listado
    nodes.extend(external_nodes.into_values());

    // Convertir edge_map a Vec<GraphEdge>
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

    // Ordenar para output determinístico
    edges.sort_by(|a, b| a.source.cmp(&b.source).then(a.target.cmp(&b.target)));

    // ── 5. Serializar graph.json ─────────────────────────────────────────────
    let graph = Graph { nodes, edges };
    let json = serde_json::to_string_pretty(&graph).unwrap();

    let output_path = Path::new(folder).join("graph.json");
    fs::write(&output_path, &json).expect("Error escribiendo graph.json");

    println!("\n✅ graph.json generado en: {}", output_path.display());
    println!(
        "   {} nodos ({} internos, {} externos)",
        graph.nodes.len(),
        graph.nodes.iter().filter(|n| !n.is_external).count(),
        graph.nodes.iter().filter(|n| n.is_external).count()
    );
    println!("   {} aristas", graph.edges.len());
}
