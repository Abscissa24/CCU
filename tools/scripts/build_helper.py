#!/usr/bin/env python3
"""
Build helper for Chess Coach Utility (CCU).

This module centralises version management, asset preparation, and
environment validation used by the Makefile / CI pipeline.  It is
intentionally lightweight and has no third-party dependencies so that it
can run in any POSIX environment with Python 3.9+.

The functions here support the Tauri build (`cargo tauri build`) and the
Vite frontend (`npm run build`).  The proprietary chess engine, AI modules,
and licensing code are NOT part of this repository — they are injected at
release time by the internal build system.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional


@dataclass
class VersionInfo:
    """Parsed semantic version information for CCU."""
    major: int
    minor: int
    patch: int
    pre_release: Optional[str] = None
    build_metadata: Optional[str] = None

    @classmethod
    def parse(cls, version_str: str) -> "VersionInfo":
        """Parse a semver string like '5.1.0' or '5.1.0-beta.1'."""
        # Strip leading 'v' if present
        if version_str.startswith("v"):
            version_str = version_str[1:]

        parts = version_str.split("-", 1)
        main_parts = parts[0].split(".")
        if len(main_parts) != 3:
            raise ValueError(f"Invalid semver: {version_str}")

        major = int(main_parts[0])
        minor = int(main_parts[1])
        patch = int(main_parts[2])

        pre_release = None
        build_metadata = None
        if len(parts) > 1:
            pre_release = parts[1]

        if "+" in version_str:
            base, build_metadata = version_str.rsplit("+", 1)

        return cls(major, minor, patch, pre_release, build_metadata)

    def to_string(self) -> str:
        """Reconstruct the semver string."""
        result = f"{self.major}.{self.minor}.{self.patch}"
        if self.pre_release:
            result += f"-{self.pre_release}"
        if self.build_metadata:
            result += f"+{self.build_metadata}"
        return result

    def next_patch(self) -> "VersionInfo":
        return VersionInfo(self.major, self.minor, self.patch + 1)

    def next_minor(self) -> "VersionInfo":
        return VersionInfo(self.major, self.minor + 1, 0)

    def next_major(self) -> "VersionInfo":
        return VersionInfo(self.major + 1, 0, 0)

    @property
    def is_stable(self) -> bool:
        return self.pre_release is None


# ---------------------------------------------------------------------------
# Environment validation
# ---------------------------------------------------------------------------

REQUIRED_TOOLS = {
    "cargo": "Rust toolchain (rustup)",
    "node": "Node.js",
    "npm": "npm",
    "cargo-tauri": "Tauri CLI",
}


def check_environment() -> bool:
    """Verify that all required build tools are available on PATH."""
    missing = []
    for tool, description in REQUIRED_TOOLS.items():
        path = shutil.which(tool)
        if path is None:
            missing.append(f"  {tool} ({description}) — not found on PATH")
            continue
        try:
            result = subprocess.run(
                [tool, "--version"],
                capture_output=True,
                text=True,
                timeout=10,
            )
            print(f"  [OK] {tool}: {result.stdout.strip()}")
        except (subprocess.TimeoutExpired, FileNotFoundError):
            missing.append(f"  {tool} ({description}) — version check failed")

    if missing:
        print("\nMissing required tools:")
        for m in missing:
            print(m)
        return False

    # Check for WebRender / WebKitGTK on Linux
    if sys.platform.startswith("linux"):
        webkit = shutil.which("pkg-config")
        if webkit:
            try:
                result = subprocess.run(
                    ["pkg-config", "--exists", "webkit2gtk-4.1"],
                    capture_output=True,
                    timeout=10,
                )
                if result.returncode == 0:
                    print("  [OK] webkit2gtk-4.1 (Linux webview dependency)")
                else:
                    print("  [WARN] webkit2gtk-4.1 not found — Linux build may fail")
            except (subprocess.TimeoutExpired, FileNotFoundError):
                pass

    return len(missing) == 0


# ---------------------------------------------------------------------------
# Asset preparation
# ---------------------------------------------------------------------------

KNOWN_ICONS = [
    "32x32.png",
    "128x128.png",
    "128x128@2x.png",
    "icon.icns",
    "icon.ico",
]

KNOWN_ASSET_DIRS = [
    "public/assets/icons",
    "public/assets/contributors",
    "public/assets/media",
]


def prepare_assets(src_dir: Path, dest_dir: Path) -> None:
    """
    Copy frontend assets from src_dir to dest_dir.

    In the open-source repository only the structural assets (icons,
    generic SVG icons) are included.  School logos and contributor photos
    are added by the private asset pipeline at release time.
    """
    dest_dir.mkdir(parents=True, exist_ok=True)

    for icon in KNOWN_ICONS:
        src = src_dir / "src-tauri" / "icons" / icon
        dest = dest_dir / "icons" / icon
        dest.parent.mkdir(parents=True, exist_ok=True)
        if src.exists():
            shutil.copy2(src, dest)
            print(f"  Copied icon: {icon}")
        else:
            print(f"  [WARN] Missing icon: {icon}")

    # Copy generic icons (not school-specific assets)
    generic_icons = ["github.svg", "google.svg", "apple.svg"]
    for icon_name in generic_icons:
        src = src_dir / "public" / "assets" / "icons" / icon_name
        if src.exists():
            dest = dest_dir / "assets" / "icons" / icon_name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dest)
            print(f"  Copied generic icon: {icon_name}")


# ---------------------------------------------------------------------------
# Version management
# ---------------------------------------------------------------------------

def read_tauri_version(tauri_conf_path: Path) -> VersionInfo:
    """Read the version from tauri.conf.json."""
    with open(tauri_conf_path, "r", encoding="utf-8") as f:
        config = json.load(f)
    return VersionInfo.parse(config.get("version", "0.0.0"))


def read_cargo_version(cargo_toml_path: Path) -> Optional[str]:
    """Read the version from Cargo.toml."""
    content = cargo_toml_path.read_text(encoding="utf-8")
    for line in content.splitlines():
        line = line.strip()
        if line.startswith("version"):
            # Simple parsing without toml dependency
            parts = line.split("=", 1)
            if len(parts) == 2:
                return parts[1].strip().strip('"')
    return None


def update_version(version: VersionInfo, project_root: Path) -> None:
    """Update the version in tauri.conf.json and Cargo.toml."""
    tauri_conf = project_root / "src-tauri" / "tauri.conf.json"
    if tauri_conf.exists():
        config = json.loads(tauri_conf.read_text(encoding="utf-8"))
        config["version"] = version.to_string()
        tauri_conf.write_text(json.dumps(config, indent=2) + "\n", encoding="utf-8")
        print(f"  Updated tauri.conf.json version to {version.to_string()}")

    # Update Cargo.toml
    cargo_toml = project_root / "src-tauri" / "Cargo.toml"
    if cargo_toml.exists():
        content = cargo_toml.read_text(encoding="utf-8")
        # Simple line-by-line replacement
        lines = content.splitlines()
        for i, line in enumerate(lines):
            if line.strip().startswith("version"):
                lines[i] = f'version = "{version.to_string()}"'
        cargo_toml.write_text("\n".join(lines), encoding="utf-8")
        print(f"  Updated Cargo.toml version to {version.to_string()}")


# ---------------------------------------------------------------------------
# Build information
# ---------------------------------------------------------------------------

def generate_build_info(project_root: Path, output_path: Path) -> None:
    """
    Generate a build info JSON file with build metadata.

    This is consumed by the app at startup to display version information
    and build timestamp.
    """
    tauri_conf = project_root / "src-tauri" / "tauri.conf.json"
    version = read_tauri_version(tauri_conf)

    build_info = {
        "version": version.to_string(),
        "build_time": datetime.now(timezone.utc).isoformat(),
        "git_commit": get_git_hash(project_root),
        "git_branch": get_git_branch(project_root),
        "rust_version": get_rust_version(),
        "node_version": sys.version,
        "platform": sys.platform,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(build_info, indent=2), encoding="utf-8")
    print(f"  Build info written to {output_path}")


def get_git_hash(project_root: Path) -> str:
    """Get the current git commit hash, or 'unknown' if not in a git repo."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            cwd=project_root,
            timeout=10,
        )
        if result.returncode == 0:
            return result.stdout.strip()[:12]
    except (subprocess.TimeoutExpired, FileNotFoundError):
        pass
    return "unknown"


