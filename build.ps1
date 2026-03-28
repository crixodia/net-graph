#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ── Verificar rustup ──────────────────────────────────────────────────────────

if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
    Write-Error "rustup no encontrado. Instalar desde https://rustup.rs"
    exit 1
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo no encontrado. Instalar desde https://rustup.rs"
    exit 1
}

# ── Targets ───────────────────────────────────────────────────────────────────

$targets = @(
    'x86_64-pc-windows-msvc',
    'i686-pc-windows-msvc'
    # 'aarch64-pc-windows-msvc'
)

Write-Host ""
Write-Host "Instalando targets de Rust..."

foreach ($target in $targets) {
    Write-Host "  + $target"
    rustup target add $target
}

# ── Compilacion ───────────────────────────────────────────────────────────────

Write-Host ""

foreach ($target in $targets) {
    Write-Host "Compilando para $target..."
    cargo build --release --target $target
    Write-Host ""
}

# ── Verificacion ──────────────────────────────────────────────────────────────

Write-Host "Binarios generados:"
Write-Host ""

foreach ($target in $targets) {
    $bin = "target\$target\release\netmap.exe"
    if (Test-Path $bin) {
        $size = (Get-Item $bin).Length / 1KB
        Write-Host ("[OK]   {0,-55} {1:N0} KB" -f $bin, $size)
    } else {
        Write-Host "[FAIL] No se genero: $bin" -ForegroundColor Red
    }
}

Write-Host ""
