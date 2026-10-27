// ── ccu-keygen GUI shell (Linux-only, WebKitGTK webview) ──────────────
//
// The GUI itself is a self-contained HTML/CSS/JS page (gui.html) that
// mirrors the CCU app's welcome school selector: same design tokens, same
// card grid, same modal language. This file is only the window shell +
// IPC bridge:
//
//   JS  ──window.ipc.postMessage({id, cmd, args})──▶  Rust (handle_ipc)
//   JS  ◀──────────window.__ccuResp({id, ok, ...})─────  Rust (evaluate_script)
//
// Commands:
//   init / reload   schools + saved service configuration
//   logo            lazy school logo
//   generate_codes  build a one-time activation batch
//   publish_codes   register that batch with the activation service
//   service_status  health + key-match check
//   save_settings   persist the service URL / admin token
//   pickdir         GTK folder chooser for another database folder
//   save_file       GTK save dialog for the batch
//
// The shell is plain GTK3: the webview is attached via wry's
// WebViewBuilderExtUnix::build_gtk, which works on both X11 and Wayland
// (the window-handle path is X11-only). Everything runs on the GTK main
// thread, so the webview handle never crosses a thread boundary.

use crate::{
    format_remaining, iso8601, issue_activation_codes, load_keyring, platform_allowed,
    read_setting, resolve_admin_token, resolve_server, school_entries, school_logo_path,
    write_setting, SchoolEntry,
};
use crate::server;
use anyhow::{anyhow, Result};
use base64::Engine as _;
use gtk::prelude::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use wry::WebViewBuilder;
use wry::WebViewBuilderExtUnix;

const INDEX_HTML: &str = include_str!("gui.html");

// ── Database folder persistence ───────────────────────────────────────

/// Folder previously chosen via the settings dialog, if it still holds an
/// index.json.
fn persisted_db_folder() -> Option<PathBuf> {
    let folder = PathBuf::from(read_setting("db-path")?);
    if folder.join("index.json").exists() {
        Some(folder)
    } else {
        None
    }
}

fn persist_db_folder(folder: &Path) {
    let _ = write_setting("db-path", &folder.display().to_string());
}

// ── Service configuration ─────────────────────────────────────────────

/// Non-fatal status shown in the GUI footer. `online` separates "the
/// command worked" from "the service answered", so an offline server does
/// not look like a broken app.
fn service_status_value() -> Value {
    let url = resolve_server(None);
    let token_configured = resolve_admin_token(None).is_some();
    let keyring = load_keyring().ok();
    let kid = keyring.as_ref().map(|r| r.activation.kid.clone());
    let local_public = keyring.as_ref().map(|r| r.activation.public_key.clone());
    match server::health(&url) {
        Ok(h) => {
            let key_match = match (&h.public_key, &local_public) {
                (Some(remote), Some(local)) => remote.eq_ignore_ascii_case(local),
                _ => false,
            };
            json!({
                "ok": true,
                "online": true,
                "url": url,
                "tokenConfigured": token_configured,
                "keyring": keyring.is_some(),
                "kid": h.kid.or(kid),
                "publicKey": h.public_key,
                "codes": h.codes,
                "activations": h.activations,
                "keyMatch": key_match,
                "warning": h.warning,
            })
        }
        Err(e) => json!({
            "ok": true,
            "online": false,
            "url": url,
            "tokenConfigured": token_configured,
            "keyring": keyring.is_some(),
            "kid": kid,
            "error": format!("{e:#}"),
        }),
    }
}

// ── IPC ───────────────────────────────────────────────────────────────