def get_git_branch(project_root: Path) -> str:
    """Get the current git branch name."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            capture_output=True,
            text=True,
            cwd=project_root,
            timeout=10,
        )
        if result.returncode == 0:
            return result.stdout.strip()
    except (subprocess.TimeoutExpired, FileNotFoundError):
        pass
    return "unknown"


def get_rust_version() -> str:
    """Get the installed Rust version."""
    rustc = shutil.which("rustc")
    if rustc:
        try:
            result = subprocess.run(
                [rustc, "--version"],
                capture_output=True,
                text=True,
                timeout=10,
            )
            if result.returncode == 0:
                return result.stdout.strip()
        except (subprocess.TimeoutExpired, FileNotFoundError):
            pass
    return "unknown"


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

def main() -> int:
    """Main CLI entry point."""
    if len(sys.argv) < 2:
        print("Usage: build_helper.py <command> [args]")
        print("Commands:")
        print("  check-env        Verify build environment")
        print("  version          Print current version")
        print("  bump <part>      Bump version (major|minor|patch)")
        print("  build-info       Generate build info JSON")
        print("  prepare-assets   Copy assets to build directory")
        return 1

    project_root = Path(__file__).resolve().parent.parent.parent.parent

    command = sys.argv[1]

    if command == "check-env":
        ok = check_environment()
        return 0 if ok else 1

    elif command == "version":
        tauri_conf = project_root / "src-tauri" / "tauri.conf.json"
        version = read_tauri_version(tauri_conf)
        print(f"CCU v{version.to_string()} ({'stable' if version.is_stable else 'pre-release'})")
        return 0

    elif command == "bump":
        if len(sys.argv) < 3:
            print("Usage: bump <major|minor|patch>")
            return 1

        part = sys.argv[2]
        tauri_conf = project_root / "src-tauri" / "tauri.conf.json"
        version = read_tauri_version(tauri_conf)

        if part == "major":
            new_version = version.next_major()
        elif part == "minor":
            new_version = version.next_minor()
        elif part == "patch":
            new_version = version.next_patch()
        else:
            print(f"Unknown version part: {part}")
            return 1

        update_version(new_version, project_root)
        print(f"Bumped version to {new_version.to_string()}")
        return 0

    elif command == "build-info":
        output = project_root / "target" / "build-info.json"
        generate_build_info(project_root, output)
        return 0

    elif command == "prepare-assets":
        src = project_root
        dest = project_root / "dist" / "assets"
        prepare_assets(src, dest)
        return 0

    else:
        print(f"Unknown command: {command}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
