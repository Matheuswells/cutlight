//! Offline speech-to-text with whisper.cpp (via whisper-rs).
//! Model lookup: <exe dir>/models, <resource dir>/models, <app data>/models — first `ggml-*.bin` found,
//! preferring base > small > tiny > medium > large.

use crate::core::Shared;
use crate::media::quiet;
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use tauri::Manager;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

static CTX: OnceLock<Mutex<Option<(PathBuf, WhisperContext)>>> = OnceLock::new();

fn model_dirs(core: &Shared) -> Vec<PathBuf> {
    let mut v = vec![];
    if let Ok(me) = std::env::current_exe() {
        if let Some(d) = me.parent() {
            v.push(d.join("models"));
        }
    }
    if let Ok(d) = core.app.path().resource_dir() {
        v.push(d.join("models"));
    }
    if let Some(d) = core.cache_root.parent() {
        v.push(d.join("models"));
    }
    // dev: src-tauri/models next to the manifest
    v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models"));
    v
}

fn find_model(core: &Shared) -> Result<PathBuf, String> {
    let rank = |n: &str| ["base", "small", "tiny", "medium", "large"].iter().position(|k| n.contains(k)).unwrap_or(9);
    let mut found: Vec<PathBuf> = vec![];
    let dirs = model_dirs(core);
    for d in &dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.starts_with("ggml-") && n.ends_with(".bin") {
                    found.push(e.path());
                }
            }
        }
    }
    found.sort_by_key(|p| rank(&p.file_name().unwrap().to_string_lossy()));
    found.into_iter().next().ok_or_else(|| {
        format!(
            "No Whisper model found. Put a ggml model (e.g. ggml-base.bin) in {}",
            dirs.last().map(|d| d.display().to_string()).unwrap_or_default()
        )
    })
}

fn decode_16k(core: &Shared, path: &str) -> Result<Vec<f32>, String> {
    let mut c = std::process::Command::new(&core.ff.ffmpeg);
    quiet(&mut c).args(["-hide_banner", "-loglevel", "error", "-i", path, "-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "-"]);
    c.stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null());
    let mut child = c.spawn().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let _ = child.wait();
    Ok(bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

pub fn transcribe(core: &Shared, a: &Value) -> Result<Value, String> {
    let mid = a["mediaId"].as_str().ok_or("mediaId required")?;
    let path = {
        let p = core.project.lock().unwrap();
        let m = p.media(mid).ok_or("media not found")?;
        if !m.has_audio {
            return Err("this media has no audio".into());
        }
        m.path.clone()
    };
    let lang = a["language"].as_str().filter(|l| !l.is_empty() && *l != "auto").map(String::from);
    let model = find_model(core)?;
    core.toast("Transcribing with Whisper…");
    let samples = decode_16k(core, &path)?;
    if samples.is_empty() {
        return Err("no audio could be decoded".into());
    }

    let cell = CTX.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().unwrap();
    if guard.as_ref().map(|(p, _)| p != &model).unwrap_or(true) {
        let ctx = WhisperContext::new_with_params(model.to_str().ok_or("bad model path")?, WhisperContextParameters::default())
            .map_err(|e| format!("could not load model: {e}"))?;
        *guard = Some((model.clone(), ctx));
    }
    let ctx = &guard.as_ref().unwrap().1;
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(lang.as_deref());
    params.set_translate(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    params.set_token_timestamps(true);
    params.set_split_on_word(true);
    params.set_max_len(42); // caption-sized segments
    params.set_n_threads(std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4).min(8));
    state.full(params, &samples).map_err(|e| format!("whisper failed: {e}"))?;

    let mut segs = vec![];
    for seg in state.as_iter() {
        let text = seg.to_str_lossy().map_err(|e| e.to_string())?.trim().to_string();
        if text.is_empty() {
            continue;
        }
        // timestamps are in centiseconds
        segs.push(json!({
            "start": seg.start_timestamp() as f64 / 100.0,
            "end": seg.end_timestamp() as f64 / 100.0,
            "text": text,
        }));
    }
    drop(guard);
    let n = segs.len();
    crate::ops::run(core, "set_transcript", &json!({ "segments": segs }))?;
    Ok(json!({ "count": n, "model": model.file_name().map(|f| f.to_string_lossy().to_string()) }))
}
