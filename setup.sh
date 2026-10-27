#!/usr/bin/env bash
# =============================================================================
#  Remuneration Tracker — Arch Linux → Windows .exe build setup
#  Run once on a fresh machine. Safe to re-run (idempotent where possible).
#  Usage:  bash setup.sh
# =============================================================================
set -e

GREEN='\033[0;32m'; GOLD='\033[0;33m'; RED='\033[0;31m'; NC='\033[0m'
step()  { echo -e "\n${GOLD}▶ $1${NC}"; }
ok()    { echo -e "${GREEN}  ✓ $1${NC}"; }
die()   { echo -e "${RED}  ✗ $1${NC}"; exit 1; }

PROJECT_DIR="$HOME/remuneration-tracker"
WEBVIEW2_DIR="$HOME/webview2-sdk"
WEBVIEW2_URL="https://www.nuget.org/api/v2/package/Microsoft.Web.WebView2/1.0.2849.39"

# =============================================================================
# 1. SYSTEM DEPENDENCIES
# =============================================================================
step "Installing system dependencies"
sudo pacman -S --needed --noconfirm \
  base-devel curl wget file openssl unzip \
  gtk3 webkit2gtk-4.1 \
  libappindicator-gtk3 librsvg nss \
  clang lld llvm \
  imagemagick
ok "System packages installed"

# =============================================================================
# 2. RUST
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

# MSVC target (required — GNU target always dynamically links WebView2Loader.dll
# which causes "DLL not found" on Windows; MSVC target links it statically)
rustup target add x86_64-pc-windows-msvc
ok "Target x86_64-pc-windows-msvc added"

# =============================================================================
# 3. CARGO-XWIN  (MSVC cross-compiler — replaces MinGW for this project)
# =============================================================================
step "Installing cargo-xwin"
if ! command -v cargo-xwin &>/dev/null; then
  cargo install cargo-xwin --locked
  ok "cargo-xwin installed"
else
  ok "cargo-xwin already installed"
fi

# =============================================================================
# 4. TAURI CLI
# =============================================================================
step "Installing Tauri CLI v2"
if ! cargo tauri --version &>/dev/null 2>&1; then
  cargo install tauri-cli --version "^2" --locked
  ok "Tauri CLI installed"
else
  ok "Tauri CLI already installed ($(cargo tauri --version 2>/dev/null))"
fi

# =============================================================================
# 5. WEBVIEW2 SDK  (provides WebView2LoaderStatic.lib for static linking)
# =============================================================================
step "Downloading WebView2 SDK"
if [ -f "$WEBVIEW2_DIR/build/native/x64/WebView2LoaderStatic.lib" ]; then
  ok "WebView2 SDK already present"
else
  mkdir -p "$WEBVIEW2_DIR"
  curl -L "$WEBVIEW2_URL" -o /tmp/webview2.zip
  unzip -o /tmp/webview2.zip -d "$WEBVIEW2_DIR" > /dev/null
  rm /tmp/webview2.zip
  [ -f "$WEBVIEW2_DIR/build/native/x64/WebView2LoaderStatic.lib" ] \
    && ok "WebView2 SDK extracted to $WEBVIEW2_DIR" \
    || die "WebView2LoaderStatic.lib not found after extraction"
fi

# =============================================================================
# 6. PROJECT STRUCTURE
# =============================================================================
step "Creating project structure"
mkdir -p "$PROJECT_DIR/frontend"
mkdir -p "$PROJECT_DIR/src-tauri/src"
mkdir -p "$PROJECT_DIR/src-tauri/capabilities"
mkdir -p "$PROJECT_DIR/src-tauri/icons"
ok "Directories created"

# =============================================================================
# 7. PROJECT FILES
# =============================================================================
step "Writing project files"

# ── src-tauri/Cargo.toml ─────────────────────────────────────────────────────
cat > "$PROJECT_DIR/src-tauri/Cargo.toml" << 'TOML'
[package]
name    = "remuneration-tracker"
version = "1.0.0"
edition = "2021"

[lib]
name       = "remuneration_tracker_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri              = { version = "2", features = ["devtools"] }
serde              = { version = "1", features = ["derive"] }
serde_json         = "1"
tauri-plugin-shell = "2"
tauri-plugin-oauth = "2"

[profile.release]
panic         = "abort"
codegen-units = 1
lto           = true
opt-level     = "s"
strip         = true
TOML

# ── src-tauri/build.rs ───────────────────────────────────────────────────────
cat > "$PROJECT_DIR/src-tauri/build.rs" << 'RUST'
fn main() {
    tauri_build::build()
}
RUST

# ── src-tauri/src/main.rs ────────────────────────────────────────────────────
cat > "$PROJECT_DIR/src-tauri/src/main.rs" << 'RUST'
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    remuneration_tracker_lib::run()
}
RUST

# ── src-tauri/src/lib.rs ─────────────────────────────────────────────────────
cat > "$PROJECT_DIR/src-tauri/src/lib.rs" << 'RUST'
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_oauth::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
RUST

