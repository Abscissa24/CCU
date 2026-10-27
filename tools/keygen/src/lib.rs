// ── ccu-keygen core library (Linux-only activation key generator) ─────
//
// The product is one 20-character activation code per device:
//
//   CCU5 + 16 random base36 characters   (grouped 5-5-5-5 for typing)
//
// A code is a bearer token, not a signature: it contains no secret and no
// expiry. It becomes useful only after the owner registers it with the
// activation service (POST /admin/import). When a user enters the code, the
// app sends it once to that service together with the device id and the
// workspace name; the service marks the seat used and replies with a signed,
// device-bound licence. From then on the app verifies that licence offline
// against the public key compiled into it (core/licenseConfig.js).
//
// Consequences (all deliberate):
//   • losing/leaking a code only affects that one code — the service stores
//     only SHA-256 hashes, so the plaintext batch file can be discarded;
//   • a code can be revoked server-side before first use, and its device can
//     be released if hardware is replaced;
//   • nobody can mint a licence from the app: only the activation private key
//     (kept in ~/.config/ccu-keygen/keys.json and on the server) can sign.
//
// The retired schemes (symmetric 20-char product keys and Ed25519-signed
// .ccukey files) are intentionally absent.

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const ACTIVATION_PREFIX: &str = "CCU5";
pub const ACTIVATION_LEN: usize = 20;
pub const ACTIVATION_GROUP: usize = 5;
pub const ACTIVATION_KID: &str = "activation-2026";
pub const BATCH_VERSION: u32 = 2;
pub const KEYRING_VERSION: u32 = 2;

/// Default public activation endpoint. The server URL can be overridden with
/// `--server`, `CCU_ACTIVATION_URL`, or the GUI's service settings.
pub const DEFAULT_ACTIVATION_URL: &str = "https://ccu-activation.ccu.deno.net";

const B36: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

fn fill_random(buf: &mut [u8]) -> Result<()> {
    getrandom::getrandom(buf)
        .map_err(|e| anyhow::anyhow!("the OS random source failed: {e}"))
}

// ── Encoding helpers ──────────────────────────────────────────────────
pub fn b36_encode(mut v: u128, len: usize) -> String {
    let mut s = vec![b'0'; len];
    let mut i = len;
    while v > 0 && i > 0 {
        i -= 1;
        s[i] = B36[(v % 36) as usize];
        v /= 36;
    }
    String::from_utf8(s).expect("base36 is always ASCII")
}

pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub fn hex_decode(value: &str) -> Result<Vec<u8>> {
    let clean = value.trim();
    if clean.len() % 2 != 0 {
        bail!("hex value has an odd length");
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    for i in (0..clean.len()).step_by(2) {
        out.push(
            u8::from_str_radix(&clean[i..i + 2], 16)
                .with_context(|| format!("invalid hex byte in {clean}"))?,
        );
    }
    Ok(out)
}

pub fn sha256_hex(value: &str) -> String {
    let mut h = Sha256::new();
    h.update(value.as_bytes());
    hex_encode(&h.finalize())
}

pub fn canonical_name(name: &str) -> String {
    name.trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// 6-char school code: (first 48 bits of SHA-256(canonical name)) mod 36^6.
/// Name-derived, so it is stable as schools are added to or removed from the
/// registry. The activation service and the app both derive the same value
/// and the server stores it as `orgId` in the signed licence payload.
pub fn school_code(name: &str) -> String {
    let digest = Sha256::digest(canonical_name(name).as_bytes());
    let mut v = 0u128;
    for b in &digest[..6] {
        v = (v << 8) | (*b as u128);
    }
    b36_encode(v % 36u128.pow(6), 6)
}

// ── Activation codes ──────────────────────────────────────────────────
pub fn normalize_activation_code(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

pub fn is_activation_code(value: &str) -> bool {
    let clean = normalize_activation_code(value);
    clean.len() == ACTIVATION_LEN && clean.starts_with(ACTIVATION_PREFIX)
}

/// 20 raw chars → 20 chars grouped 5-5-5-5 (the format users type).
pub fn format_activation_code(value: &str) -> String {
    let clean = normalize_activation_code(value);
    clean
        .chars()
        .collect::<Vec<_>>()
        .chunks(ACTIVATION_GROUP)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// 16 random base36 chars (≈80 bits) — unique per device/seat.
pub fn new_activation_code() -> Result<String> {
    let mut buf = [0u8; 16];
    fill_random(&mut buf)?;
    let v = u128::from_be_bytes(buf);
    let modulus = 36u128.pow((ACTIVATION_LEN - ACTIVATION_PREFIX.len()) as u32);
    Ok(format!(
        "{}{}",
        ACTIVATION_PREFIX,
        b36_encode(v % modulus, ACTIVATION_LEN - ACTIVATION_PREFIX.len())
    ))
}

pub fn new_license_id() -> Result<String> {
    let mut buf = [0u8; 10];
    fill_random(&mut buf)?;
    Ok(format!("lic_{}", hex_encode(&buf)))
}

pub fn new_order_id() -> Result<String> {
    let mut buf = [0u8; 6];
    fill_random(&mut buf)?;
    Ok(format!("ord_{}", hex_encode(&buf)))
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── Activation batch (wire-compatible with activation/server.mjs) ─────
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationCodeEntry {
    /// Formatted (CCU5X-XXXXX-XXXXX-XXXXX) for handing to a person.
    pub code: String,
    /// SHA-256 of the normalised code — the only form the server stores.
    pub code_hash: String,
    pub org: String,
    pub org_id: String,
    pub kind: String,
    pub license_id: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub seats: u32,
    pub order_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationBatch {
    pub version: u32,
    pub activation_kid: String,
    pub generated_at: u64,
    pub codes: Vec<ActivationCodeEntry>,
}

impl ActivationBatch {
    pub fn plain_codes(&self) -> Vec<String> {
        self.codes.iter().map(|c| c.code.clone()).collect()
    }
}

/// Builds `count` single-use codes for an org. `duration_minutes` is measured
/// from now and becomes the licence expiry when a code is first activated.
pub fn issue_activation_codes(
    name: &str,
    kind: &str,
    duration_minutes: u64,
    count: usize,
    seats: u32,
    order_id: Option<String>,
) -> Result<ActivationBatch> {
    let name = name.trim();
    if name.is_empty() {
        bail!("a school or coaching business name is required");
    }
    let kind = if kind == "coach" { "coach" } else { "school" };
    if count == 0 {
        bail!("count must be at least 1");
    }
    if count > 10_000 {
        bail!("refusing to generate more than 10,000 codes in one batch");
    }
    if seats == 0 {
        bail!("seats must be at least 1");
    }
    if duration_minutes == 0 {
        bail!("the licence duration must be positive");
    }
    let Some(duration_ms) = duration_minutes.checked_mul(60_000) else {
        bail!("the licence duration is too long");
    };
    let now = now_ms();
    let expires_at = now + duration_ms;
    let order_id = order_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(new_order_id()?);
    let org_id = school_code(name);
    // Informational only: the service stamps its own kid into each licence.
    let kid = ACTIVATION_KID.to_string();

    let mut codes = Vec::with_capacity(count);
    for _ in 0..count {
        let raw = new_activation_code()?;
        codes.push(ActivationCodeEntry {
            code: format_activation_code(&raw),
            code_hash: sha256_hex(&raw),
            org: name.to_string(),
            org_id: org_id.clone(),
            kind: kind.to_string(),
            license_id: new_license_id()?,
            issued_at: now,
            expires_at,
            seats,
            order_id: order_id.clone(),
        });
    }
    Ok(ActivationBatch {
        version: BATCH_VERSION,
        activation_kid: kid,
        generated_at: now,
        codes,
    })
}

// ── Signing keyring (activation only) ─────────────────────────────────
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyEntry {
    pub kid: String,
    pub public_key: String,
    pub private_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Keyring {
    pub version: u32,
    pub activation: KeyEntry,
}

/// v1 keyrings (file + activation) are still readable so an existing
/// installation keeps its production key; only the activation entry is used.
/// Unknown fields (such as the retired `file` entry) are ignored.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredKeyring {
    version: u32,
    #[serde(default)]
    activation: Option<KeyEntry>,
}

pub fn keygen_dir() -> PathBuf {
    if let Ok(home) = std::env::var("CCU_KEYGEN_HOME") {
        return PathBuf::from(home);
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("ccu-keygen")
}

pub fn keyring_path() -> PathBuf {
    keygen_dir().join("keys.json")
}

fn make_key(kid: &str) -> Result<KeyEntry> {
    let mut bytes = [0u8; 32];
    fill_random(&mut bytes)?;
    let signing = SigningKey::from_bytes(&bytes);
    Ok(KeyEntry {
        kid: kid.to_string(),
        public_key: hex_encode(signing.verifying_key().as_bytes()),
        private_key: hex_encode(&bytes),
    })
}

pub fn generate_keyring() -> Result<Keyring> {
    Ok(Keyring {
        version: KEYRING_VERSION,
        activation: make_key("activation-2026")?,
    })
}

fn check_entry(entry: &KeyEntry) -> Result<()> {
    let private = hex_decode(&entry.private_key)?;
    if private.len() != 32 {
        bail!("{} private key must be 32 bytes", entry.kid);
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&private);
    let derived = SigningKey::from_bytes(&seed).verifying_key();
    if hex_encode(derived.as_bytes()) != entry.public_key.to_lowercase() {
        bail!(
            "{} public key does not match its private key — the keyring is corrupt",
            entry.kid
        );
    }
    Ok(())
}

pub fn load_keyring() -> Result<Keyring> {
    load_keyring_from(&keyring_path())
}

pub fn load_keyring_from(path: &Path) -> Result<Keyring> {
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "no signing key at {}. Run `ccu-keygen init` once, then keep that file backed up.",
            path.display()
        )
    })?;
    let stored: StoredKeyring =
        serde_json::from_str(&text).context("signing key file is not valid JSON")?;
    if stored.version != 1 && stored.version != KEYRING_VERSION {
        bail!("unsupported signing key version {}", stored.version);
    }
    let activation = stored.activation.ok_or_else(|| {
        anyhow::anyhow!(
            "{} has no activation key — run `ccu-keygen init` to create one",
            path.display()
        )
    })?;
    check_entry(&activation)?;
    Ok(Keyring {
        version: KEYRING_VERSION,
        activation,
    })
}

pub fn write_keyring(ring: &Keyring) -> Result<PathBuf> {
    let path = keyring_path();
    write_keyring_to(&path, ring)?;
    Ok(path)
}

pub fn write_keyring_to(path: &Path, ring: &Keyring) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(ring)?;
    std::fs::write(path, format!("{text}\n"))
        .with_context(|| format!("cannot write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn init_keyring(force: bool) -> Result<(Keyring, PathBuf, bool)> {
    let path = keyring_path();
    if path.exists() && !force {
        let ring = load_keyring_from(&path)?;
        return Ok((ring, path, false));
    }
    let ring = generate_keyring()?;
    write_keyring_to(&path, &ring)?;
    Ok((ring, path, true))
}

pub fn public_keys_json(ring: &Keyring) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        ring.activation.kid.clone(),
        serde_json::Value::String(ring.activation.public_key.clone()),
    );
    serde_json::Value::Object(map)
}

// ── Persisted settings (server URL / admin token) ─────────────────────
pub fn read_setting(name: &str) -> Option<String> {
    let text = std::fs::read_to_string(keygen_dir().join(name)).ok()?;
    let value = text.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub fn write_setting(name: &str, value: &str) -> Result<()> {
    let dir = keygen_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join(name);
    let value = value.trim();
    if value.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    std::fs::write(&path, format!("{value}\n"))
        .with_context(|| format!("cannot write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if name == "admin-token" { 0o600 } else { 0o644 };
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode));
    }
    Ok(())
}

/// Resolution order: explicit flag → environment → saved GUI setting →
/// production endpoint.
pub fn resolve_server(explicit: Option<&str>) -> String {
    explicit
        .map(str::to_string)
        .or_else(|| std::env::var("CCU_ACTIVATION_URL").ok())
        .or_else(|| read_setting("server"))
        .unwrap_or_else(|| DEFAULT_ACTIVATION_URL.to_string())
        .trim()
        .trim_end_matches('/')
        .to_string()
}

pub fn resolve_admin_token(explicit: Option<&str>) -> Option<String> {
    explicit
        .map(str::to_string)
        .or_else(|| std::env::var("CCU_ADMIN_TOKEN").ok())
        .or_else(|| read_setting("admin-token"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ── Platform gate: Linux only ─────────────────────────────────────────
// The binary is built for Linux and refuses to run anywhere else, so the
// key generator cannot be operated from a foreign OS by accident.
pub fn platform_allowed(os: &str) -> bool {
    os == "linux"
}

// ── Schools file ──────────────────────────────────────────────────────
#[derive(Clone, Debug)]
pub struct SchoolEntry {
    pub name: String,
    pub district: String,
    /// _config palette color (hex, e.g. "#048037"), if declared.
    pub palette: Option<String>,
}

/// Parses index.json into (name, district, palette) entries — mirrors the
/// app's welcome screen, which groups schools by district under `_config`.
pub fn school_entries(path: &Path) -> Result<Vec<SchoolEntry>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read schools file {}", path.display()))?;
    let data: serde_json::Value =
        serde_json::from_str(&text).context("schools file is not valid JSON")?;
    let Some(obj) = data.as_object() else {
        bail!("schools file must be a JSON object of district → school arrays");
    };
    let config = obj.get("_config").and_then(|c| c.as_object());
    let mut out = Vec::new();
    for (district, schools) in obj {
        if district == "_config" {
            continue;
        }
        let Some(arr) = schools.as_array() else {
            bail!("district {district} is not an array");
        };
        for s in arr {
            let Some(name) = s.as_str() else {
                bail!("school entry in {district} is not a string");
            };
            let palette = config
                .and_then(|c| c.get(name))
                .and_then(|v| v.get("paletteColor"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if !out.iter().any(|e: &SchoolEntry| e.name == name) {
                out.push(SchoolEntry {
                    name: name.to_string(),
                    district: district.to_string(),
                    palette,
                });
            }
        }
    }
    Ok(out)
}

pub fn school_names(path: &Path) -> Result<Vec<String>> {
    Ok(school_entries(path)?.into_iter().map(|e| e.name).collect())
}

pub fn require_known_school(name: &str, schools: &[String]) -> Result<()> {
    if schools.iter().any(|s| s == name) {
        Ok(())
    } else {
        bail!(
            "school \"{name}\" is not in the schools file — run `ccu-keygen list` to see valid names"
        )
    }
}

/// Logo file for a school: <index.json dir>/<district>/<name>/logo.png
/// (mirrors getSchoolLogo in the app).
pub fn school_logo_path(index_json: &Path, entry: &SchoolEntry) -> PathBuf {
    index_json
        .parent()
        .unwrap_or(Path::new("."))
        .join(&entry.district)
        .join(&entry.name)
        .join("logo.png")
}

// ── Presentation helpers ───────────────────────────────────────────────
pub fn iso8601(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, minute) = (rem / 3600, (rem % 3600) / 60);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{minute:02} UTC")
}

// Howard Hinnant's civil_from_days (public domain)
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn format_remaining(expiry_ms: u64) -> String {
    let now = now_ms();
    if expiry_ms <= now {
        return "expired".to_string();
    }
    let mins = (expiry_ms - now) / 60_000;
    let fmt = |n: u64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    if mins >= 43_200 {
        fmt(mins / 43_200, "month")
    } else if mins >= 1440 {
        fmt(mins / 1440, "day")
    } else if mins >= 60 {
        fmt(mins / 60, "hour")
    } else {
        fmt(mins.max(1), "minute")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_codes_have_the_documented_shape() {
        for _ in 0..50 {
            let raw = new_activation_code().unwrap();
            assert_eq!(raw.len(), ACTIVATION_LEN);
            assert!(raw.starts_with(ACTIVATION_PREFIX));
            assert!(raw.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
            assert!(is_activation_code(&raw));
            assert!(is_activation_code(&format_activation_code(&raw)));
            let formatted = format_activation_code(&raw);
            assert_eq!(formatted.len(), 23); // 20 chars + 3 dashes
            assert_eq!(normalize_activation_code(&formatted), raw);
        }
    }

    #[test]
    fn codes_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(new_activation_code().unwrap()));
        }
    }

    #[test]
    fn rejects_malformed_codes() {
        assert!(!is_activation_code(""));
        assert!(!is_activation_code("CCU5"));
        assert!(!is_activation_code("XXXX5-ABCDE-FGHIJ-KLMNO"));
        assert!(!is_activation_code("CCU5A-BCDEF"));
    }

    #[test]
    fn batch_matches_the_activation_service_wire_format() {
        let batch = issue_activation_codes(
            "Port Shepstone High School",
            "school",
            60,
            3,
            2,
            Some("INV-1".into()),
        )
        .unwrap();
        assert_eq!(batch.version, BATCH_VERSION);
        assert_eq!(batch.codes.len(), 3);
        for entry in &batch.codes {
            let clean = normalize_activation_code(&entry.code);
            assert_eq!(entry.code_hash, sha256_hex(&clean));
            assert_eq!(entry.org_id, school_code(&entry.org));
            assert_eq!(entry.org_id.len(), 6);
            assert_eq!(entry.kind, "school");
            assert_eq!(entry.seats, 2);
            assert_eq!(entry.order_id, "INV-1");
            assert!(entry.expires_at > entry.issued_at);
            assert!(entry.license_id.starts_with("lic_"));
        }
        // The server accepts either the batch object or its codes array.
        let value = serde_json::to_value(&batch).unwrap();
        assert_eq!(value["codes"].as_array().unwrap().len(), 3);
        assert!(value["codes"][0]["codeHash"].is_string());
    }

    #[test]
    fn coach_batches_are_registry_free() {
        let batch = issue_activation_codes("Kingsway Chess Academy", "coach", 60, 1, 1, None).unwrap();
        assert_eq!(batch.codes[0].kind, "coach");
        assert_eq!(batch.codes[0].org_id, school_code("Kingsway Chess Academy"));
    }

    #[test]
    fn rejects_impossible_batches() {
        assert!(issue_activation_codes("X", "coach", 0, 1, 1, None).is_err());
        assert!(issue_activation_codes("X", "coach", 60, 0, 1, None).is_err());
        assert!(issue_activation_codes("X", "coach", 60, 1, 0, None).is_err());
        assert!(issue_activation_codes("X", "coach", 60, 10_001, 1, None).is_err());
    }

    #[test]
    fn school_code_is_canonical_and_stable() {
        assert_eq!(
            school_code("  Port   Shepstone High School "),
            school_code("port shepstone high school")
        );
        assert_ne!(
            school_code("Port Shepstone High School"),
            school_code("Izotsha Primary School")
        );
    }

    #[test]
    fn keyring_roundtrip_and_public_key_derivation() {
        let dir = std::env::temp_dir().join(format!("ccu-keygen-test-{}", std::process::id()));
        let path = dir.join("keys.json");
        let ring = generate_keyring().unwrap();
        write_keyring_to(&path, &ring).unwrap();
        let loaded = load_keyring_from(&path).unwrap();
        assert_eq!(loaded.activation.kid, ACTIVATION_KID);
        assert_eq!(loaded.activation.public_key, ring.activation.public_key);
        check_entry(&loaded.activation).unwrap();
        let json = public_keys_json(&loaded);
        assert_eq!(
            json[ACTIVATION_KID].as_str().unwrap(),
            loaded.activation.public_key
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_v1_keyring_is_still_readable() {
        let dir = std::env::temp_dir().join(format!("ccu-keygen-v1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keys.json");
        let ring = generate_keyring().unwrap();
        let v1 = serde_json::json!({
            "version": 1,
            "file": { "kid": "file-2026", "publicKey": ring.activation.public_key, "privateKey": ring.activation.private_key },
            "activation": {
                "kid": ring.activation.kid,
                "publicKey": ring.activation.public_key,
                "privateKey": ring.activation.private_key
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&v1).unwrap()).unwrap();
        let loaded = load_keyring_from(&path).unwrap();
        assert_eq!(loaded.version, KEYRING_VERSION);
        assert_eq!(loaded.activation.private_key, ring.activation.private_key);
        // A corrupt public/private pair is refused.
        let mut bad = v1.clone();
        bad["activation"]["publicKey"] = serde_json::Value::String("00".repeat(32));
        std::fs::write(&path, serde_json::to_string_pretty(&bad).unwrap()).unwrap();
        assert!(load_keyring_from(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn platform_gate() {
        assert!(platform_allowed("linux"));
        assert!(!platform_allowed("macos"));
        assert!(!platform_allowed("windows"));
        assert!(!platform_allowed(""));
    }

    #[test]
    fn known_schools_parse() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ccu_keygen_test_schools_{}.json", std::process::id()));
        std::fs::write(
            &path,
            r##"{"_config":{"Port Shepstone High School":{"paletteColor":"#7c0000"}},"UGU":["Port Shepstone High School","Izotsha Primary School","Port Shepstone High School"]}"##,
        )
        .unwrap();
        let entries = school_entries(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].palette.as_deref(), Some("#7c0000"));
        assert_eq!(school_names(&path).unwrap().len(), 2);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn all_registered_school_codes_are_distinct() {
        let db = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public/database/index.json");
        let names = school_names(&db).expect("index.json must parse");
        assert!(names.len() >= 39);
        let mut codes: std::collections::BTreeMap<String, &str> = Default::default();
        for name in &names {
            let code = school_code(name);
            if let Some(other) = codes.get(&code) {
                panic!("school code collision: {other} and {name} both encode to {code}");
            }
            codes.insert(code, name);
        }
        assert_eq!(codes.len(), names.len());
    }
}
