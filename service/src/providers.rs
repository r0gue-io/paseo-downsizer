//! Loads the LOCAL provider directory (`providers.toml`) and renders the
//! per-cycle Matrix offboarding notice. This file carries the named per-provider
//! cohorts and is intentionally kept OUT of the public repo (gitignored); it is
//! deployed to the host via scp. If it isn't present, notifications are disabled.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Providers {
    #[serde(default)]
    pub meta: Meta,
    #[serde(default, rename = "provider")]
    pub providers: Vec<Provider>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Meta {
    pub cycle_1: Option<String>,
    pub cycle_2: Option<String>,
    pub cycle_3: Option<String>,
    pub cycle_4: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // name/ibp are part of the schema; not all are rendered
pub struct Provider {
    pub name: String,
    #[serde(default)]
    pub display: String,
    #[serde(default)]
    pub matrix: String,
    #[serde(default)]
    pub ibp: String,
    #[serde(default, rename = "validator")]
    pub validators: Vec<Val>,
}

#[derive(Debug, Deserialize)]
pub struct Val {
    pub name: String,
    pub stash: String,
    /// "1".."4" for a downsize cycle, "floor" to survive to shutdown.
    pub cycle: String,
}

impl Providers {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading providers file {}", path.display()))?;
        toml::from_str(&raw)
            .with_context(|| format!("parsing providers file {}", path.display()))
    }

    fn cycle_dt(&self, cycle: u32) -> &str {
        match cycle {
            1 => self.meta.cycle_1.as_deref(),
            2 => self.meta.cycle_2.as_deref(),
            3 => self.meta.cycle_3.as_deref(),
            4 => self.meta.cycle_4.as_deref(),
            _ => None,
        }
        .unwrap_or("")
    }

    /// Render the (plain, html) Matrix notice for a cycle: tags every provider
    /// (with a Matrix handle) whose validators leave this cycle, listing each
    /// validator's name + stash. `from`/`to` are the validator-set counts.
    /// Returns None if no tagged provider leaves this cycle.
    pub fn cycle_notice(&self, cycle: u32, from: u32, to: u32) -> Option<(String, String)> {
        let key = cycle.to_string();
        let rows: Vec<(&Provider, Vec<&Val>)> = self
            .providers
            .iter()
            .filter(|p| !p.matrix.is_empty())
            .filter_map(|p| {
                let vs: Vec<&Val> = p.validators.iter().filter(|v| v.cycle == key).collect();
                (!vs.is_empty()).then_some((p, vs))
            })
            .collect();
        if rows.is_empty() {
            return None;
        }
        let dt = self.cycle_dt(cycle);
        let mut text = format!(
            "🔻 Paseo relay downsizing — Cycle {cycle} of 4 · {dt} · validator set {from} → {to}.\n\
             These validators are being rotated out of the active set this cycle \
             (force-unstaked, no slashing):\n\n"
        );
        let mut html = format!(
            "<b>🔻 Paseo relay downsizing — Cycle {cycle} of 4</b> · {dt} · validator set \
             {from} → {to}.<br/>These validators are being rotated out this cycle \
             (force-unstaked, no slashing):<ul>"
        );
        for (p, vs) in &rows {
            text.push_str(&format!("• {} — {} ({}):\n", p.matrix, p.display, vs.len()));
            html.push_str(&format!(
                "<li><a href=\"https://matrix.to/#/{m}\">{m}</a> — {d} ({n}):<ul>",
                m = p.matrix,
                d = p.display,
                n = vs.len()
            ));
            for v in vs {
                text.push_str(&format!("    {}  {}\n", v.name, v.stash));
                html.push_str(&format!("<li>{} — <code>{}</code></li>", v.name, v.stash));
            }
            html.push_str("</ul></li>");
        }
        html.push_str("</ul>");
        text.push_str(
            "\nNo slashing — validators are cleanly removed. Chains stay live at a reduced \
             cadence. Live status: https://paseo-downsizer.r0gue.io",
        );
        html.push_str(
            "No slashing — validators are cleanly removed. Chains stay live at a reduced \
             cadence. <a href=\"https://paseo-downsizer.r0gue.io\">Live status</a>.",
        );
        Some((text, html))
    }
}
