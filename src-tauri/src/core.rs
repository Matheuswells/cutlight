use crate::media::Ffmpeg;
use crate::model::Project;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ChildStdin;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Default)]
pub struct RenderStatus {
    /// idle | running | done | error | cancelled
    pub state: String,
    pub progress: f64,
    pub message: String,
    pub output: String,
}

pub struct Core {
    pub app: AppHandle,
    pub project: Mutex<Project>,
    pub undo: Mutex<Vec<String>>,
    pub redo: Mutex<Vec<String>>,
    pub path: Mutex<Option<PathBuf>>,
    pub dirty: AtomicBool,
    /// false = start screen (no project open)
    pub open: AtomicBool,
    pub ff: Ffmpeg,
    pub cache_root: PathBuf,
    pub render: Mutex<RenderStatus>,
    pub cancel: AtomicBool,
    pub replies: Mutex<HashMap<String, Sender<Value>>>,
    pub html_pipes: Mutex<HashMap<String, ChildStdin>>,
    pub html_done: Mutex<HashMap<String, Sender<Result<(), String>>>>,
    pub mcp_port: AtomicU16,
    pub seq: std::sync::atomic::AtomicU64,
}

pub type Shared = Arc<Core>;

impl Core {
    pub fn snapshot(&self) -> Value {
        let p = self.project.lock().unwrap();
        let r = self.render.lock().unwrap().clone();
        json!({
            "project": &*p,
            "path": self.path.lock().unwrap().as_ref().map(|p| p.to_string_lossy().to_string()),
            "dirty": self.dirty.load(Ordering::Relaxed),
            "open": self.open.load(Ordering::Relaxed),
            "canUndo": !self.undo.lock().unwrap().is_empty(),
            "canRedo": !self.redo.lock().unwrap().is_empty(),
            "duration": p.duration(),
            "render": { "state": r.state, "progress": r.progress, "message": r.message, "output": r.output },
            "mcpPort": self.mcp_port.load(Ordering::Relaxed),
            "exe": crate::connect::exe_path(),
        })
    }

    pub fn emit_state(&self) {
        let _ = self.app.emit("state", self.snapshot());
    }

    pub fn push_undo(&self, p: &Project) {
        let mut u = self.undo.lock().unwrap();
        u.push(serde_json::to_string(p).unwrap_or_default());
        if u.len() > 150 {
            u.remove(0);
        }
        self.redo.lock().unwrap().clear();
    }

    pub fn toast(&self, msg: &str) {
        let _ = self.app.emit("toast", msg);
    }

    /// Round-trip a call to the UI (used by MCP for screenshots/inspection/control).
    pub fn ui_call(&self, js: &str, timeout_ms: u64) -> Result<Value, String> {
        let id = format!("r{}", self.seq.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = std::sync::mpsc::channel();
        self.replies.lock().unwrap().insert(id.clone(), tx);
        let _ = self.app.emit("ui-eval", json!({ "id": id, "js": js }));
        let res = rx.recv_timeout(std::time::Duration::from_millis(timeout_ms));
        self.replies.lock().unwrap().remove(&id);
        match res {
            Ok(v) => {
                if let Some(e) = v.get("error") {
                    Err(e.as_str().unwrap_or("ui error").to_string())
                } else {
                    Ok(v.get("value").cloned().unwrap_or(Value::Null))
                }
            }
            Err(_) => Err("UI did not respond (is the window open?)".into()),
        }
    }
}