fn logo_data_url(index_json: &Path, entry: &SchoolEntry) -> Option<String> {
    let bytes = std::fs::read(school_logo_path(index_json, entry)).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn init_payload(schools_path: &Path) -> Value {
    let (schools, error) = match school_entries(schools_path) {
        Ok(v) => (v, None),
        Err(e) => (Vec::new(), Some(format!("{e:#}"))),
    };
    // Logos are deliberately NOT embedded here: base64-embedding every
    // school logo makes the init response ~1MB, which freezes the webview
    // on boot. The JS fetches each logo lazily via the "logo" command and
    // caches it, so boot is instant and each fetch is a tiny round-trip.
    let keyring = load_keyring().ok();
    json!({
        "schools": schools.iter().map(|e| json!({
            "name": e.name,
            "district": e.district,
            "palette": e.palette,
        })).collect::<Vec<_>>(),
        "platform": if platform_allowed(std::env::consts::OS) { "ok" } else { "refused" },
        "error": error,
        "service": {
            "url": resolve_server(None),
            "tokenConfigured": resolve_admin_token(None).is_some(),
            "keyring": keyring.is_some(),
            "kid": keyring.as_ref().map(|r| r.activation.kid.clone()),
        },
    })
}

fn handle_ipc(body: &str, schools_path: &Path) -> Value {
    let msg: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return json!({ "ok": false, "error": "malformed request" }),
    };
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let cmd = msg.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
    let args = msg.get("args").cloned().unwrap_or(Value::Null);
    let resp = match cmd {
        "init" | "reload" => json!({ "ok": true, "data": init_payload(schools_path) }),
        "logo" => {
            let school = args.get("school").and_then(|v| v.as_str()).unwrap_or("");
            let found = school_entries(schools_path)
                .ok()
                .and_then(|entries| entries.into_iter().find(|e| e.name == school))
                .and_then(|e| logo_data_url(schools_path, &e));
            match found {
                Some(url) => json!({ "ok": true, "url": url }),
                None => json!({ "ok": false, "error": "no logo for this school" }),
            }
        }
        "generate_codes" => {
            let school = args.get("school").and_then(|v| v.as_str()).unwrap_or("").trim();
            let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("school");
            let minutes = args.get("minutes").and_then(|v| v.as_u64()).unwrap_or(0);
            let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let seats = args.get("seats").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let order = args
                .get("order")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if school.is_empty() || minutes == 0 || count == 0 {
                json!({ "ok": false, "error": "school, count and a positive duration are required" })
            } else if kind != "coach" {
                // A school code is bound to the exact school name. Checking
                // the registry here catches typos before a customer ever
                // sees the code; coaching businesses are free-form.
                let known = school_entries(schools_path)
                    .map(|entries| entries.into_iter().any(|e| e.name == school))
                    .unwrap_or(false);
                if !known {
                    json!({ "ok": false, "error": format!("school \"{school}\" is not in the schools file — choose it from the grid or switch to Coaching") })
                } else {
                    generate_codes_value(school, kind, minutes, count, seats, order)
                }
            } else {
                generate_codes_value(school, kind, minutes, count, seats, order)
            }
        }
        "publish_codes" => {
            let batch = args.get("batch").cloned().unwrap_or(Value::Null);
            let count = batch
                .get("codes")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if count == 0 {
                json!({ "ok": false, "error": "no generated codes to register" })
            } else if let Some(token) = resolve_admin_token(None) {
                let url = resolve_server(None);
                match server::import_codes(&url, &token, &batch) {
                    Ok(result) => json!({ "ok": true, "server": url, "result": result }),
                    Err(e) => json!({ "ok": false, "error": format!("{e:#}") }),
                }
            } else {
                json!({ "ok": false, "error": "No admin token is saved. Open Service settings and paste it once." })
            }
        }
        "service_status" => json!({ "ok": true, "status": service_status_value() }),
        "save_settings" => {
            let server = args.get("server").and_then(|v| v.as_str()).unwrap_or("").trim();
            let token = args
                .get("adminToken")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or("");
            let clear_token = args
                .get("clearToken")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !server.is_empty() && !server.starts_with("http://") && !server.starts_with("https://") {
                json!({ "ok": false, "error": "The service URL must start with https:// (or http:// for local testing)." })
            } else {
                let saved = (|| -> Result<()> {
                    write_setting("server", server)?;
                    if clear_token {
                        write_setting("admin-token", "")?;
                    } else if !token.is_empty() {
                        write_setting("admin-token", token)?;
                    }
                    Ok(())
                })();
                match saved {
                    Ok(()) => json!({ "ok": true, "status": service_status_value() }),
                    Err(e) => json!({ "ok": false, "error": format!("{e:#}") }),
                }
            }
        }
        other => json!({ "ok": false, "error": format!("unknown command: {other}") }),
    };
    json!({ "id": id, "resp": resp })
}

