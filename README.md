# Chess Coach Utility (CCU)

A cross-platform desktop application for chess coaching, tournament
management, and player development.

## Overview

Chess Coach Utility (CCU) is a Tauri-based desktop application that
provides chess coaching tools for tournaments, players, and statistics.
The application is built on a Rust backend with a JavaScript frontend.

## Repository Structure

```
.
├── .cargo/                  Cargo configuration
├── src-tauri/               Rust Tauri backend
│   ├── Cargo.toml           Main application crate manifest
│   ├── build.rs             Build script
│   ├── tauri.conf.json      Tauri configuration
│   ├── capabilities/        Capability definitions
│   ├── permissions/         Permission schemas
│   ├── gen/schemas/         Generated JSON schemas
│   ├── icons/               Application icons
│   └── src/                 Rust source (backend glue)
├── patches/                 Cargo patches for dependencies
│   └── tauri-plugin-google-auth/
│       Local patched copy of tauri-plugin-google-auth
├── tools/                   Developer tools and utilities
│   ├── keygen/              Activation key generator (Linux)
│   ├── scripts/             Python utility scripts
│   └── cpp_utils/           C++ utility library
├── docs/                    Documentation
├── tests/                   Test suites
├── package.json             Frontend package configuration
├── vite.config.js           Vite build configuration
├── setup-ccu.sh             Development environment setup
└── rules.md                 Coding guidelines
```

> **Note:** The proprietary chess engine, AI modules, licensing logic,
> and frontend UI code are maintained in a separate private repository.
> This public repository contains only the build infrastructure,
> configuration, and developer tooling. A functional application
> cannot be built from this repository alone.

## Development

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

For building the activation key generator tool:

```bash
bash setup-ccu.sh --keygen
```

### Building

```bash
# Build the Tauri backend
cd src-tauri && cargo build

# Build the keygen tool
cd tools/keygen && make

# Build the C++ utilities
cd tools/cpp_utils && cmake . && make

# Frontend build
cd /project/root && npm run build
```

### Testing

```bash
# Run all tests
python tools/scripts/test_runner.py

# Run only Rust tests
cd src-tauri && cargo test
cd tools/keygen && cargo test

# Run specific test suites
python tools/scripts/test_runner.py --filter keygen licensing
```

## Tools

### keygen

The CCU Activation Key Generator is a Linux-only tool for issuing
activation codes. It communicates with the activation service to
register batches of codes for distribution.

```bash
cd tools/keygen && make install
ccu-keygen init
ccu-keygen status
```

See `tools/keygen/README.md` for full documentation.

### Utility Scripts

| Script | Description |
|--------|-------------|
| `tools/scripts/build_helper.py` | Version management and environment validation |
| `tools/scripts/test_runner.py` | Test suite orchestration and reporting |
| `tools/scripts/tournament_analyzer.py` | PGN tournament data analysis |
| `tools/scripts/data_migration.py` | Database schema migration tool |

### C++ Utilities

The `tools/cpp_utils/` module provides:

- **PGN Parser**: Parses and validates PGN chess game notation files
- **Binary Reader**: Reads/writes the CCUDB binary database format
- **Pairing Engine**: Swiss and round-robin tournament pairing algorithms
- **Data Compressor**: RLE, Huffman, and simple deflate compression

```bash
cd tools/cpp_utils
cmake . && make
./cgd_cli --games tournament.pgn --report
```

## License

Copyright &copy; 2026 Armiel Pillay. All rights reserved.

The Chess Coach Utility is a closed-source proprietary application.
This repository contains only non-proprietary infrastructure code,
build tooling, and configuration. The core application logic is
not distributed in this repository.
