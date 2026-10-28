<div align="center">

<img src="https://raw.githubusercontent.com/Abscissa24/CCU/main/src-tauri/icons/icon.png" alt="CCU logo" width="160" />

# Chess Coach Utility (CCU)

**Chess coaching. Tournament management. Player development.**

Cross-platform desktop tooling for chess coaches, tournament directors, and player development — built on a Rust backend with a JavaScript frontend.

<br />

![Version](https://img.shields.io/badge/version-5.1.0-e63946?style=for-the-badge)
![Platform](https://img.shields.io/badge/platform-Cross--Platform%20%7C%20Tauri-3ddc84?style=for-the-badge&logo=tauri&logoColor=white)
![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri-24c8db?style=for-the-badge&logo=tauri&logoColor=white)
![Vite](https://img.shields.io/badge/bundled%20with-Vite-646cff?style=for-the-badge&logo=vite&logoColor=white)
<br />
![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)
![Python](https://img.shields.io/badge/Python-3776ab?style=for-the-badge&logo=python&logoColor=white)
![JavaScript](https://img.shields.io/badge/JavaScript-f7df1e?style=for-the-badge&logo=javascript&logoColor=white)

<br />

[About](#-about) · [Repository Structure](#-repository-structure) · [Development](#-development) · [Tools](#-tools) · [License](#-license)

</div>

---

## 🏁 About

Chess Coach Utility (CCU) is a Tauri-based desktop application that provides chess coaching tools for tournaments, players, and statistics. The application is built on a Rust backend with a JavaScript frontend.

## 📁 Repository Structure

```
.
├── .cargo/                  Cargo configuration
├── src-tauri/               Rust Tauri backend
│   ├── Cargo.toml           Main application crate manifest
│   ├── build.rs             Build script
│   ├── tauri.conf.json      Tauri configuration
│   ├── capabilities/        Capability definitions
│   ├── permissions/         Permission schemas
│   ├── icons/               Application icons
│   └── src/                 Rust source (backend glue)
├── patches/                 Cargo patches for dependencies
│   └── tauri-plugin-google-auth/
│       Local patched copy of tauri-plugin-google-auth
├── tools/                   Developer tools and utilities
│   ├── rust_utils/          Rust build/verification utilities
│   └── scripts/             Python utility scripts
├── docs/                    Documentation
├── tests/                   Test suites
├── package.json             Frontend package configuration
├── vite.config.js           Vite build configuration
├── setup-ccu.sh             Development environment setup
└── rules.md                 Coding guidelines
```

> **Note:** The proprietary chess engine, AI modules, frontend UI code,
> and licensing logic are not included in this repository. This public
> repository contains only the build infrastructure, configuration, and
> developer tooling. A functional application cannot be built from this
> repository alone.

## 🛠️ Development

### Prerequisites

- Arch Linux (the primary development platform)
- Rust 1.77.2+
- Node.js 20+
- webkit2gtk-4.1

### Setup

Run the setup script to install all dependencies:

```bash
bash setup-ccu.sh
```

For Windows cross-compilation (requires cargo-xwin):

```bash
bash setup-ccu.sh --win
```

For Linux release bundles:

```bash
bash setup-ccu.sh --linux
```

### Building

```bash
# Build the Tauri backend
cd src-tauri && cargo check

# Build the Rust utilities
cd tools/rust_utils && cargo build

# Frontend build
cd /project/root && npm run build
```

### Testing

```bash
# Run all tests
python tools/scripts/test_runner.py

# Run only Rust tests
cd src-tauri && cargo test
cd tools/rust_utils && cargo test

# Run specific test suites
python tools/scripts/test_runner.py --filter rust
```

## 🔧 Tools

### Rust Utilities (`tools/rust_utils/`)

Build verification and environment checking utilities written in Rust.

```bash
cd tools/rust_utils && cargo run -- check
cd tools/rust_utils && cargo run -- verify --all
```

### Utility Scripts (`tools/scripts/`)

| Script | Description |
|--------|-------------|
| `build_helper.py` | Version management and environment validation |
| `test_runner.py` | Test suite orchestration and reporting |
| `tournament_analyzer.py` | PGN tournament data analysis |
| `data_migration.py` | Data schema migration tool |

## 📄 License

Copyright &copy; 2026 Armiel Pillay. All rights reserved.

The Chess Coach Utility is a closed-source proprietary application.
This repository contains only non-proprietary infrastructure code,
build tooling, and configuration. The core application logic is
not distributed in this repository.
