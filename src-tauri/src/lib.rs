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

/// Registers Cutlight in the Claude desktop app's config.
#[tauri::command]
async fn connect_claude() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(connect::run).await.map_err(|e| e.to_string())?
}

/// The Pixabay key is typed in the app (never in chat) and never sent back to the UI.
#[tauri::command]
fn settings_set_pixabay(core: State<'_, Shared>, key: String) {
    settings::set_pixabay_key(&core, &key);
    core.emit_state();
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
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![op, get_state, ui_reply, html_frame, html_finish, render_start, render_cancel, connect_claude, claude_status, settings_set_pixabay, rule_remove])
        .run(tauri::generate_context!())
        .expect("error while running Cutlight");
}
