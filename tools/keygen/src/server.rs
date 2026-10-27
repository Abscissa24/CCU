// ── Activation service client ─────────────────────────────────────────
//
// The keygen talks to activation/server.mjs over HTTP(S):
//
//   GET  /health         public: key id, public key and counters
//   POST /admin/import   X-Admin-Token: register generated codes
//   POST /release        X-Admin-Token: free a device's seat
//   POST /admin/revoke   X-Admin-Token: disable an unused code
//
// Admin calls always need the token; the server refuses them otherwise.
// Requests are synchronous (this is a desktop operator tool, one action at
// a time) with a short timeout so a bad URL cannot freeze the GUI.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub kid: Option<String>,
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub codes: Option<u64>,
    #[serde(default)]
    pub activations: Option<u64>,
    #[serde(default)]
    pub warning: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(TIMEOUT).build()
}

fn describe(body: &Value, fallback: &str) -> String {
    body.get("message")
        .and_then(|m| m.as_str())
        .filter(|m| !m.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn json_response(result: Result<ureq::Response, ureq::Error>) -> Result<(u16, Value)> {
    match result {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.into_json::<Value>().unwrap_or(Value::Null);
            Ok((status, body))
        }
        Err(ureq::Error::Status(status, resp)) => {
            let body = resp.into_json::<Value>().unwrap_or(Value::Null);
            Ok((status, body))
        }
        Err(err) => bail!("cannot reach the activation service: {err}"),
    }
}

pub fn health(server: &str) -> Result<Health> {
    let (status, body) = json_response(agent().get(&format!("{server}/health")).call())?;
    if status != 200 {
        bail!(
            "activation service returned HTTP {status}: {}",
            describe(&body, "health check failed")
        );
    }
    serde_json::from_value(body).context("activation service health response is not understood")
}

/// Registers a whole batch. The server ignores `activationKid`/`generatedAt`
/// and reads `codes`, storing only each code's SHA-256 hash.
pub fn import_codes(server: &str, admin_token: &str, batch: &Value) -> Result<Value> {
    let (status, body) = json_response(
        agent()
            .post(&format!("{server}/admin/import"))
            .set("Content-Type", "application/json")
            .set("X-Admin-Token", admin_token)
            .send_string(&batch.to_string()),
    )?;
    if !(200..300).contains(&status) || body.get("ok") != Some(&Value::Bool(true)) {
        bail!(
            "activation service rejected the batch (HTTP {status}): {}",
            describe(&body, "import failed")
        );
    }
    Ok(body)
}

pub fn release_device(
    server: &str,
    admin_token: &str,
    code: &str,
    device_id: &str,
) -> Result<Value> {
    let payload = json!({ "code": code, "deviceId": device_id });
    let (status, body) = json_response(
        agent()
            .post(&format!("{server}/release"))
            .set("Content-Type", "application/json")
            .set("X-Admin-Token", admin_token)
            .send_string(&payload.to_string()),
    )?;
    if !(200..300).contains(&status) || body.get("ok") != Some(&Value::Bool(true)) {
        bail!(
            "activation service rejected the release (HTTP {status}): {}",
            describe(&body, "release failed")
        );
    }
    Ok(body)
}

pub fn revoke_code(server: &str, admin_token: &str, code: &str) -> Result<Value> {
    let payload = json!({ "code": code });
    let (status, body) = json_response(
        agent()
            .post(&format!("{server}/admin/revoke"))
            .set("Content-Type", "application/json")
            .set("X-Admin-Token", admin_token)
            .send_string(&payload.to_string()),
    )?;
    if !(200..300).contains(&status) || body.get("ok") != Some(&Value::Bool(true)) {
        bail!(
            "activation service rejected the revoke (HTTP {status}): {}",
            describe(&body, "revoke failed")
        );
    }
    Ok(body)
}
