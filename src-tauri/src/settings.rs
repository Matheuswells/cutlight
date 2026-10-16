//! App-wide settings: the Pixabay key (entered in the app, never in chat) and the editing rules
//! Claude learns from your corrections ("make it a skill": every fix becomes a rule with its reason).

use crate::core::Core;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Rule {
    pub rule: String,
    pub why: String,
    pub added: u64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub pixabay_key: String,
    pub rules: Vec<Rule>,
    /// The user connected Claude once: re-register automatically if the Claude app rewrites its config
    pub auto_link: bool,
    /// The user chose "Not now": never pop the connect dialog on its own again
    pub connect_dismissed: bool,
}

fn path(core: &Core) -> std::path::PathBuf {
    crate::store::app_dir(core).join("settings.json")
}

pub fn load(core: &Core) -> AppSettings {
    std::fs::read_to_string(path(core)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(core: &Core, s: &AppSettings) {
    let _ = std::fs::create_dir_all(crate::store::app_dir(core));
    let _ = std::fs::write(path(core), serde_json::to_string_pretty(s).unwrap_or_default());
}

/// What the UI may see: never the key itself.
pub fn public(core: &Core) -> Value {
    let s = load(core);
    json!({ "pixabayConfigured": !s.pixabay_key.trim().is_empty(), "rules": s.rules, "autoLink": s.auto_link, "connectDismissed": s.connect_dismissed })
}

pub fn set_pixabay_key(core: &Core, key: &str) {
    let mut s = load(core);
    s.pixabay_key = key.trim().to_string();
    save(core, &s);
}

pub fn add_rule(core: &Core, rule: &str, why: &str) -> Result<Value, String> {
    let rule = rule.trim();
    if rule.is_empty() {
        return Err("rule is empty".into());
    }
    let mut s = load(core);
    if s.rules.iter().any(|r| r.rule.eq_ignore_ascii_case(rule)) {
        return Ok(json!({"ok": true, "duplicate": true}));
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    s.rules.push(Rule { rule: rule.into(), why: why.trim().into(), added: now });
    save(core, &s);
    core.emit_state();
    Ok(json!({"ok": true, "count": s.rules.len()}))
}

pub fn remove_rule(core: &Core, index: usize) {
    let mut s = load(core);
    if index < s.rules.len() {
        s.rules.remove(index);
        save(core, &s);
    }
}

pub fn set_flag(core: &Core, name: &str, value: bool) {
    let mut s = load(core);
    match name {
        "auto_link" => s.auto_link = value,
        "connect_dismissed" => s.connect_dismissed = value,
        _ => return,
    }
    save(core, &s);
}
