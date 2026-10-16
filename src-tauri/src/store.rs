//! Projects live as .cutlight files and are autosaved after every change.
//! `recent.json` (app data) remembers them for the start screen, with a preview thumbnail each.

use crate::core::Shared;
use crate::model::Project;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

pub fn app_dir(core: &crate::core::Core) -> PathBuf {
    core.cache_root.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| core.cache_root.clone())
}

pub fn projects_dir(core: &Shared) -> PathBuf {
    let d = app_dir(core).join("projects");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn new_project_path(core: &Shared) -> PathBuf {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    projects_dir(core).join(format!("project-{ms}.cutlight"))
}

fn recent_file(core: &Shared) -> PathBuf {
    app_dir(core).join("recent.json")
}

fn read_raw(core: &Shared) -> Vec<Value> {
    std::fs::read_to_string(recent_file(core)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn write_raw(core: &Shared, list: &[Value]) {
    let _ = std::fs::create_dir_all(app_dir(core));
    let _ = std::fs::write(recent_file(core), serde_json::to_string_pretty(list).unwrap_or_default());
}

/// Recent projects for the start screen (existing files only, newest first).
pub fn list(core: &Shared) -> Vec<Value> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut v: Vec<Value> = read_raw(core)
        .into_iter()
        .filter(|e| e["path"].as_str().map(|p| std::path::Path::new(p).exists()).unwrap_or(false))
        .map(|mut e| {
            if let Some(t) = e["thumb"].as_str() {
                if !std::path::Path::new(t).exists() {
                    e["thumb"] = Value::Null;
                }
            }
            e
        })
        .collect();
    v.sort_by_key(|e| std::cmp::Reverse(e["updated"].as_u64().unwrap_or(0)));
    v
}

pub fn remove(core: &Shared, path: &str, delete_file: bool) {
    {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut l = read_raw(core);
        l.retain(|e| e["path"].as_str() != Some(path));
        write_raw(core, &l);
    }
    // only ever delete files Cutlight created itself
    if delete_file && std::path::Path::new(path).starts_with(projects_dir(core)) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn autosave(core: &Shared) {
    let Some(path) = core.path.lock().unwrap().clone() else { return };
    let txt = match serde_json::to_string_pretty(&*core.project.lock().unwrap()) {
        Ok(t) => t,
        Err(_) => return,
    };
    if std::fs::write(&path, txt).is_ok() {
        core.dirty.store(false, std::sync::atomic::Ordering::Relaxed);
        touch(core);
    }
}

/// First picture of the project: earliest visual clip's source frame.
fn thumb_source(p: &Project) -> Option<(String, String, f64)> {
    let mut best: Option<(f64, String, f64)> = None;
    for t in p.tracks.iter().filter(|t| t.kind == "video") {
        for c in t.clips.iter().filter(|c| c.kind == "video" || c.kind == "image") {
            if let Some(m) = c.media_id.as_deref().and_then(|i| p.media(i)) {
                if best.as_ref().map(|b| c.start < b.0).unwrap_or(true) {
                    best = Some((c.start, m.path.clone(), c.in_point));
                }
            }
        }
    }
    best.map(|(_, path, t)| (format!("{path}@{t:.2}"), path, t))
}

/// Registers/refreshes this project in the recent list and (re)builds its preview if needed.
pub fn touch(core: &Shared) {
    let Some(path) = core.path.lock().unwrap().clone() else { return };
    let p = core.project.lock().unwrap().clone();
    let path_s = path.to_string_lossy().to_string();
    let src = thumb_source(&p);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let need_thumb;
    {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut list = read_raw(core);
        let old = list.iter().position(|e| e["path"].as_str() == Some(&path_s)).map(|i| list.remove(i));
        let old_key = old.as_ref().and_then(|o| o["thumbKey"].as_str().map(String::from)).unwrap_or_default();
        let key = src.as_ref().map(|s| s.0.clone()).unwrap_or_default();
        need_thumb = !key.is_empty() && key != old_key;
        let keep = !need_thumb;
        list.insert(
            0,
            json!({
                "path": path_s, "name": p.name, "updated": now, "duration": p.duration(),
                "clips": p.tracks.iter().map(|t| t.clips.len()).sum::<usize>(),
                "width": p.settings.width, "height": p.settings.height,
                "thumb": if keep { old.as_ref().map(|o| o["thumb"].clone()).unwrap_or(Value::Null) } else { Value::Null },
                "thumbKey": if keep { Value::String(old_key) } else { Value::Null },
            }),
        );
        list.truncate(80);
        write_raw(core, &list);
    }
    if need_thumb {
        if let Some((key, media_path, t)) = src {
            let core = core.clone();
            std::thread::spawn(move || {
                let dir = app_dir(&core).join("thumbs");
                let _ = std::fs::create_dir_all(&dir);
                let mut h = DefaultHasher::new();
                (&path_s, &key).hash(&mut h);
                let out = dir.join(format!("{:016x}.jpg", h.finish()));
                let mut c = core.ff.cmd();
                c.args(["-ss", &format!("{t:.3}"), "-i", &media_path, "-frames:v", "1", "-vf", "scale=480:-2", "-q:v", "4"]).arg(&out);
                if crate::media::run(c).is_err() {
                    return;
                }
                let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
                let mut list = read_raw(&core);
                if let Some(e) = list.iter_mut().find(|e| e["path"].as_str() == Some(&path_s)) {
                    e["thumb"] = json!(out.to_string_lossy());
                    e["thumbKey"] = json!(key);
                }
                write_raw(&core, &list);
                drop(_g);
                core.emit_state();
            });
        }
    }
}

/// Preview for projects whose first picture is an HTML scene: the UI renders a frame and sends the PNG.
pub fn set_thumb(core: &Shared, project_path: &str, png: &[u8]) -> Result<(), String> {
    let dir = app_dir(core).join("thumbs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut h = DefaultHasher::new();
    ("html-thumb", project_path).hash(&mut h);
    let file = dir.join(format!("{:016x}.png", h.finish()));
    std::fs::write(&file, png).map_err(|e| e.to_string())?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = read_raw(core);
    if let Some(e) = list.iter_mut().find(|e| e["path"].as_str() == Some(project_path)) {
        e["thumb"] = json!(file.to_string_lossy());
        e["thumbKey"] = json!("html");
    }
    write_raw(core, &list);
    Ok(())
}
