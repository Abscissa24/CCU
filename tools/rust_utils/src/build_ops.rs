use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

/// Build configuration — which components to verify.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub check_rust: bool,
    pub check_frontend: bool,
    pub check_cpp: bool,
}

/// Status returned by build verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildStatus {
    Ok,
    Mismatch { details: String },
    Missing { details: String },
}

/// Handles build output verification against expected configuration.
pub struct BuildVerifier {
    config: BuildConfig,
}

impl BuildVerifier {
    pub fn new(config: BuildConfig) -> Self {
        Self { config }
    }

    /// Run verification and return the build status.
    pub fn verify(&self) -> Result<BuildStatus, BuildError> {
        let project_root = self.find_project_root();
        let mut mismatches = Vec::new();
        let mut missing = Vec::new();

        if self.config.check_rust {
            self.verify_rust(&project_root, &mut mismatches, &mut missing)?;
        }

        if self.config.check_frontend {
            self.verify_frontend(&project_root, &mut mismatches, &mut missing)?;
        }

        if self.config.check_cpp {
            self.verify_cpp(&project_root, &mut mismatches, &mut missing)?;
        }

        if !missing.is_empty() {
            return Ok(BuildStatus::Missing {
                details: missing.join("\n"),
            });
        }

        if !mismatches.is_empty() {
            return Ok(BuildStatus::Mismatch {
                details: mismatches.join("\n"),
            });
        }

        Ok(BuildStatus::Ok)
    }

    fn find_project_root(&self) -> PathBuf {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let parent = manifest.parent().and_then(|p| p.parent());
        parent.unwrap_or(manifest).to_path_buf()
    }

    fn verify_rust(
        &self,
        project_root: &Path,
        mismatches: &mut Vec<String>,
        missing: &mut Vec<String>,
    ) -> Result<(), BuildError> {
        let src_tauri = project_root.join("src-tauri");
        let cargo_toml = src_tauri.join("Cargo.toml");

        if !cargo_toml.exists() {
            missing.push(format!("Rust manifest missing: {}", cargo_toml.display()));
            return Ok(());
        }

        // Check that Cargo.lock is present in a release build
        let lock_file = src_tauri.join("Cargo.lock");
        if !lock_file.exists() {
            mismatches.push("Cargo.lock not found — run `cargo generate-lockfile`".to_string());
        }

        // Check target directory for release artifacts
        let release_dir = src_tauri.join("target").join("release");
        if release_dir.exists() {
            log_debug(&format!("Release directory found: {}", release_dir.display()));
        } else {
            log_debug("No release build found — run `cargo build --release`");
        }

        // Verify the crate compiles
        let start = Instant::now();
        let output = std::process::Command::new("cargo")
            .arg("check")
            .current_dir(&src_tauri)
            .output()?;

        let elapsed = start.elapsed();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            mismatches.push(format!(
                "Cargo check failed ({:?}): {}",
                elapsed, stderr.lines().last().unwrap_or("unknown error")
            ));
        } else {
            log_debug(&format!("Cargo check succeeded in {:?}", elapsed));
        }

        Ok(())
    }

    fn verify_frontend(
        &self,
        project_root: &Path,
        mismatches: &mut Vec<String>,
        _missing: &mut Vec<String>,
    ) -> Result<(), BuildError> {
        let package_json = project_root.join("package.json");
        if !package_json.exists() {
            _missing.push(format!("package.json missing: {}", package_json.display()));
            return Ok(());
        }

        // Check that node_modules exists
        let node_modules = project_root.join("node_modules");
        if !node_modules.exists() {
            mismatches.push("node_modules not found — run `npm ci`".to_string());
        }

        // Check for dist directory (production build output)
        let dist_dir = project_root.join("dist");
        if !dist_dir.exists() {
            mismatches.push("dist/ not found — run `npm run build` for production build".to_string());
        }

        // Verify package.json is valid JSON
        let contents = std::fs::read_to_string(&package_json)?;
        let parsed: serde_json::Value = serde_json::from_str(&contents)?;
        log_debug(&format!(
            "package.json version: {}",
            parsed.get("version").unwrap_or(&serde_json::Value::Null)
        ));

        Ok(())
    }

    fn verify_cpp(
        &self,
        project_root: &Path,
        mismatches: &mut Vec<String>,
        _missing: &mut Vec<String>,
    ) -> Result<(), BuildError> {
        let cpp_dir = project_root.join("tools").join("cpp_utils");
        let makefile = cpp_dir.join("Makefile");

        if !makefile.exists() {
            _missing.push(format!("C++ Makefile missing: {}", makefile.display()));
        } else {
            // Check for compiled binary
            let binary = cpp_dir.join("ccu_cpp_utils");
            if !binary.exists() {
                mismatches.push(format!(
                    "C++ binary not found: {} — run `make` in tools/cpp_utils",
                    binary.display()
                ));
            } else {
                log_debug(&format!("C++ binary found: {}", binary.display()));
            }
        }

        // Check for CMake build directory
        let cmake_build = cpp_dir.join("build");
        if cmake_build.exists() {
            log_debug(&format!("CMake build dir found: {}", cmake_build.display()));
        }

        Ok(())
    }
}

