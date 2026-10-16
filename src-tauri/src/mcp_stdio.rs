//! `cutlight --mcp-stdio`: a stdio MCP server for the Claude desktop app.
//! It relays every JSON-RPC line to the editor's local HTTP MCP server and launches the editor if it is not running.

use std::io::{BufRead, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

fn post(port: u16, body: &str) -> Result<(u16, String), String> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(400)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(300))).ok();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").ok_or("bad response")?;
    let status = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    Ok((status, body.trim().to_string()))
}

fn find_port() -> Option<u16> {
    (47321..47341u16).find(|p| {
        matches!(post(*p, r#"{"jsonrpc":"2.0","id":0,"method":"ping"}"#), Ok((200, b)) if b.contains("result"))
    })
}

fn ensure_editor() -> Result<u16, String> {
    if let Some(p) = find_port() {
        return Ok(p);
    }
    // start the editor (same executable, no args) and wait for its MCP server
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let _ = std::process::Command::new(exe).spawn();
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(45) {
        std::thread::sleep(Duration::from_millis(500));
        if let Some(p) = find_port() {
            return Ok(p);
        }
    }
    Err("Cutlight did not start".into())
}

pub fn run() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut port: Option<u16> = None;
    for line in stdin.lock().lines().map_while(Result::ok) {
        let line = line.trim_start_matches('\u{feff}').to_string();
        if line.trim().is_empty() {
            continue;
        }
        let id = serde_json::from_str::<serde_json::Value>(&line).ok().and_then(|v| v.get("id").cloned());
        let mut result = port.ok_or(()).and_then(|p| post(p, &line).map_err(|_| ()));
        if result.is_err() {
            result = ensure_editor().and_then(|p| {
                port = Some(p);
                post(p, &line)
            }).map_err(|_| ());
        }
        match result {
            Ok((_, body)) if !body.is_empty() => {
                let _ = writeln!(out, "{body}");
                let _ = out.flush();
            }
            Ok(_) => {} // notification: no reply
            Err(_) => {
                if let Some(id) = id {
                    let e = serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"Could not reach the Cutlight editor. Open Cutlight and try again."}});
                    let _ = writeln!(out, "{e}");
                    let _ = out.flush();
                }
            }
        }
    }
}
