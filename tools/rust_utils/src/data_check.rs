use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

/// Supported data file formats for validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataFormat {
    Pgn,
    Ccudb,
    Json,
    Unknown,
}

impl std::fmt::Display for DataFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataFormat::Pgn => write!(f, "PGN"),
            DataFormat::Ccudb => write!(f, "CCUDB"),
            DataFormat::Json => write!(f, "JSON"),
            DataFormat::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Result of validating a single file.
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Valid { path: PathBuf, format: DataFormat, size: u64 },
    Invalid { path: PathBuf, format: DataFormat, error: String },
    Skipped { path: PathBuf, reason: String },
}

/// Validates data files in the CCU project.
pub struct DataValidator {
    pgn_validator: PgnValidator,
    json_validator: JsonValidator,
    ccudb_validator: CcudbValidator,
}

impl DataValidator {
    pub fn new() -> Self {
        Self {
            pgn_validator: PgnValidator::new(),
            json_validator: JsonValidator::new(),
            ccudb_validator: CcudbValidator::new(),
        }
    }

    /// Validate a path — can be a file or directory.
    pub fn validate_path(&self, path: &Path) -> Result<Vec<ValidationResult>, DataError> {
        let mut results = Vec::new();

        if path.is_dir() {
            for entry in WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path().to_path_buf();
                if let Some(result) = self.validate_file(&file_path) {
                    results.push(result);
                }
            }
        } else if path.is_file() {
            if let Some(result) = self.validate_file(path) {
                results.push(result);
            }
        } else {
            results.push(ValidationResult::Skipped {
                path: path.to_path_buf(),
                reason: "Path does not exist".to_string(),
            });
        }

