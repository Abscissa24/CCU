//! CCU development utilities library.
//!
//! Provides build verification, environment checking, and data validation
//! for the Chess Coach Utility project.
//!
//! # Modules
//!
//! - [`build_ops`]: Build configuration and verification
//! - [`data_check`]: Data file validation (PGN, CCUDB, JSON)
//! - [`env_check`]: Environment scanning and reporting

mod build_ops;
mod data_check;
mod env_check;

pub use build_ops::{
    BuildConfig, BuildError, BuildStatus, BuildVerifier, ComponentMetadata,
    collect_metadata, scan_artifacts,
};
pub use data_check::{
    CcudbError, CcudbValidator, DataError, DataFormat, DataValidator,
    GameRecord, JsonError, JsonValidator, PgnError, PgnValidator,
    ValidationResult,
};
pub use env_check::{EnvCheck, EnvReport};

/// Run configuration for utility operations.
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub verbose: bool,
    pub quiet: bool,
    pub timeout: std::time::Duration,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            quiet: false,
            timeout: std::time::Duration::from_secs(30),
        }
    }
}

impl RunConfig {
    pub fn is_verbose(&self) -> bool {
        self.verbose && !self.quiet
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet
    }

    pub fn with_verbose(mut self) -> Self {
        self.verbose = true;
        self
    }

    pub fn with_quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = std::time::Duration::from_secs(secs);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_config_defaults() {
        let config = RunConfig::default();
        assert!(!config.verbose);
        assert!(!config.quiet);
        assert_eq!(config.timeout, std::time::Duration::from_secs(30));
    }

    #[test]
    fn test_run_config_verbose() {
        let config = RunConfig::default().with_verbose();
        assert!(config.is_verbose());
        assert!(!config.is_quiet());
    }

    #[test]
    fn test_run_config_quiet() {
        let config = RunConfig::default().with_quiet();
        assert!(!config.is_verbose());
        assert!(config.is_quiet());
    }

    #[test]
    fn test_run_config_timeout() {
        let config = RunConfig::default().with_timeout(60);
        assert_eq!(config.timeout, std::time::Duration::from_secs(60));
    }

    #[test]
    fn test_run_config_is_verbose() {
        let config = RunConfig { verbose: true, quiet: false, timeout: std::time::Duration::from_secs(30) };
        assert!(config.is_verbose());
    }

    #[test]
    fn test_run_config_is_not_verbose_when_quiet() {
        let config = RunConfig { verbose: true, quiet: true, timeout: std::time::Duration::from_secs(30) };
        // quiet overrides verbose
        assert!(!config.is_verbose());
        assert!(config.is_quiet());
    }

    #[test]
    fn test_run_config_chaining() {
        let config = RunConfig::default()
            .with_verbose()
            .with_timeout(120);
        assert!(config.is_verbose());
        assert_eq!(config.timeout, std::time::Duration::from_secs(120));
    }
}