fn log_debug(msg: &str) {
    eprintln!("[ccu-utils] {}", msg);
}

/// Errors that can occur during build verification.
#[derive(Debug, Error)]
pub enum BuildError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unexpected error: {0}")]
    Other(String),
}

/// Metadata about a component's build state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentMetadata {
    pub name: String,
    pub path: String,
    pub last_built: Option<String>,
    pub artifact_path: Option<String>,
    pub version: String,
}

impl ComponentMetadata {
    pub fn new(name: &str, path: &Path, version: &str) -> Self {
        Self {
            name: name.to_string(),
            path: path.display().to_string(),
            last_built: None,
            artifact_path: None,
            version: version.to_string(),
        }
    }

    pub fn with_artifact(mut self, artifact: &Path) -> Self {
        self.artifact_path = Some(artifact.display().to_string());
        self
    }

    pub fn with_timestamp(mut self, timestamp: &str) -> Self {
        self.last_built = Some(timestamp.to_string());
        self
    }
}

/// Collect metadata about all build components.
pub fn collect_metadata(project_root: &Path) -> Vec<ComponentMetadata> {
    let mut components = Vec::new();

    let rust_app = project_root.join("src-tauri");
    if rust_app.exists() {
        components.push(ComponentMetadata::new("ccu-backend", &rust_app, "0.1.0"));
    }

    let rust_utils = project_root.join("tools").join("rust_utils");
    if rust_utils.exists() {
        components.push(ComponentMetadata::new("ccu-utils", &rust_utils, "0.1.0"));
    }

    let cpp_utils = project_root.join("tools").join("cpp_utils");
    if cpp_utils.exists() {
        components.push(ComponentMetadata::new("ccu-cpp-utils", &cpp_utils, "1.0.0"));
    }

    let frontend = project_root;
    components.push(ComponentMetadata::new("ccu-frontend", frontend, "0.1.0"));

    components
}

/// Scan a directory for build artifacts and report sizes.
pub fn scan_artifacts(dir: &Path) -> HashMap<String, u64> {
    let mut artifacts = HashMap::new();

    if !dir.exists() {
        return artifacts;
    }

    for entry in WalkDir::new(dir)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Ok(metadata) = entry.metadata() {
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            artifacts.insert(name, metadata.len());
        }
    }

    artifacts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_config_defaults() {
        let config = BuildConfig {
            check_rust: true,
            check_frontend: false,
            check_cpp: false,
        };
        assert!(config.check_rust);
        assert!(!config.check_frontend);
        assert!(!config.check_cpp);
    }

    #[test]
    fn test_build_status_eq() {
        assert_eq!(BuildStatus::Ok, BuildStatus::Ok);
        assert_ne!(
            BuildStatus::Ok,
            BuildStatus::Mismatch {
                details: "test".to_string()
            }
        );
    }

    #[test]
    fn test_component_metadata() {
        let path = Path::new("/tmp/test");
        let meta = ComponentMetadata::new("test", path, "1.0.0");
        assert_eq!(meta.name, "test");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.last_built, None);

        let meta2 = meta.with_artifact(Path::new("/tmp/test-artifact"));
        assert_eq!(
            meta2.artifact_path,
            Some("/tmp/test-artifact".to_string())
        );

        let meta3 = meta2.with_timestamp("2024-01-01T00:00:00Z");
        assert_eq!(
            meta3.last_built,
            Some("2024-01-01T00:00:00Z".to_string())
        );
    }

    #[test]
    fn test_collect_metadata_empty() {
        let temp = tempfile::tempdir().unwrap();
        let result = collect_metadata(temp.path());
        // Always returns at least the frontend entry
        assert!(result.len() >= 1);
    }

    #[test]
    fn test_scan_artifacts_empty_dir() {
        let temp = tempfile::tempdir().unwrap();
        let result = scan_artifacts(temp.path());
        assert!(result.is_empty());
    }
}