fn generate_codes_value(
    school: &str,
    kind: &str,
    minutes: u64,
    count: usize,
    seats: u32,
    order: Option<String>,
) -> Value {
    match issue_activation_codes(school, kind, minutes, count.min(500), seats, order) {
        Ok(batch) => {
            let codes = batch.codes.iter().map(|c| c.code.clone()).collect::<Vec<_>>();
            let expires_at = batch.codes.first().map(|c| c.expires_at).unwrap_or(0);
            let org_id = batch.codes.first().map(|c| c.org_id.clone()).unwrap_or_default();
            json!({
                "ok": true,
                "codes": codes,
                "batch": batch,
                "orgId": org_id,
                "expiryMs": expires_at,
                "iso": iso8601(expires_at),
                "remaining": format_remaining(expires_at),
            })
        }
        Err(e) => json!({ "ok": false, "error": format!("{e:#}") }),
    }
}

// ── Window shell ──────────────────────────────────────────────────────

/// GTK save dialog. Runs on the GTK main thread (as does every IPC handler),
/// so blocking is fine here.
fn save_text_file(window: &gtk::Window, title: &str, default_name: &str, content: &str) -> Value {
    use gtk::FileChooserAction;
    use gtk::ResponseType;

    let dialog = gtk::FileChooserDialog::new(Some(title), Some(window), FileChooserAction::Save);
    dialog.add_buttons(&[
        ("Cancel", ResponseType::Cancel),
        ("Save", ResponseType::Accept),
    ]);
    dialog.set_do_overwrite_confirmation(true);
    dialog.set_current_name(default_name);
    let result = match dialog.run() {
        ResponseType::Accept => match dialog.filename() {
            Some(path) => match std::fs::write(&path, content) {
                Ok(()) => json!({ "ok": true, "path": path.display().to_string() }),
                Err(e) => json!({ "ok": false, "error": format!("cannot write {}: {e}", path.display()) }),
            },
            None => json!({ "ok": false, "error": "no file selected" }),
        },
        _ => json!({ "ok": false, "cancelled": true }),
    };
    unsafe { dialog.destroy() };
    result
}

fn pick_schools_dir(window: &gtk::Window, path_cell: &Rc<RefCell<PathBuf>>) -> Value {
    use gtk::FileChooserAction;
    use gtk::ResponseType;

    let dialog = gtk::FileChooserDialog::new(
        Some("Select database folder (contains index.json)"),
        Some(window),
        FileChooserAction::SelectFolder,
    );
    dialog.add_buttons(&[
        ("Cancel", ResponseType::Cancel),
        ("Select", ResponseType::Accept),
    ]);
    let current = path_cell.borrow().parent().map(|p| p.to_path_buf());
    if let Some(dir) = current {
        dialog.set_current_folder(&dir);
    }

    let result = match dialog.run() {
        ResponseType::Accept => match dialog.filename() {
            Some(folder) => {
                let index = folder.join("index.json");
                if index.exists() {
                    *path_cell.borrow_mut() = index.clone();
                    persist_db_folder(&folder);
                    json!({ "ok": true, "data": init_payload(&index), "folder": folder.display().to_string() })
                } else {
                    json!({ "ok": false, "error": "selected folder has no index.json" })
                }
            }
            None => json!({ "ok": false, "error": "no folder selected" }),
        },
        _ => json!({ "ok": false, "cancelled": true }),
    };
    unsafe { dialog.destroy() };
    result
}

