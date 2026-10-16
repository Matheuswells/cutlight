//! Every editing action lives here. The UI and the MCP server both call `run`,
//! so Claude can do exactly what the user can.

use crate::core::Shared;
use crate::media;
use crate::model::*;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

type R = Result<Value, String>;
const EPS: f64 = 1e-4;

fn s<'a>(a: &'a Value, k: &str) -> Result<&'a str, String> {
    a.get(k).and_then(|v| v.as_str()).ok_or_else(|| format!("missing string argument '{k}'"))
}
fn os<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}
fn f(a: &Value, k: &str) -> Result<f64, String> {
    a.get(k).and_then(|v| v.as_f64()).ok_or_else(|| format!("missing number argument '{k}'"))
}
fn of(a: &Value, k: &str) -> Option<f64> {
    a.get(k).and_then(|v| v.as_f64())
}
fn ob(a: &Value, k: &str) -> Option<bool> {
    a.get(k).and_then(|v| v.as_bool())
}

fn find_clip(p: &Project, id: &str) -> Result<(usize, usize), String> {
    for (ti, t) in p.tracks.iter().enumerate() {
        if let Some(ci) = t.clips.iter().position(|c| c.id == id) {
            return Ok((ti, ci));
        }
    }
    Err(format!("clip '{id}' not found"))
}

fn find_track(p: &Project, id: &str) -> Result<usize, String> {
    p.tracks.iter().position(|t| t.id == id).ok_or_else(|| format!("track '{id}' not found"))
}

fn track_end(t: &Track) -> f64 {
    t.clips.iter().map(|c| c.end()).fold(0.0, f64::max)
}

/// Keep a track sorted and free of accidental overlaps (only transitions may overlap).
pub fn fix_track(t: &mut Track) {
    t.clips.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    for i in 0..t.clips.len() {
        if t.clips[i].start < 0.0 {
            t.clips[i].start = 0.0;
        }
        if i == 0 {
            continue;
        }
        let (pe, pd) = (t.clips[i - 1].end(), t.clips[i - 1].duration);
        let c = &mut t.clips[i];
        let allowed = c.transition.as_ref().map(|tr| tr.duration.min(pd).min(c.duration).max(0.0)).unwrap_or(0.0);
        if c.start < pe - allowed - EPS {
            c.start = pe - allowed;
        }
    }
}

fn fix_all(p: &mut Project) {
    for t in p.tracks.iter_mut() {
        fix_track(t);
    }
}

fn shift_after(t: &mut Track, from: f64, delta: f64) {
    for c in t.clips.iter_mut() {
        if c.start >= from - EPS {
            c.start = (c.start + delta).max(0.0);
        }
    }
}

fn split_at(p: &mut Project, ti: usize, ci: usize, at: f64) -> Option<String> {
    let c = p.tracks[ti].clips[ci].clone();
    if at <= c.start + 0.02 || at >= c.end() - 0.02 {
        return None;
    }
    let nid = p.new_id("c");
    let mut b = c.clone();
    b.id = nid.clone();
    b.start = at;
    b.in_point = c.in_point + (at - c.start) * c.speed;
    b.duration = c.end() - at;
    b.transition = None;
    b.fade_in = 0.0;
    let a = &mut p.tracks[ti].clips[ci];
    a.duration = at - c.start;
    a.fade_out = 0.0;
    p.tracks[ti].clips.push(b);
    fix_track(&mut p.tracks[ti]);
    Some(nid)
}

fn free_track(p: &mut Project, kind: &str, start: f64, end: f64) -> usize {
    let free = |t: &Track| {
        t.kind == kind && !t.locked && t.clips.iter().all(|c| c.end() <= start + EPS || c.start >= end - EPS)
    };
    if let Some(i) = p.tracks.iter().position(free) {
        return i;
    }
    let n = p.tracks.iter().filter(|t| t.kind == kind).count() + 1;
    let id = p.new_id("t");
    let name = format!("{}{}", if kind == "audio" { "A" } else { "V" }, n);
    let t = Track { id, kind: kind.into(), name, ..Default::default() };
    if kind == "audio" {
        p.tracks.push(t);
        p.tracks.len() - 1
    } else {
        let at = p.tracks.iter().position(|t| t.kind == "video").unwrap_or(0);
        p.tracks.insert(at, t);
        at
    }
}

