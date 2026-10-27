#!/usr/bin/env python3
"""
Data migration utility for Chess Coach Utility (CCU).

Handles schema migrations for the local database and settings files.
Reads the old format, applies transformations, and writes the new format.
Backs up the original file before overwriting.

Usage:
    python data_migration.py --migrate <file> [--check] [--rollback]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import tempfile
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any, Optional


MIGRATION_SCHEMAS: dict[int, dict[str, Any]] = {
    1: {
        "name": "initial",
        "tables": ["players", "tournaments", "pairings", "settings"],
        "version_key": "schema_version",
    },
    2: {
        "name": "add_rating_fields",
        "changes": [
            "Add rating_history array to each player",
            "Add fide_id field to players",
            "Add time_control field to tournaments",
        ],
        "version_key": "schema_version",
    },
    3: {
        "name": "school_district_split",
        "changes": [
            "Move school district from free-text to enum",
            "Add school_code derived from SHA-256(school_name)",
            "Normalize school names (whitespace, casing)",
        ],
        "version_key": "schema_version",
    },
    4: {
        "name": "offline_licence_format",
        "changes": [
            "Licence objects now include expiry_ms and device_binding",
            "Add signature algorithm field (Ed25519)",
            "Remove deprecated symmetric keys",
        ],
        "version_key": "schema_version",
    },
}


@dataclass
class Migration:
    """Represents a single migration step."""
    from_version: int
    to_version: int
    name: str
    description: str
    transform: str  # name of the method to call


class Migrator:
    """Applies schema migrations to JSON data files."""

    def __init__(self, data: dict[str, Any], verbose: bool = False):
        self.data = data
        self.verbose = verbose
        self.changes: list[str] = []

    def log(self, message: str) -> None:
        if self.verbose:
            print(f"  [migrate] {message}")
        self.changes.append(message)

    def get_version(self) -> int:
        return self.data.get("schema_version", 1)

    def set_version(self, version: int) -> None:
        self.data["schema_version"] = version
        self.log(f"Schema version set to {version}")

    def migrate_to(self, target_version: int) -> bool:
        """Apply migrations from current version to target version."""
        current = self.get_version()
        if current == target_version:
            self.log(f"Already at target version {target_version}")
            return True

        if current > target_version:
            self.log(f"Downgrade requested: {current} -> {target_version}")
            return self.rollback_to(target_version)

        migrations: list[tuple[int, int, str]] = []
        for v in range(current, target_version):
            migrations.append((v, v + 1, MIGRATION_SCHEMAS.get(v + 1, {}).get("name", f"v{v+1}")))

        for from_v, to_v, name in migrations:
            self.log(f"Migrating {from_v} -> {to_v} ({name})")
            method_name = f"_migrate_{from_v}_to_{to_v}"
            method = getattr(self, method_name, None)
            if method is None:
                self.log(f"  No explicit migration method for {from_v} -> {to_v}, skipping")
                self.set_version(to_v)
                continue

            try:
                method()
                self.set_version(to_v)
            except Exception as e:
                self.log(f"  ERROR during migration: {e}")
                return False

        return True

    def rollback_to(self, target_version: int) -> bool:
        """Rollback to a previous version."""
        current = self.get_version()
        self.log(f"Rolling back {current} -> {target_version}")

        for v in range(current, target_version, -1):
            method_name = f"_rollback_{v}_to_{v-1}"
            method = getattr(self, method_name, None)
            if method is None:
                self.log(f"  No rollback method for v{v}, setting version only")
                self.set_version(v - 1)
                continue

            try:
                method()
                self.set_version(v - 1)
            except Exception as e:
                self.log(f"  ERROR during rollback: {e}")
                return False

        return True

    # --- Individual migrations ---

    def _migrate_1_to_2(self) -> None:
        """Add rating history and FIDE ID to players."""
        players = self.data.get("players", [])
        for player in players:
            if "rating_history" not in player:
                player["rating_history"] = [
                    {"rating": player.get("rating", 1200), "date": player.get("created_at", "")}
                ]
            if "fide_id" not in player:
                player["fide_id"] = None

        tournaments = self.data.get("tournaments", [])
        for t in tournaments:
            if "time_control" not in t:
                t["time_control"] = "standard"

        self.log(f"Updated {len(players)} players with rating_history")

    def _migrate_2_to_3(self) -> None:
        """School district split and normalization."""
        import hashlib

        schools = self.data.get("schools", [])
        for school in schools:
            name = school.get("name", "")
            canonical = " ".join(name.split()).strip().lower()
            school_code = hashlib.sha256(canonical.encode()).hexdigest()[:6].upper()
            school["school_code"] = school_code
            school["name_normalized"] = canonical

        players = self.data.get("players", [])
        for player in players:
            if "school" in player and isinstance(player["school"], str):
                canonical = " ".join(player["school"].split()).strip().lower()
                player["school_normalized"] = canonical

        self.log(f"Updated {len(schools)} schools with school_code")

    def _migrate_3_to_4(self) -> None:
        """Update offline licence format."""
        licences = self.data.get("licences", [])
        for licence in licences:
            if "expiry_ms" not in licence:
                expiry_days = licence.get("expiry_days", 30)
                issued_at = licence.get("issued_at", 0)
                licence["expiry_ms"] = issued_at + (expiry_days * 86_400_000)

            if "device_binding" not in licence:
                licence["device_binding"] = {
                    "device_id": licence.get("device_id", ""),
                    "bound_at": issued_at,
                }

            if "signature_algorithm" not in licence:
                licence["signature_algorithm"] = "Ed25519"

        self.log(f"Updated {len(licences)} licences to offline format")

    def _rollback_4_to_3(self) -> None:
        """Remove offline licence fields."""
        licences = self.data.get("licences", [])
        for licence in licences:
            licence.pop("expiry_ms", None)
            licence.pop("device_binding", None)
            licence.pop("signature_algorithm", None)

        self.log(f"Rolled back {len(licences)} licences")

    def _rollback_3_to_2(self) -> None:
        """Remove school_code and normalization."""
        schools = self.data.get("schools", [])
        for school in schools:
            school.pop("school_code", None)
            school.pop("name_normalized", None)

        players = self.data.get("players", [])
        for player in players:
            player.pop("school_normalized", None)

        self.log("Removed school_code and normalization fields")

    def _rollback_2_to_1(self) -> None:
        """Remove rating history and FIDE ID."""
        players = self.data.get("players", [])
        for player in players:
            player.pop("rating_history", None)
            player.pop("fide_id", None)

        tournaments = self.data.get("tournaments", [])
        for t in tournaments:
            t.pop("time_control", None)

        self.log("Removed rating_history and fide_id fields")


class BackupManager:
    """Manages backup files during migration."""

    def __init__(self, backup_dir: Path):
        self.backup_dir = backup_dir
        self.backup_dir.mkdir(parents=True, exist_ok=True)

    def backup(self, file_path: Path) -> Path:
        """Create a timestamped backup of a file."""
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        backup_name = f"{file_path.stem}_{timestamp}{file_path.suffix}"
        backup_path = self.backup_dir / backup_name
        shutil.copy2(file_path, backup_path)
        return backup_path

    def list_backups(self, file_name: str) -> list[Path]:
        """List all backups for a given file name."""
        stem = Path(file_name).stem
        return sorted(
            self.backup_dir.glob(f"{stem}_*{Path(file_name).suffix}"),
            key=lambda p: p.stat().st_mtime,
            reverse=True,
        )

    def restore(self, backup_path: Path, target_path: Path) -> None:
        """Restore a file from backup."""
        shutil.copy2(backup_path, target_path)


def check_schema(file_path: Path) -> int:
    """Check the current schema version of a data file."""
    data = json.loads(file_path.read_text(encoding="utf-8"))
    version = data.get("schema_version", 1)
    latest = max(MIGRATION_SCHEMAS.keys())

    print(f"File: {file_path}")
    print(f"Current schema version: {version}")
    print(f"Latest schema version: {latest}")

    if version < latest:
        print(f"\nMigration available: v{version} -> v{latest}")
        for v in range(version + 1, latest + 1):
            schema = MIGRATION_SCHEMAS.get(v, {})
            print(f"  v{v}: {schema.get('name', 'unknown')}")
            for change in schema.get("changes", []):
                print(f"    - {change}")
    elif version == latest:
        print("\nSchema is up to date.")
    else:
        print(f"\nWARNING: Schema version {version} is ahead of latest known ({latest})")

    return version


def migrate_file(file_path: Path, target_version: int, verbose: bool = False) -> bool:
    """Migrate a single data file to the target version."""
    if not file_path.exists():
        print(f"Error: File not found: {file_path}")
        return False

    backup_mgr = BackupManager(file_path.parent / ".backups")
    backup_path = backup_mgr.backup(file_path)
    print(f"  Backup created: {backup_path.name}")

    try:
        data = json.loads(file_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON in {file_path}: {e}")
        return False

    migrator = Migrator(data, verbose=verbose)
    success = migrator.migrate_to(target_version)

    if success:
        output_path = file_path.parent / f"{file_path.stem}.migrated{file_path.suffix}"
        output_path.write_text(json.dumps(data, indent=2), encoding="utf-8")
        print(f"  Migrated file written: {output_path}")
        print(f"  Changes: {len(migrator.changes)}")
        return True
    else:
        print("  Migration FAILED — restoring from backup")
        backup_mgr.restore(backup_path, file_path)
        return False


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Data migration utility for CCU"
    )
    parser.add_argument(
        "--migrate",
        type=Path,
        help="Migrate a data file to the latest schema",
    )
    parser.add_argument(
        "--check",
        type=Path,
        help="Check the schema version of a data file",
    )
    parser.add_argument(
        "--rollback",
        type=Path,
        help="Rollback a data file to a previous version",
    )
    parser.add_argument(
        "--target-version",
        type=int,
        default=max(MIGRATION_SCHEMAS.keys()),
        help="Target schema version (default: latest)",
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Show detailed migration log",
    )

    args = parser.parse_args()

    if args.check:
        return 0 if check_schema(args.check) else 1

    if args.rollback:
        print(f"Rolling back: {args.rollback}")
        data = json.loads(args.rollback.read_text(encoding="utf-8"))
        migrator = Migrator(data, verbose=args.verbose)
        success = migrator.rollback_to(args.target_version)
        if success:
            output = args.rollback.parent / f"{args.rollback.stem}.rolledback{args.rollback.suffix}"
            output.write_text(json.dumps(data, indent=2), encoding="utf-8")
            print(f"  Rollback file written: {output}")
            return 0
        return 1

    if args.migrate:
        print(f"Migrating: {args.migrate} -> v{args.target_version}")
        return 0 if migrate_file(args.migrate, args.target_version, args.verbose) else 1

    parser.print_help()
    return 1


if __name__ == "__main__":
    sys.exit(main())
