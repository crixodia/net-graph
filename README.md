# netmap

Command-line utility for capturing active TCP connections on a host and generating a network graph from the collected data. It can operate as a standalone capture agent, a graph builder, or both in sequence.

---

## Requirements

- Rust 1.70 or later
- Linux or Windows (macOS is not supported; capture returns an empty set)
- Root or administrator privileges may be required to read network socket tables

---

## Building

```bash
cargo build --release
```

The binary will be located at `target/release/netmap`.

---

## Usage

```
netmap <subcommand> [options]
```

### Subcommands

| Subcommand | Description |
|------------|-------------|
| `capture`  | Capture active TCP connections on this host and write the result to a JSON file |
| `graph`    | Read JSON files from a folder and generate `graph.json` |
| `run`      | Perform capture and graph generation in sequence |

---

## Subcommand Reference

### capture

Captures the active TCP connections of the local host and writes a timestamped JSON file.

```
netmap capture [--once | --watch] [--interval N] [--output-dir DIR]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--once` | yes | Single capture, saves immediately |
| `--watch` | — | Accumulates connections across scans until Ctrl+C, then saves |
| `--interval N` | `2` | Seconds between scans in `--watch` mode |
| `--output-dir DIR` | `.` | Directory where the JSON file will be written |

The output filename follows the pattern `<hostname>_<unix_timestamp>.json`.

---

### graph

Reads all JSON files in a folder (excluding `graph.json` itself) and produces a `graph.json` file describing nodes and edges.

```
netmap graph [--folder DIR] [--trim-external]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--folder DIR` | `.` | Directory containing the captured JSON files |
| `--trim-external` | — | Remove external nodes that appear in only one edge |

Multiple JSON files with the same hostname are merged into a single node. The `snapshot_count` field on each node reflects how many files were merged.

---

### run

Executes `capture` followed immediately by `graph` on the same directory.

```
netmap run [--once | --watch] [--interval N] [--folder DIR] [--output-dir DIR] [--trim-external]
```

All options from both `capture` and `graph` are accepted. If `--output-dir` is not specified, it defaults to the value of `--folder` so that the captured file and the graph are written to the same location.

---

## Output Format

### Capture file (`<hostname>_<timestamp>.json`)

```json
{
  "server": {
    "hostname": "host-a",
    "ips": ["192.168.1.10"],
    "loopbacks": ["127.0.0.1"]
  },
  "connections": [
    { "remote_ip": "192.168.1.20", "remote_port": 443 }
  ]
}
```

### Graph file (`graph.json`)

```json
{
  "nodes": [
    {
      "id": "host-a",
      "hostname": "host-a",
      "ips": ["192.168.1.10"],
      "loopbacks": ["127.0.0.1"],
      "is_external": false,
      "snapshot_count": 2
    }
  ],
  "edges": [
    {
      "source": "host-a",
      "target": "host-b",
      "ports": [443, 8080],
      "connection_count": 5,
      "is_external": false,
      "is_self_loop": false
    }
  ]
}
```

**Node fields**

| Field | Description |
|-------|-------------|
| `id` | Unique identifier; equals `hostname` for internal nodes and the IP address for external ones |
| `is_external` | `true` if the node was not present in any capture file |
| `snapshot_count` | Number of JSON files merged into this node |

**Edge fields**

| Field | Description |
|-------|-------------|
| `ports` | Sorted list of unique remote ports observed on this connection |
| `connection_count` | Total number of times this connection appeared across all snapshots |
| `is_external` | `true` if the target node is external |
| `is_self_loop` | `true` if source and target are the same host (includes loopback addresses) |

---

## Examples

Single capture, current directory:

```bash
netmap capture --once
```

Watch mode, save to a specific folder:

```bash
netmap capture --watch --interval 5 --output-dir /data/snapshots
```

Build graph from an existing folder, removing noise:

```bash
netmap graph --folder /data/snapshots --trim-external
```

Full pipeline in one command:

```bash
netmap run --watch --interval 3 --folder /data/snapshots --trim-external
```

---

## Platform Notes

- **Linux**: reads `/proc/net/tcp` and `/proc/net/tcp6` via `procfs`. Requires read access to those files (typically root or a process with `CAP_NET_ADMIN`).
- **Windows**: uses `netstat2` to query the TCP socket table. Run as administrator if entries are missing.
- **Other**: the capture step is a no-op and produces an empty connections list. Graph generation works on any platform.