fn source_limit(p: &Project, c: &Clip) -> Option<f64> {
    if c.kind == "video" || c.kind == "audio" {
        let m = p.media(c.media_id.as_deref()?)?;
        if m.duration > 0.0 {
            return Some(((m.duration - c.in_point) / c.speed.max(0.01)).max(0.05));
        }
    }
    None
}

fn clamp_clip(p: &mut Project, ti: usize, ci: usize) {
    let lim = source_limit(p, &p.tracks[ti].clips[ci]);
    let c = &mut p.tracks[ti].clips[ci];
    c.speed = c.speed.clamp(0.1, 8.0);
    c.volume = c.volume.clamp(0.0, 4.0);
    c.opacity = c.opacity.clamp(0.0, 1.0);
    c.scale = c.scale.clamp(0.05, 4.0);
    c.in_point = c.in_point.max(0.0);
    c.duration = c.duration.max(0.05);
    if let Some(l) = lim {
        c.duration = c.duration.min(l);
    }
    c.fade_in = c.fade_in.clamp(0.0, c.duration);
    c.fade_out = c.fade_out.clamp(0.0, c.duration);
}

fn spawn_media_job(core: &Shared, m: Media) {
    let core = core.clone();
    std::thread::spawn(move || {
        let dir = media::cache_dir_for(&core.cache_root, &m.path);
        let res = media::build_artifacts(&core.ff, &m, &dir);
        {
            let mut p = core.project.lock().unwrap();
            if let Some(x) = p.media.iter_mut().find(|x| x.id == m.id) {
                match res {
                    Ok(a) => {
                        x.proxy = a.proxy;
                        x.strip = a.strip;
                        x.wave = a.wave;
                        x.status = "ready".into();
                        x.error.clear();
                    }
                    Err(e) => {
                        x.status = "error".into();
                        x.error = e;
                    }
                }
            }
        }
        core.emit_state();
    });
}

/// Re-run background jobs for media that never finished (after opening a project).
pub fn resume_media_jobs(core: &Shared) {
    let todo: Vec<Media> = {
        let mut p = core.project.lock().unwrap();
        p.media
            .iter_mut()
            .filter(|m| {
                let gone = |p: &str| !p.is_empty() && !std::path::Path::new(p).exists();
                m.status != "ready" || gone(&m.proxy) || gone(&m.wave) || (m.has_audio && m.wave.is_empty())
            })
            .map(|m| {
                if !std::path::Path::new(&m.path).exists() {
                    m.status = "error".into();
                    m.error = "File not found".into();
                } else {
                    m.status = "working".into();
                }
                m.clone()
            })
            .filter(|m| m.status == "working")
            .collect()
    };
    for m in todo {
        spawn_media_job(core, m);
    }
    core.emit_state();
}

const READONLY: &[&str] = &["get_project", "get_timeline", "list_presets", "get_script", "get_transcript", "find_media"];

pub fn run(core: &Shared, name: &str, a: &Value) -> R {
    match name {
        "undo" | "redo" => return undo_redo(core, name == "undo"),
        "new_project" => return new_project(core, a),
        "open_project" => return open_project(core, s(a, "path")?),
        "save_project" => return save_project(core, os(a, "path")),
        _ => {}
    }
    let mut p = core.project.lock().unwrap();
    let before = if READONLY.contains(&name) { String::new() } else { serde_json::to_string(&*p).unwrap_or_default() };
    let out = apply(core, &mut p, name, a);
    if out.is_ok() && !READONLY.contains(&name) {
        fix_all(&mut p);
        let after = serde_json::to_string(&*p).unwrap_or_default();
        if after != before {
            {
                let mut u = core.undo.lock().unwrap();
                u.push(before);
                if u.len() > 150 {
                    u.remove(0);
                }
            }
            core.redo.lock().unwrap().clear();
            core.dirty.store(true, Ordering::Relaxed);
        }
    }
    drop(p);
    if out.is_ok() && !READONLY.contains(&name) {
        crate::store::autosave(core);
        core.emit_state();
    }
    out
}

