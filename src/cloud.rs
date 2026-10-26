//! Open Cloud: publicar una versión del lugar, reiniciar servidores y correr Luau en una versión.
//!
//! Permisos de la clave (Creator Hub > Open Cloud > API Keys, con la experiencia elegida):
//!
//! ```text
//! publish   universe-places:write
//! restart   universe:write
//! run       universe.place.luau-execution-session:read y :write (y universe-places:write para subir la versión)
//! ```

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

const API: &str = "https://apis.roblox.com";
const DONE: [&str; 3] = ["COMPLETE", "FAILED", "CANCELLED"];

const BUSY_WAITS: [u64; 4] = [15, 30, 60, 90]; // segundos entre reintentos cuando Roblox está ocupado (409, 429, 5xx)

fn request(method: &str, url: &str, key: &str, body: Option<(&[u8], &str)>) -> Result<Value> {
    let waits = BUSY_WAITS.iter().map(|w| Some(*w)).chain([None]);
    for wait in waits {
        let req = ureq::request(method, url).set("x-api-key", key).timeout(Duration::from_secs(300));
        let res = match body {
            Some((bytes, ctype)) => req.set("Content-Type", ctype).send_bytes(bytes),
            None => req.call(),
        };
        match res {
            Ok(res) => {
                let raw = res.into_string().with_context(|| format!("{method} {url}"))?;
                if raw.is_empty() {
                    return Ok(json!({}));
                }
                return Ok(serde_json::from_str(&raw).unwrap_or_else(|_| Value::String(raw.trim().to_string())));
            }
            Err(ureq::Error::Status(code, res)) => {
                let detail: String = res.into_string().unwrap_or_default().chars().take(500).collect();
                if let Some(wait) = wait {
                    if code == 409 || code == 429 || code >= 500 {
                        eprintln!("Roblox ocupado ({code}); reintento en {wait} s");
                        sleep(Duration::from_secs(wait));
                        continue;
                    }
                }
                let hint =
                    if code == 401 || code == 403 { "  (¿la clave tiene el permiso y la experiencia?)" } else { "" };
                bail!("{method} {url}: {code} {detail}{hint}");
            }
            Err(e) => bail!("{method} {url}: {e}"),
        }
    }
    unreachable!("el último intento siempre devuelve")
}

/// Sube el archivo como versión nueva del lugar y devuelve su número.
pub fn publish(key: &str, universe: u64, place: u64, file: &Path, saved_only: bool) -> Result<u64> {
    let kind = if saved_only { "Saved" } else { "Published" };
    let xml = file.extension().is_some_and(|e| e.eq_ignore_ascii_case("rbxlx"));
    let ctype = if xml { "application/xml" } else { "application/octet-stream" };
    let url = format!("{API}/universes/v1/{universe}/places/{place}/versions?versionType={kind}");
    let bytes = fs::read(file).with_context(|| format!("{}", file.display()))?;
    let res = request("POST", &url, key, Some((&bytes, ctype)))?;
    let version = match &res {
        Value::Object(o) => o.get("versionNumber").and_then(Value::as_u64),
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    };
    version.ok_or_else(|| anyhow!("Roblox no devolvió el número de versión: {res}"))
}

pub fn restart(key: &str, universe: u64, places: Option<&[u64]>) -> Result<()> {
    let body = match places {
        Some(places) => json!({ "placeIds": places.iter().map(u64::to_string).collect::<Vec<_>>() }),
        None => json!({}),
    };
    let url = format!("{API}/cloud/v2/universes/{universe}:restartServers");
    request("POST", &url, key, Some((body.to_string().as_bytes(), "application/json")))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Task {
    pub state: String,
    pub results: Option<Value>,
    pub error: Option<Value>,
    pub logs: Vec<String>,
}

fn task_path(task: &Value) -> Result<String> {
    task["path"].as_str().map(str::to_string).ok_or_else(|| anyhow!("Roblox devolvió una tarea sin path: {task}"))
}

pub fn run(key: &str, universe: u64, place: u64, version: Option<u64>, script: &str, timeout: u32) -> Result<Task> {
    let mut at = format!("{API}/cloud/v2/universes/{universe}/places/{place}");
    if let Some(v) = version {
        at += &format!("/versions/{v}");
    }
    let body = json!({ "script": script, "timeout": format!("{timeout}s") }).to_string();
    let url = format!("{at}/luau-execution-session-tasks");
    let mut task = request("POST", &url, key, Some((body.as_bytes(), "application/json")))?;
    // la tarea se crea y corre aparte: se pregunta hasta que termina
    while !DONE.contains(&task["state"].as_str().unwrap_or("")) {
        sleep(Duration::from_secs(2));
        task = request("GET", &format!("{API}/cloud/v2/{}", task_path(&task)?), key, None)?;
    }
    let path = task_path(&task)?;
    Ok(Task {
        state: task["state"].as_str().unwrap_or_default().to_string(),
        results: task.pointer("/output/results").filter(|r| !r.is_null()).cloned(),
        error: task.get("error").filter(|e| !e.is_null()).cloned(),
        logs: logs(key, &path)?,
    })
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn logs(key: &str, path: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut token = String::new();
    loop {
        let mut url = format!("{API}/cloud/v2/{path}/logs?maxPageSize=10000");
        if !token.is_empty() {
            url += &format!("&pageToken={}", encode(&token));
        }
        let page = request("GET", &url, key, None)?;
        for entry in page["luauExecutionSessionTaskLogs"].as_array().into_iter().flatten() {
            for m in entry["messages"].as_array().into_iter().flatten() {
                out.push(m.as_str().map_or_else(|| m.to_string(), str::to_string));
            }
        }
        token = page["nextPageToken"].as_str().unwrap_or_default().to_string();
        if token.is_empty() {
            return Ok(out);
        }
    }
}
