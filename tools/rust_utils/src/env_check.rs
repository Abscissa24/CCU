use std::process::Command;

use serde::{Deserialize, Serialize};

/// Report on the development environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvReport {
    pub rustc_version: Option<String>,
    pub cargo_version: Option<String>,
    pub rustup_channels: Vec<String>,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub cmake_version: Option<String>,
    pub gcc_version: Option<String>,
    pub clang_version: Option<String>,
    pub webkit_version: Option<String>,
    pub disk_space_gb: Option<u64>,
    pub issues: Vec<String>,
}

impl EnvReport {
    /// Print the environment report to stdout.
    pub fn print(&self) {
        println!("Environment Report");
        println!("==================\n");

        println!("Rust toolchain:");
        println!("  rustc: {}", self.rustc_version.as_deref().unwrap_or("not found"));
        println!("  cargo: {}", self.cargo_version.as_deref().unwrap_or("not found"));
        println!("  channels: {}", self.rustup_channels.join(", "));

        println!("\nJavaScript tooling:");
        println!("  node: {}", self.node_version.as_deref().unwrap_or("not found"));
        println!("  npm:  {}", self.npm_version.as_deref().unwrap_or("not found"));

        println!("\nC/C++ tooling:");
        println!("  cmake:  {}", self.cmake_version.as_deref().unwrap_or("not found"));
        println!("  gcc:    {}", self.gcc_version.as_deref().unwrap_or("not found"));
        println!("  clang:  {}", self.clang_version.as_deref().unwrap_or("not found"));
        println!("  webkit: {}", self.webkit_version.as_deref().unwrap_or("not found"));

        println!("\nDisk space:");
        match self.disk_space_gb {
            Some(gb) => println!("  available: {} GB", gb),
            None => println!("  unable to determine"),
        }

        if !self.issues.is_empty() {
            println!("\nIssues:");
            for issue in &self.issues {
                println!("  - {}", issue);
            }
        } else {
            println!("\nNo issues detected.");
        }
    }

    /// Returns true if there are no blocking issues.
    pub fn all_good(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Runs environment checks and returns a report.
pub struct EnvCheck<'a> {
    project_root: &'a std::path::Path,
}

impl<'a> EnvCheck<'a> {
    pub fn new(project_root: &'a std::path::Path) -> Self {
        Self { project_root }
    }

    pub fn run(&self) -> EnvReport {
        let mut report = EnvReport {
            rustc_version: None,
            cargo_version: None,
            rustup_channels: Vec::new(),
            node_version: None,
            npm_version: None,
            cmake_version: None,
            gcc_version: None,
            clang_version: None,
            webkit_version: None,
            disk_space_gb: None,
            issues: Vec::new(),
        };

        // Rust
        report.rustc_version = get_command_version("rustc", "--version");
        report.cargo_version = get_command_version("cargo", "--version");
        report.rustup_channels = get_rustup_channels();

        if report.rustc_version.is_none() {
            report.issues.push("rustc not found — install from https://rustup.rs".to_string());
        }

        // Node
        report.node_version = get_command_version("node", "--version");
        report.npm_version = get_command_version("npm", "--version");

        if report.node_version.is_none() {
            report
                .issues
                .push("Node.js not found — install Node.js 20+".to_string());
        }

        // C/C++
        report.cmake_version = get_command_version("cmake", "--version");

        #[cfg(target_os = "linux")]
        {
            report.gcc_version = get_command_version("gcc", "--version");
            report.clang_version = get_command_version("clang", "--version");
            report.webkit_version = get_pkg_config_version("webkit2gtk-4.1");
        }

        if report.cmake_version.is_none() {
            report.issues.push("cmake not found — required for C++ utils".to_string());
        }

        // Disk space
        report.disk_space_gb = get_disk_space_gb(self.project_root);
        if report.disk_space_gb.is_some_and(|gb| gb < 5) {
            report.issues.push("Low disk space — recommended at least 5 GB free".to_string());
        }

        // Check src-tauri exists
        let src_tauri = self.project_root.join("src-tauri");
        if !src_tauri.exists() {
            report.issues.push("src-tauri/ directory not found".to_string());
        }

        // Check Cargo.toml
        let cargo_toml = src_tauri.join("Cargo.toml");
        if !cargo_toml.exists() {
            report.issues.push("src-tauri/Cargo.toml not found".to_string());
        }

        report
    }
}

/// Get the version string from a command.
fn get_command_version(cmd: &str, arg: &str) -> Option<String> {
    let output = Command::new(cmd).arg(arg).output().ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        Some(text.trim().to_string())
    } else {
        None
    }
}