fn undo_redo(core: &Shared, undo: bool) -> R {
    let mut p = core.project.lock().unwrap();
    let snap = if undo { core.undo.lock().unwrap().pop() } else { core.redo.lock().unwrap().pop() };
    let Some(snap) = snap else { return Err(format!("nothing to {}", if undo { "undo" } else { "redo" })) };
    let mut np: Project = serde_json::from_str(&snap).map_err(|e| e.to_string())?;
    let cur = serde_json::to_string(&*p).unwrap_or_default();
    if undo { core.redo.lock().unwrap().push(cur) } else { core.undo.lock().unwrap().push(cur) }
    // media artifacts are not part of history: keep the live pool, restore removed entries
    let live = p.media.clone();
    let mut media = live.clone();
    for m in np.media.drain(..) {
        if !live.iter().any(|x| x.id == m.id) {
            media.push(m);
        }
    }
    np.media = media;
    np.next_id = np.next_id.max(p.next_id);
    *p = np;
    drop(p);
    crate::store::autosave(core);
    core.emit_state();
    Ok(json!({"ok": true}))
}

fn new_project(core: &Shared, a: &Value) -> R {
    {
        let mut p = core.project.lock().unwrap();
        *p = Project::default();
        if let Some(n) = os(a, "name") {
            p.name = n.into();
        }
    }
    core.undo.lock().unwrap().clear();
    core.redo.lock().unwrap().clear();
    *core.path.lock().unwrap() = None;
    core.dirty.store(false, Ordering::Relaxed);
    crate::store::autosave(core);
    core.emit_state();
    Ok(json!({"ok": true}))
}

fn open_project(core: &Shared, path: &str) -> R {
    let txt = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let np: Project = serde_json::from_str(&txt).map_err(|e| format!("not a Cutlight project: {e}"))?;
    *core.project.lock().unwrap() = np;
    core.undo.lock().unwrap().clear();
    core.redo.lock().unwrap().clear();
    *core.path.lock().unwrap() = Some(path.into());
    core.dirty.store(false, Ordering::Relaxed);
    crate::store::remember_last(core);
    resume_media_jobs(core);
    Ok(json!({"ok": true}))
}

fn save_project(core: &Shared, path: Option<&str>) -> R {
    if let Some(p) = path {
        let mut p = p.to_string();
        if !p.ends_with(".cutlight") {
            p.push_str(".cutlight");
        }
        *core.path.lock().unwrap() = Some(p.into());
    }
    let target = core.path.lock().unwrap().clone().ok_or("no path: pass 'path' to save a new project")?;
    let txt = serde_json::to_string_pretty(&*core.project.lock().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&target, txt).map_err(|e| e.to_string())?;
    core.dirty.store(false, Ordering::Relaxed);
    crate::store::remember_last(core);
    core.emit_state();
    Ok(json!({"saved": target.to_string_lossy()}))
}

