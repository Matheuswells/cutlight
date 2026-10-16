use crate::core::Shared;
use crate::model::Project;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

fn app_dir(core: &Shared) -> PathBuf {
    core.cache_root.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| core.cache_root.clone())
}

pub fn autosave(core: &Shared) {
    let txt = match serde_json::to_string_pretty(&*core.project.lock().unwrap()) {
        Ok(t) => t,
        Err(_) => return,
    };
    let named = core.path.lock().unwrap().clone();
    let target = named.clone().unwrap_or_else(|| app_dir(core).join("autosave.cutlight"));
    if std::fs::write(&target, txt).is_ok() && named.is_some() {
        core.dirty.store(false, Ordering::Relaxed);
    }
}

pub fn remember_last(core: &Shared) {
    let dir = app_dir(core);
    let _ = std::fs::create_dir_all(&dir);
    match core.path.lock().unwrap().as_ref() {
        Some(p) => {
            let _ = std::fs::write(dir.join("last.txt"), p.to_string_lossy().as_bytes());
        }
        None => {
            let _ = std::fs::remove_file(dir.join("last.txt"));
        }
    }
}

/// Reopen the last project (or the unsaved autosave) at startup.
pub fn restore(core: &Shared) {
    let dir = app_dir(core);
    let last = std::fs::read_to_string(dir.join("last.txt")).ok().map(|s| PathBuf::from(s.trim()));
    let (path, named) = match last {
        Some(p) if p.exists() => (p, true),
        _ => (dir.join("autosave.cutlight"), false),
    };
    if let Ok(txt) = std::fs::read_to_string(&path) {
        if let Ok(p) = serde_json::from_str::<Project>(&txt) {
            *core.project.lock().unwrap() = p;
            if named {
                *core.path.lock().unwrap() = Some(path);
            }
            crate::ops::resume_media_jobs(core);
        }
    }
}