/// Get a pkg-config version string for a package.
#[cfg(target_os = "linux")]
fn get_pkg_config_version(pkg: &str) -> Option<String> {
    let output = Command::new("pkg-config")
        .arg("--modversion")
        .arg(pkg)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

#[cfg(not(target_os = "linux"))]
fn get_pkg_config_version(_pkg: &str) -> Option<String> {
    None
}

/// Get available disk space in GB for a path.
fn get_disk_space_gb(path: &std::path::Path) -> Option<u64> {
    let output = Command::new("df")
        .arg("-B1")
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return None;
    }

    // Output format: Filesystem  1B-blocks  Used  Available  Use%  Mounted on
    let parts: Vec<&str> = lines[1].split_whitespace().collect();
    if parts.len() >= 4 {
        let available: u64 = parts[3].parse().unwrap_or(0);
        Some(available / 1_000_000_000)
    } else {
        None
    }
}

/// Get installed rustup channels.
fn get_rustup_channels() -> Vec<String> {
    let output = Command::new("rustup")
        .arg("show")
        .output()
        .ok();

    let output = match output {
        Some(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter_map(|line| {
            line.split_whitespace().next().map(|s| s.to_string())
        })
        .filter(|line| line.contains("stable") || line.contains("beta") || line.contains("nightly"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_report_creation() {
        let report = EnvReport {
            rustc_version: Some("rustc 1.77.2".to_string()),
            cargo_version: Some("cargo 1.77.2".to_string()),
            rustup_channels: vec!["stable".to_string()],
            node_version: Some("v20.12.0".to_string()),
            npm_version: Some("10.5.0".to_string()),
            cmake_version: Some("cmake version 3.28.3".to_string()),
            gcc_version: Some("gcc 13.2.1".to_string()),
            clang_version: Some("clang 17.0.6".to_string()),
            webkit_version: Some("2.44.1".to_string()),
            disk_space_gb: Some(120),
            issues: vec![],
        };

        assert!(report.all_good());
    }

    #[test]
    fn test_env_report_issues() {
        let report = EnvReport {
            rustc_version: None,
            cargo_version: None,
            rustup_channels: vec![],
            node_version: None,
            npm_version: None,
            cmake_version: None,
            gcc_version: None,
            clang_version: None,
            webkit_version: None,
            disk_space_gb: None,
            issues: vec!["rustc not found".to_string(), "Node.js not found".to_string()],
        };

        assert!(!report.all_good());
        assert_eq!(report.issues.len(), 2);
    }

    #[test]
    fn test_env_check() {
        let temp = tempfile::tempdir().unwrap();
        let check = EnvCheck::new(temp.path());
        let report = check.run();

        // The environment report should have been generated
        // Specific values depend on the test environment
        assert!(!report.rustup_channels.is_empty() || report.rustc_version.is_none());
    }

    #[test]
    fn test_get_command_version() {
        // Test a command that exists on the system
        let result = get_command_version("ls", "--version");
        // ls --version should exist on Linux
        #[cfg(target_os = "linux")]
        assert!(result.is_some());
    }

    #[test]
    fn test_get_command_version_missing() {
        let result = get_command_version("nonexistent_command_xyz", "--version");
        assert!(result.is_none());
    }
}
