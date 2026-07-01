//! Minimal Matrix client — posts a formatted message to a room via the
//! client-server API (same pattern as the paseo-monitoring matrix-bridge).
//! Configured via env: MATRIX_HOMESERVER, MATRIX_TOKEN, MATRIX_ROOM (a `#alias`
//! is resolved to a room id, or pass a `!roomid`). Disabled if any is missing.

use anyhow::{anyhow, Context, Result};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Matrix {
    http: reqwest::Client,
    homeserver: String,
    token: String,
    room_id: String,
    txn: AtomicU64,
}

impl Matrix {
    /// Configure and resolve the room. Returns `Ok(None)` if not configured.
    pub async fn connect(
        homeserver: Option<String>,
        token: Option<String>,
        room: Option<String>,
    ) -> Result<Option<Matrix>> {
        let (homeserver, token, room) = match (homeserver, token, room) {
            (Some(h), Some(t), Some(r)) if !h.is_empty() && !t.is_empty() && !r.is_empty() => {
                (h, t, r)
            }
            _ => return Ok(None),
        };
        let http = reqwest::Client::new();
        let hs = homeserver.trim_end_matches('/').to_string();
        let room_id = if room.starts_with('!') {
            room
        } else {
            // resolve alias (#name:server) -> room id
            let url = format!("{}/_matrix/client/v3/directory/room/{}", hs, enc(&room));
            let resp = http
                .get(&url)
                .bearer_auth(&token)
                .send()
                .await
                .context("resolving Matrix room alias")?;
            let v: serde_json::Value = resp.json().await.context("parsing alias resolution")?;
            v.get("room_id")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow!("could not resolve Matrix room alias {room}"))?
        };
        Ok(Some(Matrix {
            http,
            homeserver: hs,
            token,
            room_id,
            txn: AtomicU64::new(1),
        }))
    }

    /// Post an org.matrix.custom.html message (plain `text` + `html` body).
    pub async fn post(&self, text: &str, html: &str) -> Result<()> {
        let n = self.txn.fetch_add(1, Ordering::Relaxed);
        let url = format!(
            "{}/_matrix/client/v3/rooms/{}/send/m.room.message/paseo-{}",
            self.homeserver,
            enc(&self.room_id),
            n
        );
        let body = serde_json::json!({
            "msgtype": "m.text",
            "body": text,
            "format": "org.matrix.custom.html",
            "formatted_body": html,
        });
        let resp = self
            .http
            .put(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .context("Matrix send")?;
        if !resp.status().is_success() {
            let s = resp.status();
            let t = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Matrix send failed: {s} {t}"));
        }
        Ok(())
    }
}

/// Percent-encode the characters that aren't safe in a URL path segment
/// (room ids/aliases contain `#`, `:`, `!`).
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
