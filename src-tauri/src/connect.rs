//! One-click "connect Claude": registers Cutlight's local MCP server with the Claude Code CLI.

use crate::media::quiet;
use serde_json::{json, Value};
use std::process::Command;

fn sh(line: &str) -> std::io::Result<std::process::Output> {
    #[cfg(windows)]
    {
        let mut c = Command::new("cmd");
        quiet(&mut c).args(["/C", line]);
        c.output()
    }
    #[cfg(not(windows))]
    {
        // GUI apps don't inherit the shell PATH on macOS: use a login shell
        Command::new("/bin/zsh").args(["-lc", line]).output()
    }
}

pub fn run(url: &str) -> Result<Value, String> {
    let ver = sh("claude --version").map_err(|e| e.to_string())?;
    if !ver.status.success() {
        return Err("Claude Code CLI not found. Install it from https://claude.com/claude-code, or copy the config from the Claude tab.".into());
    }
    let _ = sh("claude mcp remove --scope user cutlight"); // replace an older registration (port may have changed)
    let out = sh(&format!("claude mcp add --scope user --transport http cutlight {url}")).map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    if out.status.success() {
        Ok(json!({ "ok": true, "output": text, "url": url }))
    } else {
        Err(if text.is_empty() { "claude mcp add failed".into() } else { text })
    }
}
