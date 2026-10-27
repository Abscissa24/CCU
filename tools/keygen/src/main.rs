// ── ccu-keygen CLI + GUI launcher (Linux only — see lib.rs) ────────────

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use ccu_keygen::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod gui;
mod server;

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Smart resolution: check the usual repository layouts (run from the
/// project root, from tools/keygen, the cargo target directory, or an
/// installed releases/keygen binary), then fall back to the CWD.
fn default_schools_path() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let mut candidates = vec![
        cwd.join("public/database/index.json"),
        cwd.join("database/index.json"),
        cwd.join("../../public/database/index.json"),
    ];
    if let Some(dir) = exe_dir {
        candidates.extend([
            dir.join("../../../../public/database/index.json"), // target/release
            dir.join("../../public/database/index.json"),       // releases/keygen
            dir.join("../public/database/index.json"),
            dir.join("database/index.json"),
        ]);
    }
    for candidate in candidates {
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("public/database/index.json")
}

#[derive(Args)]
struct DurationArgs {
    /// Duration in minutes
    #[arg(long, default_value_t = 1440, conflicts_with_all = ["hours", "days", "months", "years"])]
    minutes: u64,
    #[arg(long, conflicts_with_all = ["minutes", "days", "months", "years"])]
    hours: Option<u64>,
    #[arg(long, conflicts_with_all = ["minutes", "hours", "months", "years"])]
    days: Option<u64>,
    #[arg(long, conflicts_with_all = ["minutes", "hours", "days", "years"])]
    months: Option<u64>,
    #[arg(long, conflicts_with_all = ["minutes", "hours", "days", "months"])]
    years: Option<u64>,
}

impl DurationArgs {
    fn total_minutes(&self) -> u64 {
        if let Some(h) = self.hours {
            h * 60
        } else if let Some(d) = self.days {
            d * 1440
        } else if let Some(m) = self.months {
            m * 43200
        } else if let Some(y) = self.years {
            y * 525600
        } else {
            self.minutes.max(1)
        }
    }

    fn label(&self) -> String {
        if let Some(h) = self.hours {
            format!("{h} hour(s)")
        } else if let Some(d) = self.days {
            format!("{d} day(s)")
        } else if let Some(m) = self.months {
            format!("{m} month(s)")
        } else if let Some(y) = self.years {
            format!("{y} year(s)")
        } else {
            format!("{} minute(s)", self.minutes.max(1))
        }
    }
}

#[derive(Parser)]
#[command(
    name = "ccu-keygen",
    version,
    about = "Linux-only activation key generator for Chess Coach Utility"
)]
struct Cli {
    /// Launch the graphical interface (also the default when no command is given)
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Launch the graphical interface
    Gui,
    /// Create the Ed25519 activation signing key pair (kept on this machine)
    Init {
        /// Replace the signing key (invalidates every licence already issued)
        #[arg(long)]
        force: bool,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Show the public activation key to embed in the app
    PublicKeys {
        #[arg(long)]
        json: bool,
    },
    /// Export the private activation key for the activation service
    ExportActivationKey {
        /// Write the private key to this file instead of standard output
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Generate one-time activation codes (one per device/seat)
    #[command(alias = "gen")]
    GenCodes {
        /// School name (must appear in the schools file)
        #[arg(long, conflicts_with = "coach", required_unless_present = "coach")]
        school: Option<String>,
        /// Coaching business name for private coaching keys (no registry check)
        #[arg(long, conflicts_with = "school", required_unless_present = "school")]
        coach: Option<String>,
        /// Schools list (database/index.json); auto-detected by default
        #[arg(long)]
        schools: Option<PathBuf>,
        #[command(flatten)]
        duration: DurationArgs,
        /// How many codes to generate
        #[arg(long, default_value_t = 1)]
        count: usize,
        /// Seats on each code (default 1 = one device)
        #[arg(long, default_value_t = 1)]
        seats: u32,
        /// Optional order/customer reference
        #[arg(long)]
        order: Option<String>,
        /// Write the activation batch JSON here (otherwise it is only printed)
        #[arg(long)]
        out: Option<PathBuf>,
        /// Register the codes with the activation service immediately
        #[arg(long)]
        publish: bool,
        /// Activation service base URL (default: CCU_ACTIVATION_URL or the saved setting)
        #[arg(long)]
        server: Option<String>,
        /// Admin token for --publish (default: CCU_ADMIN_TOKEN or the saved setting)
        #[arg(long)]
        admin_token: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Check the activation service and whether its key matches this keyring
    Status {
        /// Activation service base URL
        #[arg(long)]
        server: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Free a seat so a replacement device can activate
    Release {
        /// The activation code
        #[arg(long)]
        code: String,
        /// Device ID shown in the app (Settings → Support → Device ID)
        #[arg(long)]
        device: String,
        /// Activation service base URL
        #[arg(long)]
        server: Option<String>,
        /// Admin token (default: CCU_ADMIN_TOKEN or the saved setting)
        #[arg(long)]
        admin_token: Option<String>,
    },
    /// Disable an activation code that has not been used yet
    Revoke {
        /// The activation code
        #[arg(long)]
        code: String,
        /// Activation service base URL
        #[arg(long)]
        server: Option<String>,
        /// Admin token (default: CCU_ADMIN_TOKEN or the saved setting)
        #[arg(long)]
        admin_token: Option<String>,
    },
    /// List schools from the schools file
    List {
        /// Schools list (database/index.json)
        #[arg(long)]
        schools: Option<PathBuf>,
    },
}

fn resolve_identity(
    school: &Option<String>,
    coach: &Option<String>,
    schools: &Option<PathBuf>,
    require_registry: bool,
) -> Result<(String, bool)> {
    let (name, is_coach) = match (school, coach) {
        (Some(s), _) => (s.clone(), false),
        (None, Some(c)) => (c.clone(), true),
        _ => unreachable!("clap requires --school or --coach"),
    };
    if !is_coach && require_registry {
        let schools = schools.clone().unwrap_or_else(default_schools_path);
        let names = school_names(&schools)?;
        require_known_school(&name, &names)?;
    }
    Ok((name, is_coach))
}

fn require_token(explicit: Option<&str>, action: &str) -> Result<String> {
    resolve_admin_token(explicit).ok_or_else(|| {
        anyhow::anyhow!(
            "an admin token is required to {action}. Set CCU_ADMIN_TOKEN, pass --admin-token, \
             or save it once in the GUI's service settings."
        )
    })
}

fn write_batch(path: &Path, batch: &ActivationBatch) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(batch)? + "\n")
        .with_context(|| format!("cannot write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    // Hard platform gate — the generator must never be operated off-Linux.
    if !platform_allowed(std::env::consts::OS) {
        bail!(
            "ccu-keygen only runs on Linux. This host is {}; refusing to run.",
            std::env::consts::OS
        );
    }

    match cli.command {
        None | Some(Command::Gui) => gui::run_gui(default_schools_path()),

        Some(Command::Init { force, json }) => {
            if force && keyring_path().exists() {
                eprintln!(
                    "warning: --force replaces the existing activation key; every licence already issued will stop verifying."
                );
            }
            let (ring, path, created) = init_keyring(force)?;
            if json {
                println!(
                    "{}",
                    json!({
                        "created": created,
                        "key_file": path.display().to_string(),
                        "public_keys": public_keys_json(&ring),
                        "warning": "Back up the key file. It is the only key that can sign licences."
                    })
                );
            } else {
                println!(
                    "{} {}",
                    if created { "Created" } else { "Using existing" },
                    path.display()
                );
                println!();
                println!("Public key (safe to embed in the app):");
                println!("{}", serde_json::to_string_pretty(&public_keys_json(&ring))?);
                println!();
                if created {
                    println!("Back up this file now. Without it you cannot issue or renew licences:");
                    println!("  {}", path.display());
                    println!();
                    println!("Start the activation service with its private key:");
                    println!("  ccu-keygen export-activation-key --out activation.key");
                    println!();
                    println!("Then put the public key above into core/licenseConfig.js and rebuild.");
                } else {
                    println!("Existing activation key kept — the app and service keep trusting it.");
                }
            }
            Ok(())
        }

        Some(Command::PublicKeys { json: _ }) => {
            let ring = load_keyring()?;
            println!("{}", serde_json::to_string_pretty(&public_keys_json(&ring))?);
            Ok(())
        }

        Some(Command::ExportActivationKey { out }) => {
            let ring = load_keyring()?;
            match out {
                Some(path) => {
                    std::fs::write(&path, format!("{}\n", ring.activation.private_key))
                        .with_context(|| format!("cannot write {}", path.display()))?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = std::fs::set_permissions(
                            &path,
                            std::fs::Permissions::from_mode(0o600),
                        );
                    }
                    println!("Wrote activation private key to {}", path.display());
                }
                None => {
                    eprintln!("warning: this is a private key; redirect it to a protected file");
                    println!("{}", ring.activation.private_key);
                }
            }
            Ok(())
        }

        Some(Command::GenCodes {
            school,
            coach,
            schools,
            duration,
            count,
            seats,
            order,
            out,
            publish,
            server: server_arg,
            admin_token,
            json,
        }) => {
            let (name, is_coach) = resolve_identity(&school, &coach, &schools, true)?;
            let batch = issue_activation_codes(
                &name,
                if is_coach { "coach" } else { "school" },
                duration.total_minutes(),
                count,
                seats,
                order,
            )?;
            if let Some(path) = out.as_ref() {
                write_batch(path, &batch)?;
            }

            let server_url = resolve_server(server_arg.as_deref());
            let published = if publish {
                let token = require_token(admin_token.as_deref(), "register codes")?;
                let value = serde_json::to_value(&batch)?;
                match server::import_codes(&server_url, &token, &value) {
                    Ok(result) => Some(result),
                    Err(e) => {
                        // Never lose a freshly generated batch because the
                        // network or the admin token failed: keep the codes
                        // and make the failed registration obvious.
                        let fallback = match out.clone() {
                            Some(path) => path,
                            None => {
                                let dir = keygen_dir().join("unregistered");
                                std::fs::create_dir_all(&dir).with_context(|| {
                                    format!("cannot create {}", dir.display())
                                })?;
                                let path = dir.join(format!("batch-{}.json", epoch_secs()));
                                write_batch(&path, &batch)?;
                                path
                            }
                        };
                        eprintln!("error: could not register the batch: {e:#}");
                        eprintln!(
                            "The codes below are NOT active yet. The batch is saved at {}.",
                            fallback.display()
                        );
                        for code in batch.plain_codes() {
                            eprintln!("  {code}");
                        }
                        bail!(
                            "registration failed; the batch was saved to {}",
                            fallback.display()
                        );
                    }
                }
            } else {
                None
            };

            let expires_at = batch.codes.first().map(|c| c.expires_at).unwrap_or(0);
            if json {
                let codes = batch.plain_codes();
                println!(
                    "{}",
                    json!({
                        "kind": batch.codes.first().map(|c| c.kind.clone()).unwrap_or_default(),
                        "org": name,
                        "count": codes.len(),
                        "codes": codes,
                        "expires_at_utc": iso8601(expires_at),
                        "remaining": format_remaining(expires_at),
                        "server": server_url,
                        "published": published,
                        "out": out.as_ref().map(|p| p.display().to_string()),
                        "batch": batch,
                    })
                );
            } else {
                println!("{}: {}", if is_coach { "Coach" } else { "School" }, name);
                println!(
                    "Codes:    {} × {} seat(s) · {} · expires {} ({})",
                    batch.codes.len(),
                    seats.max(1),
                    duration.label(),
                    iso8601(expires_at),
                    format_remaining(expires_at)
                );
                println!();
                for code in batch.plain_codes() {
                    println!("  {code}");
                }
                println!();
                if let Some(saved) = out.as_ref() {
                    println!("Batch saved to {}", saved.display());
                } else if publish {
                    println!("(not saved — pass --out to keep a copy)");
                }
                if let Some(result) = published.as_ref() {
                    println!(
                        "Registered with {}: imported {}, skipped {}, {} code(s) on the service.",
                        server_url,
                        result.get("imported").and_then(|v| v.as_u64()).unwrap_or(0),
                        result.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0),
                        result.get("totalCodes").and_then(|v| v.as_u64()).unwrap_or(0)
                    );
                } else {
                    println!("Not registered yet. Either re-run with --publish, or import the");
                    println!("batch into the activation service (POST /admin/import).");
                }
                println!();
                println!("Give one code per device. The service stores only hashes, so delete");
                println!("the plaintext batch file after handing the codes out.");
            }
            Ok(())
        }

        Some(Command::Status {
            server: server_arg,
            json,
        }) => {
            let server_url = resolve_server(server_arg.as_deref());
            let health = server::health(&server_url)?;
            let local = load_keyring().ok();
            let key_match = match (&health.public_key, &local) {
                (Some(remote), Some(ring)) => {
                    remote.eq_ignore_ascii_case(&ring.activation.public_key)
                }
                _ => false,
            };
            if json {
                println!(
                    "{}",
                    json!({
                        "server": server_url,
                        "ok": health.ok,
                        "service": health.service,
                        "kid": health.kid,
                        "public_key": health.public_key,
                        "local_public_key": local.as_ref().map(|r| r.activation.public_key.clone()),
                        "key_match": key_match,
                        "codes": health.codes,
                        "activations": health.activations,
                        "warning": health.warning,
                    })
                );
            } else {
                println!("Activation service: {server_url}");
                println!("  status:      {}", if health.ok { "OK" } else { "DEGRADED" });
                println!(
                    "  key id:      {}",
                    health.kid.as_deref().unwrap_or("unknown")
                );
                println!(
                    "  public key:  {}",
                    health.public_key.as_deref().unwrap_or("unknown")
                );
                println!(
                    "  codes:       {} (activations: {})",
                    health.codes.unwrap_or(0),
                    health.activations.unwrap_or(0)
                );
                if let Some(ring) = &local {
                    println!(
                        "  keyring:     {} — {}",
                        ring.activation.kid,
                        if key_match {
                            "matches the service"
                        } else {
                            "DOES NOT MATCH the service — licences will be rejected!"
                        }
                    );
                } else {
                    println!("  keyring:     not found on this machine");
                }
                if let Some(warning) = health.warning.as_deref() {
                    println!("  warning:     {warning}");
                }
            }
            Ok(())
        }

        Some(Command::Release {
            code,
            device,
            server: server_arg,
            admin_token,
        }) => {
            let server_url = resolve_server(server_arg.as_deref());
            let token = require_token(admin_token.as_deref(), "release a seat")?;
            let result = server::release_device(&server_url, &token, &code, &device)?;
            println!(
                "Released {} from {} — remaining seats: {}",
                device,
                server_url,
                result
                    .get("remainingSeats")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
            );
            Ok(())
        }

        Some(Command::Revoke {
            code,
            server: server_arg,
            admin_token,
        }) => {
            let server_url = resolve_server(server_arg.as_deref());
            let token = require_token(admin_token.as_deref(), "revoke a code")?;
            let result = server::revoke_code(&server_url, &token, &code)?;
            println!(
                "Revoked {} on {} — code revoked: {}",
                format_activation_code(&code),
                server_url,
                result
                    .get("revoked")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
            );
            Ok(())
        }

        Some(Command::List { schools }) => {
            let schools = schools.unwrap_or_else(default_schools_path);
            for (i, name) in school_names(&schools)?.iter().enumerate() {
                println!("{:>3}  {}", i + 1, name);
            }
            Ok(())
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
