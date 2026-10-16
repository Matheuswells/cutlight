mod assets;
mod connect;
mod core;
mod guide;
mod settings;
mod mcp;
mod mcp_stdio;
mod media;
mod model;
mod ops;
mod render;
mod shortcuts;
mod store;
mod templates;
mod whisper;

use core::{Core, RenderStatus, Shared};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64};
use std::sync::Mutex;
use tauri::{ipc::Request, Manager, State};

#[tauri::command]
async fn op(core: State<'_, Shared>, name: String, args: Option<Value>) -> Result<Value, String> {
    let core = core.inner().clone();
    let args = args.unwrap_or(Value::Null);
    tauri::async_runtime::spawn_blocking(move || {
        if name == "transcribe" {
            whisper::transcribe(&core, &args)
        } else {
            ops::run(&core, &name, &args)
        }
    })
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_state(core: State<'_, Shared>) -> Value {
    core.snapshot()
}

#[tauri::command]
fn ui_reply(core: State<'_, Shared>, id: String, value: Option<Value>, error: Option<String>) {
    if let Some(tx) = core.replies.lock().unwrap().get(&id) {
        let _ = tx.send(match error {
            Some(e) => serde_json::json!({ "error": e }),
            None => serde_json::json!({ "value": value.unwrap_or(Value::Null) }),
        });
    }
}

#[tauri::command]
fn html_frame(core: State<'_, Shared>, request: Request<'_>) -> Result<(), String> {
    let key = request
        .headers()
        .get("x-key")
        .and_then(|v| v.to_str().ok())
        .ok_or("missing key")?
        .to_string();
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw bytes".into()) };
    let mut pipes = core.html_pipes.lock().unwrap();
    let w = pipes.get_mut(&key).ok_or("render was cancelled")?;
    w.write_all(bytes).map_err(|e| e.to_string())
}

#[tauri::command]
fn html_finish(core: State<'_, Shared>, key: String, error: Option<String>) {
    if let Some(tx) = core.html_done.lock().unwrap().get(&key) {
        let _ = tx.send(match error {
            Some(e) => Err(e),
            None => Ok(()),
        });
    }
}

/// Registers Cutlight in the Claude desktop app's config. The Claude app rewrites that file when it quits, so we also
/// remember the choice (auto_link) and re-register in the background once the app has closed.
#[tauri::command]
async fn connect_claude(core: State<'_, Shared>) -> Result<Value, String> {
    let c = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut r = connect::run()?;
        settings::set_flag(&c, "auto_link", true);
        settings::set_flag(&c, "connect_dismissed", true);
        r["claudeRunning"] = Value::Bool(connect::claude_running());
        c.emit_state();
        Ok(r)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn settings_dismiss_connect(core: State<'_, Shared>) {
    settings::set_flag(&core, "connect_dismissed", true);
    core.emit_state();
}

/// The Pixabay key is typed in the app (never in chat) and never sent back to the UI.
#[tauri::command]
fn settings_set_pixabay(core: State<'_, Shared>, key: String) {
    settings::set_pixabay_key(&core, &key);
    core.emit_state();
}

#[tauri::command]
fn set_thumb(core: State<'_, Shared>, request: Request<'_>) -> Result<(), String> {
    let enc = request.headers().get("x-path").and_then(|v| v.to_str().ok()).ok_or("missing path")?;
    let path = percent_decode(enc);
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw bytes".into()) };
    store::set_thumb(core.inner(), &path, bytes)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i) = (Vec::with_capacity(b.len()), 0);
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[tauri::command]
fn rule_remove(core: State<'_, Shared>, index: usize) {
    settings::remove_rule(&core, index);
    core.emit_state();
}

#[tauri::command]
async fn claude_status() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(connect::linked).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn render_start(core: State<'_, Shared>, path: String) -> Result<Value, String> {
    render::start(core.inner(), path)
}

#[tauri::command]
fn render_cancel(core: State<'_, Shared>) {
    render::cancel(core.inner());
}

pub fn run() {
    // Claude desktop launches us as `cutlight --mcp-stdio`: act as a stdio MCP relay, no window
    if std::env::args().any(|a| a == "--mcp-stdio") {
        mcp_stdio::run();
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data = app.path().app_data_dir().expect("app data dir");
            let cache_root = data.join("cache");
            std::fs::create_dir_all(&cache_root).ok();
            let core: Shared = std::sync::Arc::new(Core {
                app: app.handle().clone(),
                project: Mutex::new(model::Project::default()),
                undo: Mutex::new(vec![]),
                redo: Mutex::new(vec![]),
                path: Mutex::new(None),
                dirty: AtomicBool::new(false),
                open: AtomicBool::new(false), // always start on the start screen
                ff: media::Ffmpeg::locate(),
                cache_root,
                render: Mutex::new(RenderStatus { state: "idle".into(), ..Default::default() }),
                cancel: AtomicBool::new(false),
                replies: Mutex::new(HashMap::new()),
                html_pipes: Mutex::new(HashMap::new()),
                html_done: Mutex::new(HashMap::new()),
                mcp_port: AtomicU16::new(0),
                seq: AtomicU64::new(1),
            });
            mcp::start(core.clone());
            shortcuts::first_run(&core);
            assets::install_libs(&core);
            // keep the Claude connection alive: the Claude app drops entries it did not write when it quits
            {
                let c = core.clone();
                std::thread::spawn(move || {
                    let mut calm = 0;
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        if !settings::load(&c).auto_link || connect::linked() {
                            calm = 0;
                            continue;
                        }
                        if connect::claude_running() {
                            calm = 0;
                            continue;
                        }
                        calm += 1;
                        if calm >= 2 {
                            if connect::run().is_ok() {
                                c.emit_state();
                            }
                            calm = 0;
                        }
                    }
                });
            }
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![op, get_state, ui_reply, html_frame, html_finish, render_start, render_cancel, connect_claude, claude_status, settings_set_pixabay, rule_remove, set_thumb, settings_dismiss_connect])
        .run(tauri::generate_context!())
        .expect("error while running Cutlight");
}