# ── src-tauri/tauri.conf.json ────────────────────────────────────────────────
cat > "$PROJECT_DIR/src-tauri/tauri.conf.json" << 'JSON'
{
  "productName": "Remuneration Tracker",
  "version": "1.0.0",
  "identifier": "com.armiel.remuneration-tracker",
  "build": {
    "frontendDist": "../frontend"
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "title": "Remuneration Tracker",
        "width": 1280,
        "height": 800,
        "minWidth": 900,
        "minHeight": 600,
        "resizable": true,
        "fullscreen": false,
        "center": true,
        "additionalBrowserArgs": "--disable-features=msTrackingPrevention"
      }
    ],
    "security": {
      "dangerousDisableAssetCspModification": true,
      "csp": "default-src 'self'; script-src 'self' https://www.gstatic.com 'unsafe-inline'; connect-src 'self' https://*.googleapis.com https://*.firebaseio.com wss://*.firebaseio.com https://fonts.googleapis.com https://www.gstatic.com https://gist.githubusercontent.com; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src 'self' data: https://*.googleusercontent.com"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "publisher": "Armiel Pillay",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
JSON

# ── src-tauri/capabilities/default.json ──────────────────────────────────────
cat > "$PROJECT_DIR/src-tauri/capabilities/default.json" << 'JSON'
{
  "identifier": "default",
  "description": "Default capabilities",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-create",
    "shell:allow-open",
    "oauth:allow-start",
    "oauth:allow-cancel"
  ]
}
JSON

ok "All project files written"

# =============================================================================
# 8. CARGO LINKER CONFIG
# =============================================================================
step "Configuring Cargo linker (~/.cargo/config.toml)"
mkdir -p "$HOME/.cargo"
# Only write the MSVC xwin section if it isn't already there
if ! grep -q "x86_64-pc-windows-msvc" "$HOME/.cargo/config.toml" 2>/dev/null; then
  cat >> "$HOME/.cargo/config.toml" << 'TOML'

[target.x86_64-pc-windows-msvc]
# cargo-xwin provides the MSVC cross-compilation toolchain on Linux
TOML
  ok "Linker config updated"
else
  ok "Linker config already present"
fi

# =============================================================================
# 9. ICONS
# =============================================================================
step "Generating app icons"
if [ -f "$PROJECT_DIR/src-tauri/icons/icon.ico" ]; then
  ok "Icons already exist — skipping"
else
  # Generate a gold placeholder icon if no source icon is provided.
  # Replace source.png with your actual icon before running for production.
  SOURCE_ICON="$PROJECT_DIR/src-tauri/icons/source.png"
  convert -size 512x512 \
    radial-gradient:'#c9a84c-#8b6914' \
    -gravity center \
    -font DejaVu-Sans-Bold -pointsize 200 \
    -fill '#1a1400' -annotate 0 'RT' \
    "$SOURCE_ICON"
  cd "$PROJECT_DIR"
  cargo tauri icon "$SOURCE_ICON"
  ok "Icons generated"
fi

# =============================================================================
# 10. FRONTEND PLACEHOLDER
# =============================================================================
step "Checking frontend"
if [ -f "$PROJECT_DIR/frontend/index.html" ]; then
  ok "frontend/index.html already present"
else
  echo -e "${GOLD}  ⚠ frontend/index.html not found.${NC}"
  echo    "    Copy your index.html to:  $PROJECT_DIR/frontend/index.html"
  echo    "    Then re-run the build step below."
fi

# =============================================================================
# 11. BUILD
# =============================================================================
step "Building for Windows (x86_64-pc-windows-msvc)"
if [ ! -f "$PROJECT_DIR/frontend/index.html" ]; then
  echo -e "${GOLD}  Skipping build — frontend/index.html missing.${NC}"
  echo    "  Once you place your index.html, run:"
  echo    ""
  echo    "    cd $PROJECT_DIR/src-tauri"
  echo    "    export WEBVIEW2_SDK_DIR=$WEBVIEW2_DIR"
  echo    "    export RUSTFLAGS=\"-C target-feature=+crt-static\""
  echo    "    cargo xwin build --target x86_64-pc-windows-msvc --release"
  echo    ""
else
  cd "$PROJECT_DIR/src-tauri"
  export WEBVIEW2_SDK_DIR="$WEBVIEW2_DIR"
  export RUSTFLAGS="-C target-feature=+crt-static"
  cargo xwin build --target x86_64-pc-windows-msvc --release
  ok "Build complete"
  echo -e "\n${GREEN}  .exe location:${NC}"
  echo    "  $PROJECT_DIR/src-tauri/target/x86_64-pc-windows-msvc/release/remuneration-tracker.exe"
fi

# =============================================================================
# DONE
# =============================================================================
echo -e "\n${GREEN}═══════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  Setup complete.${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════${NC}"
echo ""
echo "  Future builds (no setup needed):"
echo ""
echo "    cd $PROJECT_DIR/src-tauri"
echo "    export WEBVIEW2_SDK_DIR=$WEBVIEW2_DIR"
echo "    export RUSTFLAGS=\"-C target-feature=+crt-static\""
echo "    cargo xwin build --target x86_64-pc-windows-msvc --release"
echo ""
