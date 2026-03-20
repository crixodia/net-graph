# net-graph — Network Topology Visualizer

Herramienta para visualizar la topología de red entre servidores. Compuesta por dos programas independientes y un visualizador web.

```
net-info/             ← recolector (corre en cada servidor)
net-graph-builder/    ← procesador (corre en tu máquina)
  ├── src/main.rs
  └── Cargo.toml
net-graph-view/       ← visualizador web
  └── index.html
```

---

## 1. net-info — Recolector

Captura las conexiones TCP activas de un servidor y las guarda en un archivo JSON. Soporta Windows y Linux desde el mismo código fuente.

### Compilar

**Windows**
```powershell
cargo build --release
# binario: target\release\net-info.exe
```

**Linux / WSL**
```bash
# Dependencias del sistema (Fedora/RHEL)
sudo dnf install gcc pkg-config libpcap-devel -y

# Dependencias del sistema (Debian/Ubuntu)
sudo apt install gcc pkg-config libpcap-devel -y

cargo build --release
# binario: target/release/net-info
```

### Uso

```
net-info --once
net-info --watch [--interval N]
```

| Argumento | Descripción |
|---|---|
| `--once` | Captura una vez y guarda el JSON inmediatamente |
| `--watch` | Sensa continuamente hasta que presiones Ctrl+C |
| `--interval N` | Segundos entre cada scan en modo `--watch` (default: `2`) |

### Ejemplos

```bash
# Captura puntual
./net-info --once

# Sensa durante el tiempo que necesites y guarda al salir
./net-info --watch

# Sensa cada 5 segundos
./net-info --watch --interval 5
```

### Salida

Genera un archivo JSON en el directorio actual con el nombre `hostname_timestamp.json`:

```
hancock_1737482910.json
webserver_1737483200.json
```

Estructura del JSON:

```json
{
  "server": {
    "hostname": "hancock",
    "ips": ["192.168.1.105", "10.0.0.4"],
    "loopbacks": ["127.0.0.1", "::1"]
  },
  "connections": [
    { "remote_ip": "192.168.1.200", "remote_port": 443 },
    { "remote_ip": "127.0.0.1",     "remote_port": 5432 }
  ]
}
```

### Unicidad de conexiones

Una conexión es única por el par `(remote_ip, remote_port)`. La misma IP con distinto puerto se considera una conexión diferente. En modo `--watch`, si la misma conexión aparece en múltiples scans se registra una sola vez en el JSON final.

### Captura programada (cron)

Para capturar snapshots automáticos a lo largo del día en Linux:

```bash
# Editar crontab
crontab -e

# Captura cada 2 horas y guarda en /opt/snapshots/
0 */2 * * * /opt/net-info --once && mv /opt/connections.json /opt/snapshots/
```

---

## 2. net-graph-builder — Procesador

Lee todos los JSON recolectados, los fusiona por servidor y genera un `graph.json` listo para visualizar.

### Compilar

```bash
cargo build --release
# binario: target/release/net-graph-builder
```

### Uso

```
net-graph-builder [carpeta]
```

| Argumento | Descripción |
|---|---|
| `carpeta` | Ruta a la carpeta con los JSON recolectados (default: `.`) |

### Ejemplos

```bash
# Procesar JSONs en el directorio actual
./net-graph-builder

# Procesar JSONs en una carpeta específica
./net-graph-builder /opt/snapshots/

# Windows
net-graph-builder.exe C:\datos\snapshots
```

### Fusión de snapshots

Si hay múltiples archivos del mismo servidor (mismo `hostname`), el builder los fusiona automáticamente en un único nodo:

- Las IPs y loopbacks se acumulan (unión de todos los archivos).
- Las conexiones se deduplicán por `(remote_ip, remote_port)`.
- `connection_count` en cada arista refleja cuántas veces apareció esa conexión en total entre todos los snapshots.

```
Leyendo JSONs desde: /opt/snapshots
  + hancock (hancock_1737482910.json)
  + hancock (hancock_1737490000.json)
  + webserver (webserver_1737483200.json)

Servidores cargados: 2 (3 archivos fusionados en 2 servidores unicos)
  ~ hancock -> 2 snapshots fusionados

graph.json generado en: /opt/snapshots/graph.json
   3 nodos (2 internos, 1 externos)
   5 aristas
```

### Salida

Genera `graph.json` en la misma carpeta de los JSONs de entrada. Este archivo es la entrada para el visualizador.

---

## 3. net-graph-view/index.html — Visualizador Web

Abre `index.html` en cualquier navegador moderno. No requiere servidor web ni instalación adicional.

### Cargar datos

Al abrir la página aparece una pantalla de carga. Puedes:
- Hacer clic en **CARGAR graph.json** y seleccionar el archivo.
- Arrastrar el archivo `graph.json` directamente sobre la ventana.

### Tipos de nodos

| Visual | Significado |
|---|---|
| Círculo cian | Servidor interno del grupo |
| Círculo rojo | Host externo (IP no reconocida en el grupo) |
| Ring punteado púrpura | Servidor con auto-conexiones loopback |

### Tipos de aristas

| Visual | Significado |
|---|---|
| Línea cian sólida | Conexión entre servidores internos |
| Línea roja punteada | Conexión a host externo |
| Arco púrpura animado | Auto-conexión loopback |

### Controles

| Control | Acción |
|---|---|
| Scroll / pinch | Zoom |
| Click y arrastrar (fondo) | Pan |
| Click y arrastrar (nodo) | Mover nodo |
| Click en nodo | Ver detalles en el panel Inspector |
| Click en fondo | Deseleccionar |
| **⊡ RESET** | Restaurar zoom y posición |
| **⊘ EXTERNOS** | Mostrar / ocultar nodos externos |
| **⟳ REORGANIZAR** | Relanzar la simulación de fuerzas |

### Panel Inspector

Al hacer click en un nodo muestra:

- Tipo (interno / externo / con loopback)
- Hostname e IPs asignadas
- Direcciones loopback
- Puertos usados en conexiones loopback
- Puertos de salida hacia otros nodos
- Servidores a los que se conecta (con conteo de conexiones y puertos)
- Servidores desde los que recibe conexiones
- Número de snapshots fusionados en ese nodo

---

## Flujo completo de uso

```
Servidor A                    Servidor B
│                             │
│  ./net-info --watch         │  ./net-info --watch
│  (captura durante X horas)  │  (captura durante X horas)
│  Ctrl+C                     │  Ctrl+C
│                             │
│  hancock_1737482910.json    │  webserver_1737483200.json
│  hancock_1737490000.json    │  (múltiples snapshots OK)
│            │                │           │
│            └────────────────┘           │
│                      │                  │
│               /opt/snapshots/ ──────────┘
│                      │
│                      ↓
│   ./net-graph-builder /opt/snapshots/
│                      │
│                      ↓
│                 graph.json
│                      │
│                      ↓
│    Abrir net-graph-view/index.html
│         → arrastrar graph.json a la ventana
```