        Ok(results)
    }

    /// Validate a single file based on its extension.
    fn validate_file(&self, path: &Path) -> Option<ValidationResult> {
        let extension = path.extension()?.to_string_lossy().to_lowercase();
        let size = std::fs::metadata(path).ok()?.len();

        match extension.as_ref() {
            "pgn" => self.validate_pgn(path, size),
            "json" | "jsonc" => self.validate_json(path, size),
            "ccudb" => self.validate_ccudb(path, size),
            _ => {
                // Check magic bytes
                if let Ok(contents) = std::fs::read(path) {
                    if contents.len() >= 4 && &contents[..4] == CcudbValidator::MAGIC {
                        match self.ccudb_validator.validate(path, &contents) {
                            Ok(()) => Some(ValidationResult::Valid {
                                path: path.to_path_buf(),
                                format: DataFormat::Ccudb,
                                size,
                            }),
                            Err(e) => Some(ValidationResult::Invalid {
                                path: path.to_path_buf(),
                                format: DataFormat::Ccudb,
                                error: e.to_string(),
                            }),
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    }

    fn validate_pgn(&self, path: &Path, size: u64) -> Option<ValidationResult> {
        let contents = std::fs::read_to_string(path).ok()?;
        match self.pgn_validator.validate(&contents) {
            Ok(()) => Some(ValidationResult::Valid {
                path: path.to_path_buf(),
                format: DataFormat::Pgn,
                size,
            }),
            Err(e) => Some(ValidationResult::Invalid {
                path: path.to_path_buf(),
                format: DataFormat::Pgn,
                error: e.to_string(),
            }),
        }
    }

    fn validate_json(&self, path: &Path, size: u64) -> Option<ValidationResult> {
        let contents = std::fs::read_to_string(path).ok()?;
        match self.json_validator.validate(&contents) {
            Ok(()) => Some(ValidationResult::Valid {
                path: path.to_path_buf(),
                format: DataFormat::Json,
                size,
            }),
            Err(e) => Some(ValidationResult::Invalid {
                path: path.to_path_buf(),
                format: DataFormat::Json,
                error: e.to_string(),
            }),
        }
    }

    fn validate_ccudb(&self, path: &Path, size: u64) -> Option<ValidationResult> {
        let contents = std::fs::read(path).ok()?;
        match self.ccudb_validator.validate(path, &contents) {
            Ok(()) => Some(ValidationResult::Valid {
                path: path.to_path_buf(),
                format: DataFormat::Ccudb,
                size,
            }),
            Err(e) => Some(ValidationResult::Invalid {
                path: path.to_path_buf(),
                format: DataFormat::Ccudb,
                error: e.to_string(),
            }),
        }
    }
}

/// Validates PGN (Portable Game Notation) files.
pub struct PgnValidator;

impl PgnValidator {
    pub fn new() -> Self {
        Self
    }

    /// Validate PGN content structure.
    pub fn validate(&self, content: &str) -> Result<(), PgnError> {
        if content.is_empty() {
            return Err(PgnError::EmptyFile);
        }

        let mut has_event = false;
        let mut has_site_tag = false;
        let mut has_white = false;
        let mut has_black = false;
        let mut has_result = false;
        let mut has_moves = false;
        let line_count = content.lines().count();

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                has_site_tag = true;
                if trimmed.starts_with("[Event ") || trimmed.starts_with("[EVENT ") {
                    has_event = true;
                }
                if trimmed.starts_with("[Site ") || trimmed.starts_with("[SITE ") {
                    // Site tag
                }
                if trimmed.starts_with("[Date ") || trimmed.starts_with("[DATE ") {
                    // Date tag
                }
                if trimmed.starts_with("[Round ") || trimmed.starts_with("[ROUND ") {
                    // Round tag
                }
                if trimmed.starts_with("[White ") || trimmed.starts_with("[WHITE ") {
                    has_white = true;
                }
                if trimmed.starts_with("[Black ") || trimmed.starts_with("[BLACK ") {
                    has_black = true;
                }
                if trimmed.starts_with("[Result ") || trimmed.starts_with("[RESULT ") {
                    has_result = true;
                }
            }

            // Check for move content (basic heuristic)
            if !trimmed.is_empty() && !trimmed.starts_with('[') && !trimmed.starts_with('{') {
                if trimmed.chars().any(|c| c.is_ascii_digit()) {
                    has_moves = true;
                }
            }
        }

        let required_fields = has_event || has_white || has_black || has_result;
        if has_site_tag && !required_fields {
            return Err(PgnError::MissingRequiredFields);
        }

        if !has_moves && has_site_tag {
            return Err(PgnError::MissingMoves);
        }

        if line_count > 100_000 {
            return Err(PgnError::FileTooLarge);
        }

        Ok(())
    }
}

/// PGN validation errors.
#[derive(Debug, Error)]
pub enum PgnError {
    #[error("File is empty")]
    EmptyFile,
    #[error("Missing required PGN tags")]
    MissingRequiredFields,
    #[error("Missing move data")]
    MissingMoves,
    #[error("File exceeds maximum size (100,000 lines)")]
    FileTooLarge,
    #[error("Invalid PGN syntax: {0}")]
    Syntax(String),
}

/// Validates JSON files.
pub struct JsonValidator;

impl JsonValidator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, content: &str) -> Result<(), JsonError> {
        let trimmed = content.trim();

        if trimmed.is_empty() {
            return Err(JsonError::EmptyFile);
        }

        // Strip JSONC comments (JSON with comments)
        let clean = if trimmed.contains("//") || trimmed.contains("/*") {
            strip_json_comments(trimmed)
        } else {
            trimmed.to_string()
        };

        if clean.trim().is_empty() {
            return Err(JsonError::EmptyFile);
        }

        match serde_json::from_str::<serde_json::Value>(&clean) {
            Ok(_) => Ok(()),
            Err(e) => Err(JsonError::ParseError(e.to_string())),
        }
    }

    /// Validate JSON with a provided schema (best-effort).
    pub fn validate_with_schema(
        &self,
        content: &str,
        _schema: &serde_json::Value,
    ) -> Result<(), JsonError> {
        // Basic validation first
        self.validate(content)?;

        // Schema validation would go here with a proper schema library
        // For now, we accept any valid JSON
        Ok(())
    }
}