fn apply(core: &Shared, p: &mut Project, name: &str, a: &Value) -> R {
    match name {
        "get_project" => Ok(serde_json::to_value(&*p).unwrap()),
        "get_timeline" => {
            let tracks: Vec<Value> = p
                .tracks
                .iter()
                .map(|t| {
                    json!({
                        "id": t.id, "name": t.name, "kind": t.kind, "muted": t.muted, "hidden": t.hidden, "locked": t.locked,
                        "clips": t.clips.iter().map(|c| json!({
                            "id": c.id, "kind": c.kind, "label": c.label, "mediaId": c.media_id,
                            "start": c.start, "end": c.end(), "inPoint": c.in_point, "speed": c.speed,
                            "audio": c.audio, "volume": c.volume, "transition": c.transition,
                        })).collect::<Vec<_>>()
                    })
                })
                .collect();
            Ok(json!({"duration": p.duration(), "settings": p.settings, "tracks": tracks}))
        }
        "list_presets" => Ok(json!(PRESETS
            .iter()
            .map(|x| json!({"id": x.id, "label": x.label, "group": x.group, "width": x.w, "height": x.h, "fps": x.fps, "note": x.note, "maxSeconds": x.max_s}))
            .collect::<Vec<_>>())),
        "find_media" => {
            let q = os(a, "query").unwrap_or("").to_lowercase();
            Ok(json!(p.media.iter().filter(|m| m.name.to_lowercase().contains(&q)).collect::<Vec<_>>()))
        }

        // ---------- media ----------
        "import_media" => {
            let path = s(a, "path")?;
            if let Some(m) = p.media.iter().find(|m| m.path == path) {
                return Ok(json!({"mediaId": m.id, "existing": true}));
            }
            let mut m = core.ff.probe(path)?;
            m.id = p.new_id("m");
            let id = m.id.clone();
            p.media.push(m.clone());
            spawn_media_job(core, m);
            Ok(json!({"mediaId": id}))
        }
        "remove_media" => {
            let id = s(a, "id")?;
            p.media.retain(|m| m.id != id);
            for t in p.tracks.iter_mut() {
                t.clips.retain(|c| c.media_id.as_deref() != Some(id));
            }
            Ok(json!({"ok": true}))
        }

        // ---------- clips ----------
        "add_clip" => {
            let mid = s(a, "mediaId")?;
            let m = p.media(mid).cloned().ok_or("media not found")?;
            let mode = os(a, "mode").unwrap_or(if m.kind == "audio" { "audio" } else { "both" });
            let as_audio = mode == "audio";
            if as_audio && !m.has_audio {
                return Err("this media has no audio".into());
            }
            if !as_audio && !m.has_video {
                return Err("this media has no video".into());
            }
            let dur = of(a, "duration").unwrap_or(m.duration).max(0.05);
            let inp = of(a, "inPoint").unwrap_or(0.0);
            let kind = if as_audio { "audio" } else { m.kind.as_str() };
            let track_kind = if as_audio { "audio" } else { "video" };
            let ti = match os(a, "trackId") {
                Some(id) => find_track(p, id)?,
                None => {
                    // default: bottom video track (V1) / first audio track
                    let mut it = p.tracks.iter().enumerate().filter(|(_, t)| t.kind == track_kind && !t.locked);
                    let first = if as_audio { it.next() } else { it.last() };
                    match first {
                        Some((i, _)) => i,
                        None => free_track(p, track_kind, 0.0, 1.0),
                    }
                }
            };
            if p.tracks[ti].kind != track_kind {
                return Err(format!("a {} clip can't go on a {} track", kind, p.tracks[ti].kind));
            }
            let start = of(a, "start").unwrap_or_else(|| track_end(&p.tracks[ti]));
            let id = p.new_id("c");
            let clip = Clip {
                id: id.clone(),
                kind: kind.into(),
                media_id: Some(m.id.clone()),
                label: m.name.clone(),
                start,
                in_point: inp,
                duration: dur,
                audio: (mode == "both" || as_audio) && m.has_audio,
                ..Default::default()
            };
            p.tracks[ti].clips.push(clip);
            let ci = p.tracks[ti].clips.len() - 1;
            clamp_clip(p, ti, ci);
            fix_track(&mut p.tracks[ti]);
            Ok(json!({"clipId": id}))
        }
        "remove_clip" => {
            let ids: Vec<String> = if let Some(l) = a.get("ids").and_then(|v| v.as_array()) {
                l.iter().filter_map(|v| v.as_str().map(String::from)).collect()
            } else {
                vec![s(a, "id")?.to_string()]
            };
            let ripple = ob(a, "ripple").unwrap_or(false);
            for id in ids {
                let (ti, ci) = find_clip(p, &id)?;
                let c = p.tracks[ti].clips.remove(ci);
                if ripple {
                    let gap = c.duration - c.transition.as_ref().map(|t| t.duration).unwrap_or(0.0);
                    shift_after(&mut p.tracks[ti], c.end(), -gap.max(0.0));
                }
            }
            Ok(json!({"ok": true}))
        }
        "split_clip" => {
            let at = f(a, "at")?;
            if let Some(id) = os(a, "id") {
                let (ti, ci) = find_clip(p, id)?;
                let n = split_at(p, ti, ci, at).ok_or("playhead is not inside the clip")?;
                return Ok(json!({"newClipId": n}));
            }
            let mut made = vec![];
            for ti in 0..p.tracks.len() {
                if p.tracks[ti].locked {
                    continue;
                }
                if let Some(ci) = p.tracks[ti].clips.iter().position(|c| c.start < at && c.end() > at) {
                    if let Some(n) = split_at(p, ti, ci, at) {
                        made.push(n);
                    }
                }
            }
            if made.is_empty() {
                return Err("no clip under that time".into());
            }
            Ok(json!({"newClipIds": made}))
        }
        "trim_clip" => {
            let (ti, ci) = find_clip(p, s(a, "id")?)?;
            let at = f(a, "at")?;
            let side = s(a, "side")?;
            let c = p.tracks[ti].clips[ci].clone();
            let m = &mut p.tracks[ti].clips[ci];
            if side == "start" {
                let at = at.clamp(0.0, c.end() - 0.05);
                let mut delta = at - c.start;
                if c.kind == "video" || c.kind == "audio" {
                    delta = delta.max(-c.in_point / c.speed); // can't reveal before the media start
                }
                m.start = c.start + delta;
                m.in_point = (c.in_point + delta * c.speed).max(0.0);
                m.duration = c.duration - delta;
            } else if side == "end" {
                m.duration = (at - c.start).max(0.05);
            } else {
                return Err("side must be 'start' or 'end'".into());
            }
            clamp_clip(p, ti, ci);
            Ok(json!({"ok": true}))
        }
        "move_clip" => {
            let (ti, ci) = find_clip(p, s(a, "id")?)?;
            let start = f(a, "start")?.max(0.0);
            let mut c = p.tracks[ti].clips.remove(ci);
            let mut to = ti;
            if let Some(tid) = os(a, "trackId") {
                let t2 = find_track(p, tid)?;
                let want = if c.kind == "audio" { "audio" } else { "video" };
                if p.tracks[t2].kind != want {
                    p.tracks[ti].clips.insert(ci, c);
                    return Err(format!("can't move a {} clip to a {} track", want, p.tracks[t2].kind));
                }
                to = t2;
            }
            c.start = start;
            c.transition = None;
            p.tracks[to].clips.push(c);
            Ok(json!({"ok": true}))
        }
        "set_clip" => {
            let (ti, ci) = find_clip(p, s(a, "id")?)?;
            {
                let c = &mut p.tracks[ti].clips[ci];
                if let Some(v) = os(a, "label") { c.label = v.into(); }
                if let Some(v) = of(a, "start") { c.start = v; }
                if let Some(v) = of(a, "inPoint") { c.in_point = v; }
                if let Some(v) = of(a, "duration") { c.duration = v; }
                if let Some(v) = of(a, "speed") {
                    let v = v.clamp(0.1, 8.0);
                    c.duration = c.duration * c.speed / v; // keep the same source range
                    c.speed = v;
                }
                if let Some(v) = of(a, "volume") { c.volume = v; }
                if let Some(v) = of(a, "fadeIn") { c.fade_in = v; }
                if let Some(v) = of(a, "fadeOut") { c.fade_out = v; }
                if let Some(v) = ob(a, "audio") { c.audio = v; }
                if let Some(v) = os(a, "fit") { c.fit = v.into(); }
                if let Some(v) = of(a, "opacity") { c.opacity = v; }
                if let Some(v) = of(a, "scale") { c.scale = v; }
                if let Some(v) = of(a, "x") { c.x = v; }
                if let Some(v) = of(a, "y") { c.y = v; }
                if let Some(v) = a.get("html").and_then(|v| v.as_str()) { c.html = Some(v.into()); }
            }
            clamp_clip(p, ti, ci);
            Ok(json!({"ok": true}))
        }
        "set_transition" => {
            let (ti, ci) = find_clip(p, s(a, "clipId")?)?;
            let kind = os(a, "kind").unwrap_or("none");
            let t = &mut p.tracks[ti];
            // current overlap
            let old = t.clips[ci].transition.as_ref().map(|x| x.duration).unwrap_or(0.0);
            let prev_end = if ci > 0 { Some((t.clips[ci - 1].end(), t.clips[ci - 1].duration)) } else { None };
            if kind == "none" || kind.is_empty() {
                if old > 0.0 {
                    let from = t.clips[ci].start;
                    t.clips[ci].transition = None;
                    shift_after(t, from, old);
                }
                return Ok(json!({"ok": true}));
            }
            let (pe, pd) = prev_end.ok_or("a transition needs a clip before it on the same track")?;
            let gap = t.clips[ci].start - (pe - old);
            if gap > 0.25 {
                return Err("clips are not adjacent: close the gap first (organize → close_gaps)".into());
            }
            let want = of(a, "duration").unwrap_or(0.5);
            let d = want.min(pd * 0.9).min(t.clips[ci].duration * 0.9).max(0.1);
            let from = t.clips[ci].start;
            t.clips[ci].transition = Some(Transition { kind: kind.into(), duration: d });
            let new_start = pe - d;
            let delta = new_start - from;
            t.clips[ci].start = new_start;
            shift_after_index(t, ci, delta);
            Ok(json!({"ok": true, "duration": d}))
        }
        "detach_audio" | "remove_audio" | "remove_video" => {
            let (ti, ci) = find_clip(p, s(a, "id")?)?;
            let c = p.tracks[ti].clips[ci].clone();
            if c.kind != "video" {
                return Err("only video clips have audio to detach/remove".into());
            }
            let m = c.media_id.as_deref().and_then(|i| p.media(i)).cloned();
            let has_audio = m.map(|m| m.has_audio).unwrap_or(false) && c.audio;
            if name == "remove_audio" {
                p.tracks[ti].clips[ci].audio = false;
                return Ok(json!({"ok": true}));
            }
            let mut audio_id = Value::Null;
            if has_audio {
                let ati = free_track(p, "audio", c.start, c.end());
                let id = p.new_id("c");
                let mut ac = c.clone();
                ac.id = id.clone();
                ac.kind = "audio".into();
                ac.transition = None;
                ac.audio = true;
                ac.opacity = 1.0;
                p.tracks[ati].clips.push(ac);
                audio_id = json!(id);
            }
            let (ti, ci) = find_clip(p, &c.id)?;
            if name == "detach_audio" {
                p.tracks[ti].clips[ci].audio = false;
            } else {
                p.tracks[ti].clips.remove(ci); // remove_video: keep only the audio
            }
            Ok(json!({"audioClipId": audio_id}))
        }

        // ---------- tracks ----------
        "add_track" => {
            let kind = os(a, "kind").unwrap_or("video");
            let n = p.tracks.iter().filter(|t| t.kind == kind).count() + 1;
            let id = p.new_id("t");
            let t = Track {
                id: id.clone(),
                kind: kind.into(),
                name: os(a, "name").map(String::from).unwrap_or(format!("{}{}", if kind == "audio" { "A" } else { "V" }, n)),
                ..Default::default()
            };
            if kind == "audio" {
                p.tracks.push(t);
            } else {
                let at = p.tracks.iter().position(|t| t.kind == "video").unwrap_or(0);
                p.tracks.insert(at, t);
            }
            Ok(json!({"trackId": id}))
        }
        "remove_track" => {
            let ti = find_track(p, s(a, "id")?)?;
            if p.tracks.iter().filter(|t| t.kind == p.tracks[ti].kind).count() <= 1 {
                return Err("keep at least one track of each kind".into());
            }
            p.tracks.remove(ti);
            Ok(json!({"ok": true}))
        }
        "set_track" => {
            let ti = find_track(p, s(a, "id")?)?;
            let t = &mut p.tracks[ti];
            if let Some(v) = os(a, "name") { t.name = v.into(); }
            if let Some(v) = ob(a, "muted") { t.muted = v; }
            if let Some(v) = ob(a, "hidden") { t.hidden = v; }
            if let Some(v) = ob(a, "locked") { t.locked = v; }
            Ok(json!({"ok": true}))
        }
        "move_track" => {
            let ti = find_track(p, s(a, "id")?)?;
            let to = (f(a, "index")? as usize).min(p.tracks.len() - 1);
            let t = p.tracks.remove(ti);
            p.tracks.insert(to, t);
            Ok(json!({"ok": true}))
        }

        // ---------- timeline-wide ----------
        "delete_range" => {
            let (s0, e0) = (f(a, "start")?, f(a, "end")?);
            if e0 <= s0 {
                return Err("end must be after start".into());
            }
            let ripple = ob(a, "ripple").unwrap_or(true);
            for ti in 0..p.tracks.len() {
                if p.tracks[ti].locked {
                    continue;
                }
                for at in [e0, s0] {
                    if let Some(ci) = p.tracks[ti].clips.iter().position(|c| c.start < at - 0.02 && c.end() > at + 0.02) {
                        split_at(p, ti, ci, at);
                    }
                }
                let t = &mut p.tracks[ti];
                t.clips.retain(|c| !(c.start >= s0 - 0.03 && c.end() <= e0 + 0.03));
                if ripple {
                    shift_after(t, e0 - 0.03, -(e0 - s0));
                }
            }
            Ok(json!({"ok": true}))
        }
        "organize" => {
            let mode = os(a, "mode").unwrap_or("close_gaps");
            let only = os(a, "trackId").map(String::from);
            match mode {
                "close_gaps" | "tidy" => {
                    for t in p.tracks.iter_mut().filter(|t| only.as_deref().map_or(true, |i| i == t.id)) {
                        if t.locked {
                            continue;
                        }
                        t.clips.sort_by(|x, y| x.start.partial_cmp(&y.start).unwrap());
                        let mut cursor = 0.0;
                        for c in t.clips.iter_mut() {
                            let ov = c.transition.as_ref().map(|x| x.duration).unwrap_or(0.0);
                            c.start = (cursor - ov).max(0.0);
                            cursor = c.start + c.duration;
                        }
                    }
                    if mode == "tidy" {
                        for kind in ["video", "audio"] {
                            let mut keep = p.tracks.iter().filter(|t| t.kind == kind).count();
                            p.tracks.retain(|t| {
                                if t.kind == kind && t.clips.is_empty() && keep > 1 {
                                    keep -= 1;
                                    false
                                } else {
                                    true
                                }
                            });
                        }
                        // rename V/A in order
                        let (mut v, mut au) = (0, 0);
                        let nv = p.tracks.iter().filter(|t| t.kind == "video").count();
                        for t in p.tracks.iter_mut() {
                            if t.kind == "video" {
                                t.name = format!("V{}", nv - v);
                                v += 1;
                            } else {
                                au += 1;
                                t.name = format!("A{au}");
                            }
                        }
                    }
                }
                "align_start" => {
                    let min = p.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| c.start).fold(f64::MAX, f64::min);
                    if min.is_finite() && min > 0.0 {
                        for t in p.tracks.iter_mut() {
                            for c in t.clips.iter_mut() {
                                c.start -= min;
                            }
                        }
                    }
                }
                _ => return Err("mode: close_gaps | tidy | align_start".into()),
            }
            Ok(json!({"ok": true}))
        }

        // ---------- html ----------
        "add_html" => {
            let html = s(a, "html")?.to_string();
            let start = f(a, "start")?;
            let dur = f(a, "duration")?.max(0.1);
            let ti = match os(a, "trackId") {
                Some(id) => find_track(p, id)?,
                None => p.tracks.iter().position(|t| t.kind == "video" && !t.locked).ok_or("no video track")?,
            };
            if p.tracks[ti].kind != "video" {
                return Err("html overlays go on video tracks".into());
            }
            let id = p.new_id("c");
            p.tracks[ti].clips.push(Clip {
                id: id.clone(),
                kind: "html".into(),
                label: os(a, "label").unwrap_or("HTML animation").into(),
                start,
                duration: dur,
                audio: false,
                html: Some(html),
                ..Default::default()
            });
            Ok(json!({"clipId": id}))
        }
        "add_captions" => {
            let segs: Vec<(f64, f64, String)> = match a.get("segments").and_then(|v| v.as_array()) {
                Some(l) => l
                    .iter()
                    .map(|x| (x["start"].as_f64().unwrap_or(0.0), x["end"].as_f64().unwrap_or(0.0), x["text"].as_str().unwrap_or("").to_string()))
                    .collect(),
                None => p.transcript.iter().map(|x| (x.start, x.end, x.text.clone())).collect(),
            };
            if segs.is_empty() {
                return Err("no transcript segments to caption".into());
            }
            let total = segs.iter().map(|s| s.1).fold(0.0, f64::max);
            let html = crate::templates::captions(&segs, os(a, "position").unwrap_or("bottom"));
            let ti = p.tracks.iter().position(|t| t.kind == "video" && !t.locked).ok_or("no video track")?;
            let id = p.new_id("c");
            p.tracks[ti].clips.push(Clip {
                id: id.clone(),
                kind: "html".into(),
                label: "Captions".into(),
                start: 0.0,
                duration: total,
                audio: false,
                html: Some(html),
                ..Default::default()
            });
            Ok(json!({"clipId": id}))
        }

        // ---------- project ----------
        "set_settings" => {
            let st = &mut p.settings;
            if let Some(id) = os(a, "preset") {
                if let Some(pr) = preset(id) {
                    st.preset = pr.id.into();
                    st.width = pr.w;
                    st.height = pr.h;
                    st.fps = pr.fps;
                } else if id == "custom" {
                    st.preset = "custom".into();
                } else {
                    return Err(format!("unknown preset '{id}' (see list_presets)"));
                }
            }
            if let Some(v) = of(a, "width") { st.width = (v as u32).max(2) & !1; st.preset = "custom".into(); }
            if let Some(v) = of(a, "height") { st.height = (v as u32).max(2) & !1; st.preset = "custom".into(); }
            if let Some(v) = of(a, "fps") { st.fps = (v as u32).clamp(1, 120); }
            if let Some(v) = os(a, "quality") { st.quality = v.into(); }
            if let Some(v) = of(a, "videoBitrateK") { st.video_bitrate_k = v as u32; }
            if let Some(v) = of(a, "audioBitrateK") { st.audio_bitrate_k = (v as u32).clamp(32, 512); }
            if let Some(v) = of(a, "sampleRate") { st.sample_rate = v as u32; }
            if let Some(v) = of(a, "channels") { st.channels = (v as u32).clamp(1, 2); }
            if let Some(v) = ob(a, "normalizeAudio") { st.normalize_audio = v; }
            if let Some(v) = ob(a, "hardware") { st.hardware = v; }
            if let Some(v) = os(a, "format") { st.format = v.into(); }
            if let Some(v) = os(a, "name") { p.name = v.into(); }
            Ok(json!({"settings": p.settings}))
        }
        "set_script" => {
            p.script = s(a, "text")?.to_string();
            Ok(json!({"ok": true}))
        }
        "get_script" => Ok(json!({"script": p.script})),
        "get_transcript" => Ok(json!({"segments": p.transcript, "script": p.script})),
        "set_transcript" => {
            let list = a.get("segments").and_then(|v| v.as_array()).ok_or("segments array required")?;
            let mut out = vec![];
            for x in list {
                let id = p.new_id("s");
                out.push(Segment {
                    id,
                    start: x["start"].as_f64().unwrap_or(0.0),
                    end: x["end"].as_f64().unwrap_or(0.0),
                    text: x["text"].as_str().unwrap_or("").to_string(),
                });
            }
            p.transcript = out;
            Ok(json!({"count": p.transcript.len()}))
        }
        "update_segment" => {
            let id = s(a, "id")?;
            let seg = p.transcript.iter_mut().find(|x| x.id == id).ok_or("segment not found")?;
            if let Some(v) = os(a, "text") { seg.text = v.into(); }
            if let Some(v) = of(a, "start") { seg.start = v; }
            if let Some(v) = of(a, "end") { seg.end = v; }
            Ok(json!({"ok": true}))
        }
        _ => Err(format!("unknown operation '{name}'")),
    }
}

fn shift_after_index(t: &mut Track, ci: usize, delta: f64) {
    for c in t.clips.iter_mut().skip(ci + 1) {
        c.start = (c.start + delta).max(0.0);
    }
}
