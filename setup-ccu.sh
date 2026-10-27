#!/usr/bin/env bash
# =============================================================================
#  Chess Coach Utility (CCU) — Arch Linux dev/build environment setup
#  Run once on a fresh machine. Safe to re-run (idempotent where possible).
#  Usage:  bash setup-ccu.sh [--win] [--linux]
#
#  Flags:
#    --win      additionally build the Windows portable .exe (needs cargo-xwin)
#    --linux    additionally build the Linux AppImage + deb bundles
#    --all      same as --win --linux
#
#  This project lives in the folder this script lives in. Restore the whole
#  CCU/CCU folder where it lives (~/Documents/CCU/CCU) then run this script.
# =============================================================================
set -e

GREEN='\033[0;32m'; GOLD='\033[0;33m'; RED='\033[0;31m'; NC='\033[0m'
step()  { echo -e "\n${GOLD}▶ $1${NC}"; }
ok()    { echo -e "${GREEN}  ✓ $1${NC}"; }
die()   { echo -e "${RED}  ✗ $1${NC}"; exit 1; }

if ! command -v pacman &>/dev/null; then
  die "This script targets Arch Linux (pacman package manager)."
fi

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DO_WIN=false; DO_LINUX=false
for arg in "$@"; do
  case "$arg" in
    --win)    DO_WIN=true ;;
    --linux)  DO_LINUX=true ;;
    --all)    DO_WIN=true; DO_LINUX=true ;;
    *) die "Unknown argument: $arg" ;;
  esac
done

# =============================================================================
# 1. SYSTEM DEPENDENCIES  (Arch packages — Tauri v2 requires webkit2gtk-4.1)
# =============================================================================
step "Installing system dependencies"
sudo pacman -S --needed --noconfirm \
  base-devel curl git file unzip openssl \
  gtk3 webkit2gtk-4.1 \
  libappindicator-gtk3 librsvg nss \
  clang lld llvm \
  pkgconf \
  nodejs npm
ok "System packages installed"

# =============================================================================
# 2. NODE.DEVELOPMENT  (frontend / Vite / Tauri npm CLI)
# =============================================================================
step "Installing frontend dependencies"
if [ ! -d "$PROJECT_DIR/node_modules" ]; then
  npm ci --prefix "$PROJECT_DIR"
  ok "npm install complete"
else
  ok "node_modules already present"
fi

# tauri.conf.json points devUrl at http://localhost:8000 but there is no
# vite.config.* in the repo, so vite would serve on 5173 and `npm run tauri
# dev` would never connect. Generate a config that matches the app.
if [ ! -f "$PROJECT_DIR/vite.config.js" ]; then
  cat > "$PROJECT_DIR/vite.config.js" << 'JS'
import { defineConfig } from 'vite';

export default defineConfig({
  server: { port: 8000, strictPort: true }
});
JS
  ok "vite.config.js written (dev server pinned to port 8000)"
else
  ok "vite.config.js already present"
fi

# =============================================================================
# 3. RUST
# =============================================================================
step "Setting up Rust"
if ! command -v rustup &>/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
  ok "Rust installed"
else
  ok "Rust already present — updating"
  rustup update stable
fi
source "$HOME/.cargo/env"

# MSVC target (required for Windows cross-builds on Linux).
rustup target add x86_64-pc-windows-msvc
ok "Target x86_64-pc-windows-msvc added"

# =============================================================================
# 4. CARGO-XWIN  (MSVC cross-compiler — Windows .exe builds on Linux)
# =============================================================================
step "Installing cargo-xwin"
if ! command -v cargo-xwin &>/dev/null; then
  cargo install cargo-xwin --locked
  ok "cargo-xwin installed"
else
  ok "cargo-xwin already installed"
fi

# =============================================================================
# 5. TAURI CLI v2
# =============================================================================
step "Installing Tauri CLI v2"
# Like the project's other crates, pin the same major. This is what `npm run
# tauri` forwards to when you run `cargo tauri` from src-tauri.
if ! cargo tauri --version &>/dev/null 2>&1; then
  cargo install tauri-cli --version "^2" --locked
  ok "Tauri CLI installed"
else
  ok "Tauri CLI already installed ($(cargo tauri --version 2>/dev/null))"
fi

# =============================================================================
# 6. VERIFY  (fast sanity check that the Rust + Vite toolchains work together)
# =============================================================================
step "Verifying frontend build (vite)"
npm run --prefix "$PROJECT_DIR" build
ok "vite build complete"

step "Verifying Rust toolchain (cargo check on src-tauri)"
( cd "$PROJECT_DIR/src-tauri" && cargo check )
ok "cargo check complete"

# =============================================================================
# 7. (FLAG) LINUX RELEASE BUNDLES — AppImage + deb
# =============================================================================
if [ "$DO_LINUX" = true ]; then
  step "Building Linux bundles (AppImage + deb)"
  ( cd "$PROJECT_DIR" && npm run build:linux )
  ok "Linux bundles built"
  echo "  see: $PROJECT_DIR/src-tauri/target/release/bundle/"
fi

# =============================================================================
# 8. (FLAG) WINDOWS PORTABLE .exe — cross-compiled via cargo-xwin
# =============================================================================
if [ "$DO_WIN" = true ]; then
  step "Building Windows portable .exe (x86_64-pc-windows-msvc)"
  ( cd "$PROJECT_DIR" && npm run build:win )
  ok "Windows .exe built"
  echo "  see: $PROJECT_DIR/src-tauri/target/x86_64-pc-windows-msvc/release/chess-coach-utility.exe"
fi

# =============================================================================
# DONE
# =============================================================================
echo -e "\n${GREEN}═══════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  Setup complete.${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════${NC}"
echo ""
echo "  Develop (hot reload, launches the app):"
echo ""
echo "    cd $PROJECT_DIR && npm run tauri dev"
echo ""
echo "  Build without the GUI (release):"
echo ""
echo "    cd $PROJECT_DIR && npm run build:linux   # AppImage + deb"
echo "    cd $PROJECT_DIR && npm run build:win     # portable Windows .exe"
echo ""