/// Strip JavaScript-style comments from JSON (JSONC).
fn strip_json_comments(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(&c) = chars.peek() {
        if escape {
            result.push(c);
            escape = false;
            chars.next();
            continue;
        }

        if c == '\\' && in_string {
            result.push(c);
            escape = true;
            chars.next();
            continue;
        }

        if c == '"' {
            in_string = !in_string;
            result.push(c);
            chars.next();
            continue;
        }

        if !in_string {
            // Check for line comment
            if c == '/' {
                chars.next();
                if let Some(&'/') = chars.peek() {
                    chars.next();
                    // Skip until end of line
                    while let Some(&next) = chars.peek() {
                        if next == '\n' {
                            result.push('\n');
                            chars.next();
                            break;
                        }
                        chars.next();
                    }
                    continue;
                } else if let Some(&'*') = chars.peek() {
                    chars.next();
                    // Skip until */
                    while let Some(c1) = chars.next() {
                        if c1 == '*' {
                            if let Some(&'/') = chars.peek() {
                                chars.next();
                                break;
                            }
                        }
                    }
                    continue;
                }
                // Just a single slash
                result.push('/');
                continue;
            }
        }

        result.push(c);
        chars.next();
    }

    result
}

/// JSON validation errors.
#[derive(Debug, Error)]
pub enum JsonError {
    #[error("File is empty")]
    EmptyFile,
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Schema validation error: {0}")]
    SchemaError(String),
}

/// Validates CCUDB binary database files.
pub struct CcudbValidator;

impl CcudbValidator {
    pub fn new() -> Self {
        Self
    }

    /// Magic header for CCUDB files.
    pub const MAGIC: &[u8] = b"CCUD";

    /// Validate a CCUDB file.
    pub fn validate(&self, _path: &Path, data: &[u8]) -> Result<(), CcudbError> {
        if data.is_empty() {
            return Err(CcudbError::EmptyFile);
        }

        if data.len() < 8 {
            return Err(CcudbError::TruncatedHeader);
        }

        // Check magic bytes
        if &data[..4] != Self::MAGIC {
            return Err(CcudbError::InvalidMagic);
        }

        // Read version (bytes 4-5, little endian u16)
        let version = u16::from_le_bytes([data[4], data[5]]);

        // Read header size (bytes 6-7, little endian u16)
        let header_size = u16::from_le_bytes([data[6], data[7]]) as usize;

        if header_size < 8 {
            return Err(CcudbError::InvalidHeaderSize);
        }

        // Check supported version range
        if version > 10 {
            return Err(CcudbError::UnsupportedVersion(version));
        }

        // Check that file is long enough for the declared header size
        if data.len() < header_size {
            return Err(CcudbError::TruncatedFile {
                expected: header_size,
                actual: data.len(),
            });
        }

        // Validate record section follows header
        if data.len() > header_size {
            // Check magic bytes of first record
            let record_magic = &data[header_size..header_size + 4];
            if record_magic == Self::MAGIC {
                // Valid continuation
            } else {
                // Might still be valid if header includes padding
            }
        }

        Ok(())
    }

    /// Create a new CCUDB file from game data.
    pub fn create(&self, version: u16, games: &[GameRecord]) -> Vec<u8> {
        let mut data = Vec::new();

        // Magic header
        data.extend_from_slice(Self::MAGIC);

        // Version (u16 LE)
        let version_bytes = version.to_le_bytes();
        data.extend_from_slice(&version_bytes);

        // Calculate header size and reserve space
        let header_size = 8 + games.len() * 8; // magic(4) + version(2) + size(2) + game_entries(8 each)
        let header_size_bytes = (header_size as u16).to_le_bytes();
        data.extend_from_slice(&header_size_bytes);

        // Write game record headers
        let mut offset = header_size;
        for game in games {
            // Each game entry: offset (4 bytes) + length (4 bytes)
            data.extend_from_slice(&(offset as u32).to_le_bytes());
            let encoded = serde_json::to_vec(game).unwrap_or_default();
            data.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
            offset += encoded.len();
        }

        // Write game data (JSON encoded)
        for game in games {
            if let Ok(encoded) = serde_json::to_vec(game) {
                data.extend_from_slice(&encoded);
            }
        }

        data
    }