pub fn run_gui(schools_path: PathBuf) -> Result<()> {
    if !platform_allowed(std::env::consts::OS) {
        return Err(anyhow!(
            "ccu-keygen only runs on Linux. This host is {}; refusing to run.",
            std::env::consts::OS
        ));
    }
    gtk::init().map_err(|e| anyhow!("GTK init failed: {e}"))?;

    // A folder picked via the settings dialog wins over the exe/CWD
    // resolution — it is remembered across sessions.
    let schools_path = persisted_db_folder()
        .map(|f| f.join("index.json"))
        .filter(|p| p.exists())
        .unwrap_or(schools_path);

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("CCU · Activation Key Generator");
    window.set_default_size(1400, 950);
    window.set_size_request(1075, 750);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        gtk::glib::Propagation::Stop
    });

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // The IPC handler runs on the GTK main thread, so the webview handle
    // (not Send) can be stashed in an Rc and borrowed from there — it
    // never crosses a thread boundary. Code generation is fast; the only
    // blocking call is a service check with a short network timeout.
    let holder = Rc::new(RefCell::new(None::<wry::WebView>));
    let path_cell = Rc::new(RefCell::new(schools_path));
    let dialog_parent = window.clone();
    let builder = WebViewBuilder::new()
        .with_html(INDEX_HTML)
        .with_ipc_handler({
            let holder = Rc::clone(&holder);
            let path_cell = Rc::clone(&path_cell);
            move |req| {
                // "pickdir" needs the window + mutable path, so it is handled
                // here rather than in the pure handle_ipc.
                let body = req.body();
                let msg: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                let id = msg.get("id").cloned().unwrap_or(Value::Null);
                let cmd = msg.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
                let out = if cmd == "pickdir" {
                    // pick_schools_dir returns the bare response — wrap it here.
                    json!({ "id": id, "resp": pick_schools_dir(&dialog_parent, &path_cell) }).to_string()
                } else if cmd == "save_file" {
                    let title = msg.get("args").and_then(|a| a.get("title")).and_then(|v| v.as_str()).unwrap_or("Save file");
                    let name = msg.get("args").and_then(|a| a.get("filename")).and_then(|v| v.as_str()).unwrap_or("activation-codes.json");
                    let content = msg.get("args").and_then(|a| a.get("content")).and_then(|v| v.as_str()).unwrap_or("");
                    json!({ "id": id, "resp": save_text_file(&dialog_parent, title, name, content) }).to_string()
                } else {
                    // handle_ipc already returns the full {id, resp} wrapper.
                    handle_ipc(body, &path_cell.borrow().clone()).to_string()
                };
                if let Some(wv) = holder.borrow().as_ref() {
                    let _ = wv.evaluate_script(&format!("window.__ccuResp({out})"));
                }
            }
        });

    let webview = builder
        .build_gtk(&vbox)
        .map_err(|e| anyhow!("cannot create webview: {e:#}"))?;
    *holder.borrow_mut() = Some(webview);

    window.add(&vbox);
    window.show_all();

    gtk::main();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schools_path() -> PathBuf {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../public/database/index.json");
        assert!(p.exists(), "index.json must exist for IPC tests");
        p
    }

    #[test]
    fn ipc_init_lists_schools_and_service_state() {
        let path = schools_path();
        let payload = handle_ipc(r#"{"id":1,"cmd":"init"}"#, &path).to_string();
        let v: Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(v["resp"]["ok"], true);
        let schools = v["resp"]["data"]["schools"].as_array().unwrap();
        assert!(schools.len() >= 39);
        // logos are lazy now — init must stay small
        assert!(v["resp"]["data"].get("logos").is_none());
        assert!(v["resp"]["data"].get("fingerprint").is_none());
        assert!(v["resp"]["data"].get("format").is_none());
        // service config is present but no network call is made on boot
        assert!(v["resp"]["data"]["service"].get("url").is_some());
        assert!(v["resp"]["data"]["service"].get("tokenConfigured").is_some());
    }

    #[test]
    fn ipc_logo_lazy_load() {
        let path = schools_path();
        let body = r#"{"id":5,"cmd":"logo","args":{"school":"Creston College"}}"#;
        let v: Value = serde_json::from_str(&handle_ipc(body, &path).to_string()).unwrap();
        let r = &v["resp"];
        assert_eq!(r["ok"], true);
        let url = r["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        // unknown school → ok:false
        let bad = r#"{"id":6,"cmd":"logo","args":{"school":"No Such School"}}"#;
        let v: Value = serde_json::from_str(&handle_ipc(bad, &path).to_string()).unwrap();
        assert_eq!(v["resp"]["ok"], false);
    }

    #[test]
    fn ipc_generate_activation_codes() {
        let path = schools_path();
        let body = r#"{"id":2,"cmd":"generate_codes","args":{"school":"Port Shepstone High School","kind":"school","minutes":1440,"count":2,"seats":1,"order":"INV-7"}}"#;
        let v: Value = serde_json::from_str(&handle_ipc(body, &path).to_string()).unwrap();
        let r = &v["resp"];
        assert_eq!(r["ok"], true, "{}", r["error"]);
        let codes = r["codes"].as_array().unwrap();
        assert_eq!(codes.len(), 2);
        for code in codes {
            let code = code.as_str().unwrap();
            assert_eq!(code.len(), 23); // 20 chars + 3 dashes
            assert!(code.starts_with("CCU5"));
        }
        assert_eq!(r["batch"]["codes"].as_array().unwrap().len(), 2);
        assert_eq!(r["batch"]["codes"][0]["orderId"], "INV-7");
        assert_eq!(r["batch"]["codes"][0]["kind"], "school");
        assert_eq!(r["orgId"].as_str().unwrap().len(), 6);
        assert!(r["iso"].is_string());
        assert!(r["remaining"].is_string());
    }

    #[test]
    fn ipc_generate_codes_rejects_an_unknown_school() {
        let path = schools_path();
        let body = r#"{"id":3,"cmd":"generate_codes","args":{"school":"No Such School","kind":"school","minutes":1440,"count":1}}"#;
        let v: Value = serde_json::from_str(&handle_ipc(body, &path).to_string()).unwrap();
        assert_eq!(v["resp"]["ok"], false);
        assert!(v["resp"]["error"].as_str().unwrap().contains("not in the schools file"));
    }

    #[test]
    fn ipc_generate_codes_for_a_coach_business() {
        // Coaching mode issues codes for any business name — the IPC handler
        // is registry-free for kind=coach, exactly like `gen-codes --coach`.
        let path = schools_path();
        let body = r#"{"id":4,"cmd":"generate_codes","args":{"school":"Kingsway Chess Academy","kind":"coach","minutes":43200,"count":1}}"#;
        let v: Value = serde_json::from_str(&handle_ipc(body, &path).to_string()).unwrap();
        let r = &v["resp"];
        assert_eq!(r["ok"], true, "{}", r["error"]);
        assert_eq!(r["batch"]["codes"][0]["kind"], "coach");
        assert_eq!(r["batch"]["codes"][0]["org"], "Kingsway Chess Academy");
    }

    #[test]
    fn ipc_rejects_bad_input_and_bad_urls() {
        let path = schools_path();
        let v: Value = serde_json::from_str(&handle_ipc("not json", &path).to_string()).unwrap();
        assert_eq!(v["ok"], false); // no id wrapper for unparseable input
        let v: Value = serde_json::from_str(
            &handle_ipc(r#"{"id":9,"cmd":"nope"}"#, &path).to_string(),
        )
        .unwrap();
        assert_eq!(v["resp"]["ok"], false);

        let body = r#"{"id":10,"cmd":"save_settings","args":{"server":"ftp://example.com","adminToken":"x"}}"#;
        let v: Value = serde_json::from_str(&handle_ipc(body, &path).to_string()).unwrap();
        assert_eq!(v["resp"]["ok"], false);
        assert!(v["resp"]["error"].as_str().unwrap().contains("https://"));
    }
}
