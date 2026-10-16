//! One-click "connect Claude": registers Cutlight in the Claude desktop app's MCP config.
//! Claude Desktop launches local servers from `claude_desktop_config.json`, so Cutlight registers
//! itself as `cutlight.exe --mcp-stdio` (a tiny relay to the running editor, see mcp_stdio.rs).

use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

fn home() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default())
}

/// Every place the Claude desktop app may read its config from on this machine.
fn config_paths() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = vec![];
    if cfg!(windows) {
        if let Ok(ad) = std::env::var("APPDATA") {
            v.push(PathBuf::from(ad).join("Claude/claude_desktop_config.json"));
        }
        // Microsoft Store build: the app sees a virtualised copy that shadows the real one
        if let Ok(la) = std::env::var("LOCALAPPDATA") {
            if let Ok(rd) = std::fs::read_dir(PathBuf::from(la).join("Packages")) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().starts_with("Claude_") {
                        v.push(e.path().join("LocalCache/Roaming/Claude/claude_desktop_config.json"));
                    }
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        v.push(home().join("Library/Application Support/Claude/claude_desktop_config.json"));
    } else {
        v.push(home().join(".config/Claude/claude_desktop_config.json"));
    }
    v
}

pub fn exe_path() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

fn entry() -> Value {
    json!({ "command": exe_path(), "args": ["--mcp-stdio"] })
}

/// Is Cutlight registered in Claude Desktop with *this* executable?
pub fn linked() -> bool {
    let me = exe_path();
    config_paths().iter().any(|p| {
        std::fs::read_to_string(p)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .map(|v| v["mcpServers"]["cutlight"]["command"].as_str() == Some(me.as_str()))
            .unwrap_or(false)
    })
}

pub fn run() -> Result<Value, String> {
    let paths = config_paths();
    let existing: Vec<&PathBuf> = paths.iter().filter(|p| p.exists()).collect();
    let targets: Vec<PathBuf> = if existing.is_empty() {
        // Claude has not been run yet (or is not installed): create the standard location
        let first = paths.first().cloned().ok_or("could not locate the Claude app config folder")?;
        if let Some(d) = first.parent() {
            if !d.exists() && !d.parent().map(|p| p.exists()).unwrap_or(false) {
                return Err("The Claude app doesn't seem to be installed. Install it from https://claude.com/download, then try again.".into());
            }
            let _ = std::fs::create_dir_all(d);
        }
        vec![first]
    } else {
        existing.into_iter().cloned().collect()
    };

    let mut written = vec![];
    for p in &targets {
        let mut v: Value = match std::fs::read_to_string(p) {
            Ok(t) if !t.trim().is_empty() => serde_json::from_str(&t).map_err(|e| format!("{} is not valid JSON: {e}", p.display()))?,
            _ => json!({}),
        };
        let bak = p.with_extension("json.cutlight.bak");
        if p.exists() && !bak.exists() {
            let _ = std::fs::copy(p, &bak); // one-time backup of the user's original config
        }
        let obj = v.as_object_mut().ok_or("config is not a JSON object")?;
        let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
        servers.as_object_mut().ok_or("mcpServers is not an object")?.insert("cutlight".into(), entry());
        std::fs::write(p, serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| format!("could not write {}: {e}", p.display()))?;
        written.push(p.to_string_lossy().to_string());
    }
    Ok(json!({ "ok": true, "files": written }))
}

/// Is the Claude *desktop app* running? (The Claude Code CLI also has a claude.exe: it must not count.)
pub fn claude_running() -> bool {
    #[cfg(windows)]
    {
        let mut c = Command::new("powershell");
        crate::media::quiet(&mut c).args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-Process -Name Claude -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*WindowsApps*Claude*' -or $_.Path -like '*AnthropicClaude*' } | Measure-Object).Count",
        ]);
        c.output().map(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u32>().unwrap_or(0) > 0).unwrap_or(false)
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("pgrep").args(["-f", "Claude.app/Contents/MacOS/Claude"]).output().map(|o| o.status.success()).unwrap_or(false)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        false
    }
}