    /// Parse game records from CCUDB data.
    pub fn parse_games(&self, data: &[u8]) -> Result<Vec<GameRecord>, CcudbError> {
        if data.len() < 8 {
            return Err(CcudbError::TruncatedHeader);
        }

        if &data[..4] != Self::MAGIC {
            return Err(CcudbError::InvalidMagic);
        }

        let version = u16::from_le_bytes([data[4], data[5]]);
        let header_size = u16::from_le_bytes([data[6], data[7]]) as usize;

        if version > 10 {
            return Err(CcudbError::UnsupportedVersion(version));
        }

        let mut games = Vec::new();
        let num_entries = (header_size - 8) / 8;
        let mut offset = 8;

        for _ in 0..num_entries {
            if offset + 8 > data.len() {
                break;
            }

            let record_offset = u32::from_le_bytes([
                data[offset], data[offset + 1], data[offset + 2], data[offset + 3]
            ]) as usize;
            let record_len = u32::from_le_bytes([
                data[offset + 4], data[offset + 5], data[offset + 6], data[offset + 7]
            ]) as usize;

            offset += 8;

            if record_offset + record_len > data.len() {
                break;
            }

            let record_data = &data[record_offset..record_offset + record_len];
            if let Ok(game) = serde_json::from_slice::<GameRecord>(record_data) {
                games.push(game);
            }
        }

        Ok(games)
    }
}

/// A single game record in CCUDB format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRecord {
    pub id: String,
    pub white_player: String,
    pub black_player: String,
    pub result: String,
    pub event: Option<String>,
    pub date: Option<String>,
    pub moves: Vec<String>,
    pub annotations: Vec<String>,
}

impl GameRecord {
    pub fn new(
        white: &str,
        black: &str,
        result: &str,
    ) -> Self {
        Self {
            id: generate_id(white, black, result),
            white_player: white.to_string(),
            black_player: black.to_string(),
            result: result.to_string(),
            event: None,
            date: None,
            moves: Vec::new(),
            annotations: Vec::new(),
        }
    }

    pub fn with_event(mut self, event: &str) -> Self {
        self.event = Some(event.to_string());
        self
    }

    pub fn with_date(mut self, date: &str) -> Self {
        self.date = Some(date.to_string());
        self
    }

    pub fn add_move(&mut self, movetext: &str, san: &str) -> &mut Self {
        self.moves.push(format!("{} {}", movetext, san));
        self
    }

    pub fn add_annotation(&mut self, text: &str) -> &mut Self {
        self.annotations.push(text.to_string());
        self
    }

    pub fn move_count(&self) -> usize {
        self.moves.len()
    }

    pub fn has_annotations(&self) -> bool {
        !self.annotations.is_empty()
    }

    pub fn display_summary(&self) -> String {
        let event_str = self.event.as_deref().unwrap_or("Unknown Event");
        let date_str = self.date.as_deref().unwrap_or("????.??.??");
        format!(
            "{}: {} vs {} ({}) [{}] — {} moves",
            event_str,
            self.white_player,
            self.black_player,
            date_str,
            self.result,
            self.move_count()
        )
    }
}

/// Generate a unique game ID from player names and result.
fn generate_id(white: &str, black: &str, result: &str) -> String {
    use sha2::{Digest, Sha256};

    let input = format!("{}|{}|{}", white, black, result);
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();

    // Take first 8 bytes and hex-encode
    hex::encode(&hash[..8])
}

/// CCUDB validation errors.
#[derive(Debug, Error)]
pub enum CcudbError {
    #[error("File is empty")]
    EmptyFile,
    #[error("File is truncated — header too small")]
    TruncatedHeader,
    #[error("Invalid magic bytes — not a CCUDB file")]
    InvalidMagic,
    #[error("Invalid header size")]
    InvalidHeaderSize,
    #[error("Unsupported CCUDB version: {0}")]
    UnsupportedVersion(u16),
    #[error("File truncated: expected {expected} bytes, got {actual}")]
    TruncatedFile { expected: usize, actual: usize },
    #[error("Read error: {0}")]
    ReadError(#[from] std::io::Error),
}

/// Errors that can occur during data validation.
#[derive(Debug, Error)]
pub enum DataError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_format_display() {
        assert_eq!(DataFormat::Pgn.to_string(), "PGN");
        assert_eq!(DataFormat::Ccudb.to_string(), "CCUDB");
        assert_eq!(DataFormat::Json.to_string(), "JSON");
        assert_eq!(DataFormat::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn test_json_validator_valid() {
        let validator = JsonValidator::new();
        let valid = r#"{"name":"test","value":42}"#;
        assert!(validator.validate(valid).is_ok());
    }

    #[test]
    fn test_json_validator_empty() {
        let validator = JsonValidator::new();
        let result = validator.validate("");
        assert!(matches!(result, Err(JsonError::EmptyFile)));
    }

    #[test]
    fn test_json_validator_invalid() {
        let validator = JsonValidator::new();
        let invalid = r#"{"name":"test""#;
        assert!(validator.validate(invalid).is_err());
    }

    #[test]
    fn test_json_validator_with_comments() {
        let validator = JsonValidator::new();
        let jsonc = r#"{
            // This is a comment
            "name": "test" /* inline comment */,
            "value": 42
        }"#;
        assert!(validator.validate(jsonc).is_ok());
    }

