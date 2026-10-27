use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use ccu_utils::{
    BuildConfig, BuildStatus, BuildVerifier, DataValidator,
    EnvCheck, ValidationResult,
};

/// CCU development utilities — build verification, environment checks,
/// and data validation for the Chess Coach Utility project.
#[derive(Parser)]
#[command(
    name = "ccu-utils",
    author = "Abscissa24",
    about = "Development utilities for the Chess Coach Utility project"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run all build verification checks
    Check,

    /// Verify the development environment
    #[command(alias = "env")]
    Env,

    /// Verify build outputs against expected configuration
    Verify {
        /// Check all configured components
        #[arg(long)]
        all: bool,
        /// Check only the Rust backend
        #[arg(long)]
        rust: bool,
        /// Check only the frontend
        #[arg(long)]
        frontend: bool,
        /// Check only the C++ utils
        #[arg(long)]
        cpp: bool,
    },

    /// Validate data files (PGN, CCUDB, JSON)
    #[command(alias = "validate")]
    Data {
        /// Path to the file or directory to validate
        path: PathBuf,
    },

    /// Compute checksums for build artifacts
    Checksums {
        /// Directory to scan for artifacts
        dir: PathBuf,
        /// Hash algorithm to use (sha256 or sha512)
        #[arg(short, long, default_value = "sha256")]
        algo: String,
    },

    /// Print version information
    Version,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
    pub path: String,
    pub available: bool,
}

/// Entry point — parse CLI args and dispatch.
pub fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check => run_check(),
        Commands::Env => run_env(),
        Commands::Verify { all, rust, frontend, cpp } => {
            run_verify(all, rust, frontend, cpp)
        }
        Commands::Data { path } => run_data_validate(&path),
        Commands::Checksums { dir, algo } => run_checksums(&dir, &algo),
        Commands::Version => run_version(),
    }
}

fn run_check() -> Result<i32, Box<dyn std::error::Error>> {
    println!("CCU Build Verification Checklist");
    println!("==================================\n");

    let results = run_all_checks();
    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();

    for result in &results {
        let status = if result.passed { "PASS" } else { "FAIL" };
        println!("  [{}] {}", status, result.name);
        if !result.passed {
            println!("        {}", result.message);
        }
    }

    println!("\n{}/{} checks passed", passed, total);

    if passed == total {
        println!("\nAll checks passed — build is ready.");
        Ok(0)
    } else {
        println!("\nSome checks failed.");
        Ok(1)
    }
}

fn run_env() -> Result<i32, Box<dyn std::error::Error>> {
    let report = env::var_os("CCU_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        });

    let env_report = EnvCheck::new(&report).run();
    env_report.print();

    if env_report.all_good() {
        Ok(0)
    } else {
        Ok(1)
    }
}

fn run_verify(
    all: bool,
    rust: bool,
    frontend: bool,
    cpp: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    let config = BuildConfig { check_rust: rust || all, check_frontend: frontend || all, check_cpp: cpp || all };

    let verifier = BuildVerifier::new(config);
    let status = verifier.verify()?;

    match status {
        BuildStatus::Ok => {
            println!("Build verification passed.");
            Ok(0)
        }
        BuildStatus::Mismatch { details } => {
            println!("Build verification mismatch:");
            println!("{}", details);
            Ok(1)
        }
        BuildStatus::Missing { details } => {
            println!("Build outputs missing:");
            println!("{}", details);
            Ok(1)
        }
    }
}

fn run_data_validate(path: &Path) -> Result<i32, Box<dyn std::error::Error>> {
    let validator = DataValidator::new();
    let results = validator.validate_path(path)?;

    let mut had_error = false;
    for result in results {
        match result {
            ValidationResult::Valid { path, format, size } => {
                println!("  [VALID]   {:?} ({} bytes, {})", path, size, format);
            }
            ValidationResult::Invalid { path, format, error } => {
                println!("  [INVALID] {:?} ({})", path, format);
                println!("            {}", error);
                had_error = true;
            }
            ValidationResult::Skipped { path, reason } => {
                println!("  [SKIP]    {:?} ({})", path, reason);
            }
        }
    }

    if had_error {
        println!("\nValidation errors found.");
        Ok(1)
    } else {
        println!("\nAll files valid.");
        Ok(0)
    }
}

fn run_checksums(dir: &Path, algo: &str) -> Result<i32, Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256, Sha512};
    use std::collections::BTreeMap;

    let mut checksums: BTreeMap<String, String> = BTreeMap::new();

    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let contents = std::fs::read(path)?;

        let hash = match algo {
            "sha256" => {
                let mut hasher = Sha256::new();
                hasher.update(&contents);
                hex::encode(hasher.finalize())
            }
            "sha512" => {
                let mut hasher = Sha512::new();
                hasher.update(&contents);
                hex::encode(hasher.finalize())
            }
            _ => {
                eprintln!("Unknown algorithm: {}", algo);
                return Ok(1);
            }
        };

        let name = path.strip_prefix(dir).unwrap().display().to_string();
        checksums.insert(name, hash);
    }

    println!("{} checksums ({}):", algo, checksums.len());
    println!("{}", "-".repeat(algo.len() + 12 + checksums.len().to_string().len()));
    for (name, hash) in &checksums {
        println!("{}  {}", hash, name);
    }

    Ok(0)
}

fn run_version() -> Result<i32, Box<dyn std::error::Error>> {
    println!("ccu-utils 0.1.0");
    println!("Built with Rust {}", env!("CARGO_PKG_RUST_VERSION"));
    Ok(0)
}

// ---------------------------------------------------------------------------
// Internal check result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CheckResult {
    name: String,
    passed: bool,
    message: String,
}

fn run_all_checks() -> Vec<CheckResult> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let mut results = Vec::new();

    // Check 1: Cargo workspace
    let cargo_manifest = project_root.join("src-tauri").join("Cargo.toml");
    results.push(CheckResult {
        name: "Cargo manifest exists".to_string(),
        passed: cargo_manifest.exists(),
        message: format!("Expected: {}", cargo_manifest.display()),
    });

    // Check 2: package.json exists
    let package_json = project_root.join("package.json");
    results.push(CheckResult {
        name: "package.json exists".to_string(),
        passed: package_json.exists(),
        message: format!("Expected: {}", package_json.display()),
    });

    // Check 3: Rust toolchain
    let rustc = Command::new("rustc")
        .arg("--version")
        .output();
    results.push(CheckResult {
        name: "Rust toolchain available".to_string(),
        passed: rustc.is_ok() && rustc.as_ref().unwrap().status.success(),
        message: match rustc {
            Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            Err(e) => e.to_string(),
        },
    });

    // Check 4: Node.js
    let node = Command::new("node")
        .arg("--version")
        .output();
    results.push(CheckResult {
        name: "Node.js available".to_string(),
        passed: node.is_ok() && node.as_ref().unwrap().status.success(),
        message: match node {
            Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            Err(e) => e.to_string(),
        },
    });

    // Check 5: C++ utilities directory
    let cpp_dir = project_root.join("tools").join("cpp_utils");
    results.push(CheckResult {
        name: "C++ utilities present".to_string(),
        passed: cpp_dir.exists(),
        message: format!("Expected: {}", cpp_dir.display()),
    });

    // Check 6: tests directory
    let tests_dir = project_root.join("tests");
    results.push(CheckResult {
        name: "Tests directory exists".to_string(),
        passed: tests_dir.exists(),
        message: format!("Expected: {}", tests_dir.display()),
    });

    results
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(2);
        }
    }
}
