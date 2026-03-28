#!/usr/bin/env bash
set -euo pipefail

# ── Detección de distro ───────────────────────────────────────────────────────

if [ -f /etc/os-release ]; then
    . /etc/os-release
    DISTRO="${ID}"
else
    echo "ERROR: No se puede determinar la distribución." >&2
    exit 1
fi

# ── Instalación de dependencias del sistema ───────────────────────────────────

echo "Distribución detectada: ${DISTRO}"
echo "Instalando dependencias del sistema..."

case "${DISTRO}" in
    debian|ubuntu|raspbian)
        sudo apt-get update -qq
        sudo apt-get install -y \
            musl-tools \
            gcc-aarch64-linux-gnu \
            gcc-arm-linux-gnueabihf
        ;;
    fedora)
        sudo dnf install -y \
            musl-gcc \
            gcc-aarch64-linux-gnu \
            gcc-arm-linux-gnu
        ;;
    *)
        echo "ERROR: Distribución no soportada: ${DISTRO}" >&2
        echo "       Soportadas: debian, ubuntu, raspbian, fedora" >&2
        exit 1
        ;;
esac

# ── Targets de Rust ───────────────────────────────────────────────────────────

echo ""
echo "Instalando targets de Rust..."

rustup target add \
    x86_64-unknown-linux-musl \
    aarch64-unknown-linux-musl \
    armv7-unknown-linux-musleabihf

# ── Resolución de nombres de linker según distro ──────────────────────────────

case "${DISTRO}" in
    debian|ubuntu|raspbian)
        LINKER_AARCH64="aarch64-linux-gnu-gcc"
        LINKER_ARMV7="arm-linux-gnueabihf-gcc"
        ;;
    fedora)
        LINKER_AARCH64="aarch64-linux-gnu-gcc"
        LINKER_ARMV7="arm-linux-gnu-gcc"
        ;;
esac

# ── Compilación ───────────────────────────────────────────────────────────────

echo ""
echo "Compilando para x86_64-unknown-linux-musl..."
cargo build --release --target x86_64-unknown-linux-musl

echo ""
echo "Compilando para aarch64-unknown-linux-musl (RPi 64-bit)..."
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="${LINKER_AARCH64}" \
cargo build --release --target aarch64-unknown-linux-musl

echo ""
echo "Compilando para armv7-unknown-linux-musleabihf (RPi 32-bit)..."
CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER="${LINKER_ARMV7}" \
cargo build --release --target armv7-unknown-linux-musleabihf

# ── Verificación ──────────────────────────────────────────────────────────────

echo ""
echo "Verificando binarios..."
echo ""

for TARGET in \
    x86_64-unknown-linux-musl \
    aarch64-unknown-linux-musl \
    armv7-unknown-linux-musleabihf
do
    BIN="target/${TARGET}/release/netmap"
    if [ -f "${BIN}" ]; then
        echo "[OK] ${BIN}"
        file "${BIN}"
        ldd  "${BIN}" 2>&1 || true
    else
        echo "[FAIL] No se generó: ${BIN}" >&2
    fi
    echo ""
done