    #[test]
    fn test_strip_json_comments() {
        let input = r#"{"a": 1 // comment
        , "b": 2}"#;
        let result = strip_json_comments(input);
        assert!(!result.contains("comment"));
    }

    #[test]
    fn test_pgn_validator_valid() {
        let validator = PgnValidator::new();
        let pgn = r#"[Event "Test Game"]
[Site "TestSite"]
[Date "2024.01.01"]
[Round "?"]
[White "Player1"]
[Black "Player2"]
[Result "1-0"]

1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0
"#;
        assert!(validator.validate(pgn).is_ok());
    }

    #[test]
    fn test_pgn_validator_empty() {
        let validator = PgnValidator::new();
        let result = validator.validate("");
        assert!(matches!(result, Err(PgnError::EmptyFile)));
    }

    #[test]
    fn test_ccudb_validator_magic() {
        assert_eq!(CcudbValidator::MAGIC, b"CCUD");
    }

    #[test]
    fn test_ccudb_validator_invalid_magic() {
        let validator = CcudbValidator::new();
        let bad_data = b"BADX\x00\x01\x00\x08";
        let result = validator.validate(Path::new("test.ccudb"), bad_data);
        assert!(matches!(result, Err(CcudbError::InvalidMagic)));
    }

    #[test]
    fn test_ccudb_validator_truncated() {
        let validator = CcudbValidator::new();
        let data = b"CCUD";
        let result = validator.validate(Path::new("test.ccudb"), data);
        assert!(matches!(result, Err(CcudbError::TruncatedHeader)));
    }

    #[test]
    fn test_game_record() {
        let mut record = GameRecord::new("Alice", "Bob", "1-0")
            .with_event("Test Tournament")
            .with_date("2024.01.15");
        record.add_move("1", "e4");
        record.add_move("1", "...e5");
        record.add_annotation("Good opening");

        assert_eq!(record.white_player, "Alice");
        assert_eq!(record.black_player, "Bob");
        assert_eq!(record.result, "1-0");
        assert_eq!(record.move_count(), 2);
        assert!(record.has_annotations());
        assert!(record.display_summary().contains("Alice vs Bob"));
    }

    #[test]
    fn test_generate_id_is_deterministic() {
        let id1 = generate_id("Alice", "Bob", "1-0");
        let id2 = generate_id("Alice", "Bob", "1-0");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_generate_id_differs() {
        let id1 = generate_id("Alice", "Bob", "1-0");
        let id2 = generate_id("Alice", "Charlie", "0-1");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_ccudb_create_and_parse() {
        let validator = CcudbValidator::new();
        let games = vec![
            GameRecord::new("Alice", "Bob", "1-0"),
            GameRecord::new("Carol", "Dave", "*"),
        ];

        let data = validator.create(1, &games);
        let parsed = validator.parse_games(&data).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].white_player, "Alice");
        assert_eq!(parsed[1].white_player, "Carol");
    }

    #[test]
    fn test_validation_result_variants() {
        let path = PathBuf::from("/tmp/test.pgn");
        let valid = ValidationResult::Valid {
            path: path.clone(),
            format: DataFormat::Pgn,
            size: 1024,
        };
        let invalid = ValidationResult::Invalid {
            path: path.clone(),
            format: DataFormat::Pgn,
            error: "syntax error".to_string(),
        };
        let skipped = ValidationResult::Skipped {
            path: path.clone(),
            reason: "unknown format".to_string(),
        };

        // Just ensure they compile and are comparable
        let _ = format!("{:?}", valid);
        let _ = format!("{:?}", invalid);
        let _ = format!("{:?}", skipped);
    }
}
