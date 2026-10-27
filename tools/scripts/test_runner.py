#!/usr/bin/env python3
"""
Test runner for Chess Coach Utility (CCU).

Runs the project's test suites in the correct order and aggregates results.
Supports Rust, Python, and C++ test suites alongside frontend integration tests.

This is a developer convenience script — the proprietary test fixtures
are injected at CI time and are not available in this repository.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from pathlib import Path
from typing import Optional


class TestStatus(Enum):
    PASS = "PASS"
    FAIL = "FAIL"
    SKIP = "SKIP"
    ERROR = "ERROR"


@dataclass
class TestResult:
    """Holds the result of a single test suite."""
    name: str
    status: TestStatus
    duration_ms: int
    output: str = ""
    error_message: str = ""
    test_count: int = 0
    failure_count: int = 0
    skip_count: int = 0

    def to_dict(self) -> dict:
        return {
            "name": self.name,
            "status": self.status.value,
            "duration_ms": self.duration_ms,
            "test_count": self.test_count,
            "failure_count": self.failure_count,
            "skip_count": self.skip_count,
            "output": self.output[-2000:] if self.output else "",
            "error": self.error_message,
        }


@dataclass
class TestReport:
    """Aggregated test report for all suites."""
    results: list[TestResult] = field(default_factory=list)
    start_time: str = ""
    total_duration_ms: int = 0
    all_passed: bool = True

    def add_result(self, result: TestResult) -> None:
        self.results.append(result)
        if result.status != TestStatus.PASS:
            self.all_passed = False

    def to_dict(self) -> dict:
        return {
            "start_time": self.start_time,
            "total_duration_ms": self.total_duration_ms,
            "all_passed": self.all_passed,
            "results": [r.to_dict() for r in self.results],
        }

    def save(self, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(self.to_dict(), indent=2), encoding="utf-8")


# ---------------------------------------------------------------------------
# Test suite definitions
# ---------------------------------------------------------------------------

class TestSuite:
    """Defines a test suite to run."""

    def __init__(self, name: str, command: list[str], cwd: Path,
                 env: Optional[dict] = None):
        self.name = name
        self.command = command
        self.cwd = cwd
        self.env = env or {}

    def run(self) -> TestResult:
        start = time.monotonic()
        env = os.environ.copy()
        env.update(self.env)

        try:
            result = subprocess.run(
                self.command,
                cwd=self.cwd,
                capture_output=True,
                text=True,
                timeout=300,
                env=env,
            )
            elapsed_ms = int((time.monotonic() - start) * 1000)
            output = result.stdout + result.stderr

            if result.returncode == 0:
                status = TestStatus.PASS
            else:
                status = TestStatus.FAIL

            # Try to parse test counts from cargo output
            test_count, failure_count, skip_count = self._parse_counts(output, status)

            return TestResult(
                name=self.name,
                status=status,
                duration_ms=elapsed_ms,
                output=output,
                test_count=test_count,
                failure_count=failure_count,
                skip_count=skip_count,
            )
        except subprocess.TimeoutExpired:
            elapsed_ms = int((time.monotonic() - start) * 1000)
            return TestResult(
                name=self.name,
                status=TestStatus.ERROR,
                duration_ms=elapsed_ms,
                error_message=f"Test suite timed out after 300s",
            )
        except FileNotFoundError as e:
            elapsed_ms = int((time.monotonic() - start) * 1000)
            return TestResult(
                name=self.name,
                status=TestStatus.ERROR,
                duration_ms=elapsed_ms,
                error_message=str(e),
            )

    def _parse_counts(self, output: str, status: TestStatus) -> tuple[int, int, int]:
        """Parse test counts from output. Returns (total, failures, skips)."""
        import re

        # Cargo test format: "test result: ok. 20 passed; 0 failed; 0 ignored"
        cargo_match = re.search(
            r"test result:\s+\w+\.\s+(\d+)\s+passed;\s+(\d+)\s+failed;\s+(\d+)\s+ignored",
            output
        )
        if cargo_match:
            return (int(cargo_match.group(1)),
                    int(cargo_match.group(2)),
                    int(cargo_match.group(3)))

        # Node test format varies
        if "PASS" in output and "FAIL" in output:
            passes = output.count(" PASS ")
            fails = output.count(" FAIL ")
            return (passes + fails, fails, 0)

        if status == TestStatus.PASS:
            return (1, 0, 0)
        return (0, 1, 0)


# ---------------------------------------------------------------------------
# Suite factory
# ---------------------------------------------------------------------------

def build_suites(project_root: Path, suite_filter: Optional[list[str]] = None) -> list[TestSuite]:
    """Build the list of test suites to run."""
    suites: list[TestSuite] = []

    # Rust utility tests
    rust_utils_dir = project_root / "tools" / "rust_utils"
    if rust_utils_dir.exists():
        suites.append(TestSuite(
            name="rust-utils:unit",
            command=["cargo", "test"],
            cwd=rust_utils_dir,
        ))

    # Licensing tests
    licensing_test = project_root / "tests" / "licensing.test.mjs"
    if licensing_test.exists():
        suites.append(TestSuite(
            name="licensing",
            command=["node", str(licensing_test)],
            cwd=project_root,
        ))

    # Workspace guard
    workspace_guard = project_root / "tests" / "workspace_guard.test.mjs"
    if workspace_guard.exists():
        suites.append(TestSuite(
            name="workspace-guard",
            command=["node", str(workspace_guard)],
            cwd=project_root,
        ))

    # Boot test (no animation)
    boot_test = project_root / "tests" / "boot_noanim.test.mjs"
    if boot_test.exists():
        suites.append(TestSuite(
            name="boot-noanim",
            command=["node", str(boot_test)],
            cwd=project_root,
        ))

    # Clinton AI tests
    clinton_dir = project_root / "tests" / "clinton"
    if clinton_dir.exists():
        agent_test = clinton_dir / "agent.test.mjs"
        if agent_test.exists():
            suites.append(TestSuite(
                name="clinton:agent",
                command=["node", str(agent_test)],
                cwd=project_root,
            ))

        ai_reliability = clinton_dir / "ai_reliability.test.mjs"
        if ai_reliability.exists():
            suites.append(TestSuite(
                name="clinton:ai-reliability",
                command=["node", str(ai_reliability)],
                cwd=project_root,
            ))

    # Filter suites if filter provided
    if suite_filter:
        suites = [s for s in suites if any(f in s.name for f in suite_filter)]

    return suites


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------

def print_result(result: TestResult) -> None:
    """Print a single test result to stderr."""
    symbol = {
        TestStatus.PASS: "\033[32mPASS\033[0m",
        TestStatus.FAIL: "\033[31mFAIL\033[0m",
        TestStatus.SKIP: "\033[33mSKIP\033[0m",
        TestStatus.ERROR: "\033[31mERROR\033[0m",
    }[result.status]

    duration_str = f"{result.duration_ms}ms"
    if result.duration_ms > 1000:
        duration_str = f"{result.duration_ms / 1000:.1f}s"

    name_str = f"  {result.name:<30}"
    status_str = f"[{symbol}] ({duration_str})"
    counts_str = f"  {result.test_count} ran, {result.failure_count} failed, {result.skip_count} skipped"

    print(f"{name_str} {status_str}")
    if result.failure_count > 0 or result.status == TestStatus.FAIL:
        print(f"    {counts_str}")
        if result.error_message:
            print(f"    Error: {result.error_message}")
        # Print last few lines of output
        if result.output:
            tail = result.output.strip().split("\n")[-5:]
            for line in tail:
                print(f"    | {line}")
    elif result.status == TestStatus.PASS:
        print(f"    {counts_str}")

    if result.status == TestStatus.SKIP:
        print(f"    Skipped: {result.error_message}")


def print_summary(report: TestReport) -> None:
    """Print the final summary."""
    separator = "=" * 60

    print(f"\n{separator}")
    print("Test Summary")
    print(separator)

    total_tests = sum(r.test_count for r in report.results)
    total_failures = sum(r.failure_count for r in report.results)
    total_skips = sum(r.skip_count for r in report.results)

    print(f"  Total suites: {len(report.results)}")
    print(f"  Total tests:  {total_tests}")
    print(f"  Failures:     {total_failures}")
    print(f"  Skipped:      {total_skips}")
    print(f"  Duration:     {report.total_duration_ms}ms")

    passed = sum(1 for r in report.results if r.status == TestStatus.PASS)
    failed = sum(1 for r in report.results if r.status == TestStatus.FAIL)
    errored = sum(1 for r in report.results if r.status == TestStatus.ERROR)
    skipped = sum(1 for r in report.results if r.status == TestStatus.SKIP)

    print(f"\n  Suites: {passed} passed, {failed} failed, {errored} errored, {skipped} skipped")
    print(f"  Overall: {'ALL PASSED' if report.all_passed else 'FAILURES DETECTED'}")
    print(separator)


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run CCU test suites"
    )
    parser.add_argument(
        "--filter", "-f",
        nargs="*",
        default=None,
        help="Filter test suites by name substring"
    )
    parser.add_argument(
        "--report", "-r",
        type=str,
        default=None,
        help="Write JSON report to this path"
    )
    parser.add_argument(
        "--list", "-l",
        action="store_true",
        help="List available test suites without running them"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Print full output for each suite"
    )

    args = parser.parse_args()

    project_root = Path(__file__).resolve().parent.parent.parent

    suites = build_suites(project_root, args.filter)

    if args.list:
        print("Available test suites:")
        for s in suites:
            print(f"  {s.name}")
        return 0

    if not suites:
        print("No test suites found matching the filter.")
        return 1

    print(f"Running {len(suites)} test suite(s)...")
    print()

    report = TestReport(start_time=datetime.now(timezone.utc).isoformat())

    for suite in suites:
        result = suite.run()
        report.add_result(result)
        print_result(result)

        if args.verbose and result.output:
            print(f"    Output:")
            for line in result.output.strip().split("\n")[-10:]:
                print(f"    | {line}")
        print()

    report.total_duration_ms = sum(r.duration_ms for r in report.results)

    print_summary(report)

    if args.report:
        report_path = Path(args.report)
        report.save(report_path)
        print(f"\nReport saved to: {report_path}")

    return 0 if report.all_passed else 1


if __name__ == "__main__":
    sys.exit(main())
