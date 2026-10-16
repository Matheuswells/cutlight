//! Local MCP server (streamable HTTP, JSON responses) so Claude can drive every editor function,
//! see the UI (screenshots) and control it. Bound to 127.0.0.1 only; browser-origin requests are refused.

use crate::core::Shared;
use crate::{ops, render};
use base64::Engine;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

fn t(name: &str, desc: &str, props: &[(&str, &str, &str)], req: &[&str]) -> Value {
    let mut p = serde_json::Map::new();
    for (k, ty, d) in props {
        p.insert(k.to_string(), json!({"type": ty, "description": d}));
    }
    json!({"name": name, "description": desc, "inputSchema": {"type": "object", "properties": p, "required": req}})
}

fn tools() -> Vec<Value> {
    let id = ("id", "string", "clip id");
    vec![
        t("get_timeline", "Compact view of every track and clip (ids, start/end seconds, transitions). Call this first.", &[], &[]),
        t("get_project", "Full project JSON including media pool, settings, script and transcript.", &[], &[]),
        t("list_presets", "Instagram/social/video size presets.", &[], &[]),
        t("list_projects", "Recent projects (path, name, duration). Cutlight starts with no project open: open one or create one first.", &[], &[]),
        t("close_project", "Close the current project and return to the start screen.", &[], &[]),
        t("remove_recent", "Remove a project from the start screen list (deleteFile only deletes files Cutlight created).", &[("path", "string", ""), ("deleteFile", "boolean", "")], &["path"]),
        t("new_project", "Create and open an empty project.", &[("name", "string", "project name")], &[]),
        t("open_project", "Open a .cutlight project file.", &[("path", "string", "absolute path")], &["path"]),
        t("save_project", "Save the project (pass path the first time).", &[("path", "string", "absolute path ending .cutlight")], &[]),
        t("undo", "Undo the last change.", &[], &[]),
        t("redo", "Redo.", &[], &[]),
        t("import_media", "Import a video, audio or image file into the media pool (proxy, thumbnails and waveform are built in the background; check media status in get_project).", &[("path", "string", "absolute file path")], &["path"]),
        t("remove_media", "Remove media from the pool and every clip that uses it.", &[("id", "string", "media id")], &["id"]),
        t("find_media", "Search the media pool by name.", &[("query", "string", "substring")], &[]),
        t("add_clip", "Put media on the timeline. mode: both (video + its audio), video (picture only), audio (sound only, goes on an audio track).", &[("mediaId", "string", ""), ("trackId", "string", "optional; default is V1 / A1"), ("start", "number", "timeline seconds; default end of track"), ("inPoint", "number", "source start seconds"), ("duration", "number", "seconds"), ("mode", "string", "both | video | audio")], &["mediaId"]),
        t("remove_clip", "Delete a clip (ripple closes the gap).", &[id, ("ids", "array", "several clip ids"), ("ripple", "boolean", "")], &[]),
        t("split_clip", "Cut a clip at a timeline time. Without id, cuts every clip under that time.", &[id, ("at", "number", "timeline seconds")], &["at"]),
        t("trim_clip", "Move a clip edge to a timeline time.", &[id, ("side", "string", "start | end"), ("at", "number", "timeline seconds")], &["id", "side", "at"]),
        t("move_clip", "Move a clip in time and/or to another track.", &[id, ("start", "number", ""), ("trackId", "string", "")], &["id", "start"]),
        t("set_clip", "Edit clip properties: label,start,inPoint,duration,speed,volume(0-4),fadeIn,fadeOut,audio(bool),fit(fit|fill),opacity,scale,x,y(0-1 center),html.", &[id, ("label", "string", ""), ("start", "number", ""), ("inPoint", "number", ""), ("duration", "number", ""), ("speed", "number", ""), ("volume", "number", ""), ("fadeIn", "number", ""), ("fadeOut", "number", ""), ("audio", "boolean", ""), ("fit", "string", ""), ("opacity", "number", ""), ("scale", "number", ""), ("x", "number", ""), ("y", "number", ""), ("html", "string", "")], &["id"]),
        t("set_transition", "Transition INTO this clip from the previous clip on the track: dissolve | fade-black | slide-left | slide-right | slide-up | slide-down | none. The clip slides earlier by the duration (overlap).", &[("clipId", "string", ""), ("kind", "string", ""), ("duration", "number", "seconds, default 0.5")], &["clipId", "kind"]),
        t("detach_audio", "Split a video clip's audio onto an audio track (video keeps playing silent).", &[id], &["id"]),
        t("remove_audio", "Delete the audio of a video clip.", &[id], &["id"]),
        t("remove_video", "Delete the picture of a clip, keep only its audio on an audio track.", &[id], &["id"]),
        t("add_track", "Add a video or audio track.", &[("kind", "string", "video | audio"), ("name", "string", "")], &[]),
        t("remove_track", "Remove a track (and its clips).", &[("id", "string", "track id")], &["id"]),
        t("set_track", "Rename / mute / hide / lock a track.", &[("id", "string", "track id"), ("name", "string", ""), ("muted", "boolean", ""), ("hidden", "boolean", ""), ("locked", "boolean", "")], &["id"]),
        t("move_track", "Reorder tracks (index 0 = top layer).", &[("id", "string", ""), ("index", "number", "")], &["id", "index"]),
        t("delete_range", "Remove a time range from all unlocked tracks (ripple by default). Ideal for cutting filler/mistakes found in the transcript.", &[("start", "number", ""), ("end", "number", ""), ("ripple", "boolean", "default true")], &["start", "end"]),
        t("organize", "Tidy the timeline. mode: close_gaps | tidy (close gaps + drop empty tracks) | align_start.", &[("mode", "string", ""), ("trackId", "string", "optional")], &[]),
        t("add_html", "Add an HTML animation overlay clip. HTML must be self-contained with a transparent background, laid out in vw/vh, animated with CSS animations / Web Animations, or define window.__seek(t) (seconds from clip start) for JS-driven animation. Inline images as data: URIs. It previews live and is rendered frame-exact.", &[("html", "string", "full HTML document"), ("start", "number", ""), ("duration", "number", ""), ("trackId", "string", ""), ("label", "string", "")], &["html", "start", "duration"]),
        t("add_captions", "Burn captions from the transcript (or the given segments) as an HTML overlay.", &[("segments", "array", "[{start,end,text}] optional"), ("position", "string", "bottom | middle | top")], &[]),
        t("set_settings", "Output settings: preset (see list_presets), width, height, fps, quality (draft|good|high|max), videoBitrateK (0 = quality based), audioBitrateK, sampleRate, channels(1|2), normalizeAudio (-14 LUFS), hardware (GPU encode), format (mp4|mov), name.", &[("preset", "string", ""), ("width", "number", ""), ("height", "number", ""), ("fps", "number", ""), ("quality", "string", ""), ("videoBitrateK", "number", ""), ("audioBitrateK", "number", ""), ("sampleRate", "number", ""), ("channels", "number", ""), ("normalizeAudio", "boolean", ""), ("hardware", "boolean", ""), ("format", "string", ""), ("name", "string", "")], &[]),
        t("set_script", "Store the intended script of the video (the creator can paste it in the Script tab). Use it to correct the transcript.", &[("text", "string", "")], &["text"]),
        t("get_script", "Read the script the creator pasted.", &[], &[]),
        t("get_transcript", "Read transcript segments (from Whisper) plus the script.", &[], &[]),
        t("set_transcript", "Replace the transcript with corrected segments.", &[("segments", "array", "[{start,end,text}]")], &["segments"]),
        t("update_segment", "Fix one transcript segment.", &[("id", "string", ""), ("text", "string", ""), ("start", "number", ""), ("end", "number", "")], &["id"]),
        t("transcribe", "Run Whisper (offline) on a media file and store segments as the transcript. Use the script to review and fix errors afterwards.", &[("mediaId", "string", ""), ("language", "string", "ISO code, e.g. pt, en; omit to auto-detect")], &["mediaId"]),
        t("create_shortcuts", "Create (or refresh) the Cutlight shortcuts on the desktop and in the Start/Applications menu.", &[], &[]),
        t("render", "Render the timeline to a video file. Returns immediately; poll get_render_status.", &[("path", "string", "absolute output path, .mp4 or .mov")], &["path"]),
        t("get_render_status", "Render progress (state idle|running|done|error|cancelled).", &[], &[]),
        t("cancel_render", "Cancel a running render.", &[], &[]),
        t("screenshot", "PNG screenshot of the editor window.", &[], &[]),
        t("ui_state", "What the UI shows: playhead, selection, zoom, active tab, playing.", &[], &[]),
        t("ui_control", "Drive the UI: action = play | pause | seek(t) | select(clipId) | zoom(pxPerSec) | tab(media|script|export) | fit_timeline.", &[("action", "string", ""), ("value", "string", "argument as string or number")], &["action"]),
        t("ui_eval", "Run JavaScript in the editor UI (debugging/inspection). Returns the value.", &[("js", "string", "expression or async function body; use return")], &["js"]),
        t("ui_click", "Click a UI element by CSS selector (or visible text via text=...).", &[("selector", "string", "")], &["selector"]),
    ]
}

fn text(v: &Value) -> Value {
    json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(v).unwrap_or_default()}]})
}
fn err(msg: &str) -> Value {
    json!({"isError": true, "content": [{"type": "text", "text": msg}]})
}

fn screenshot(core: &Shared) -> Result<Value, String> {
    use tauri::Manager;
    if let Some(win) = core.app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let win = core.app.get_webview_window("main").ok_or("no window")?;
    let _ = win.set_focus();
    std::thread::sleep(std::time::Duration::from_millis(300));
    // Capture the monitor and crop to the window: works for GPU-composited webviews on Windows and macOS.
    let pos = win.outer_position().map_err(|e| e.to_string())?;
    let size = win.outer_size().map_err(|e| e.to_string())?;
    let mon = xcap::Monitor::from_point(pos.x + 10, pos.y + 10).map_err(|e| e.to_string())?;
    let full = mon.capture_image().map_err(|e| e.to_string())?;
    let (mx, my) = (mon.x().map_err(|e| e.to_string())?, mon.y().map_err(|e| e.to_string())?);
    let x = (pos.x - mx).max(0) as u32;
    let y = (pos.y - my).max(0) as u32;
    let w = size.width.min(full.width().saturating_sub(x));
    let h = size.height.min(full.height().saturating_sub(y));
    let img = xcap::image::imageops::crop_imm(&full, x, y, w, h).to_image();
    let (iw, ih) = (img.width(), img.height());
    let img = if iw > 1600 {
        xcap::image::imageops::resize(&img, 1600, ih * 1600 / iw, xcap::image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut png = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png, xcap::image::ImageFormat::Png).map_err(|e| e.to_string())?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
    Ok(json!({"content": [{"type": "image", "data": b64, "mimeType": "image/png"}]}))
}

fn call(core: &Shared, name: &str, a: &Value) -> Value {
    let r: Result<Value, String> = match name {
        "render" => a["path"].as_str().ok_or("path required".to_string()).and_then(|p| render::start(core, p.to_string())),
        "get_render_status" => Ok(core.snapshot()["render"].clone()),
        "cancel_render" => {
            render::cancel(core);
            Ok(json!({"ok": true}))
        }
        "transcribe" => crate::whisper::transcribe(core, a),
        "create_shortcuts" => crate::shortcuts::create(),
        "screenshot" => return screenshot(core).unwrap_or_else(|e| err(&e)),
        "ui_state" => core.ui_call("return window.cutlight.state()", 5000),
        "ui_control" => {
            let act = serde_json::to_string(a.get("action").unwrap_or(&Value::Null)).unwrap();
            let val = serde_json::to_string(a.get("value").unwrap_or(&Value::Null)).unwrap();
            core.ui_call(&format!("return window.cutlight.control({act},{val})"), 5000)
        }
        "ui_eval" => core.ui_call(a["js"].as_str().unwrap_or("return null"), 15000),
        "ui_click" => {
            let sel = serde_json::to_string(a["selector"].as_str().unwrap_or("")).unwrap();
            core.ui_call(&format!("return window.cutlight.click({sel})"), 5000)
        }
        _ => ops::run(core, name, a),
    };
    match r {
        Ok(v) => text(&v),
        Err(e) => err(&e),
    }
}

fn rpc(core: &Shared, req: &Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req["method"].as_str().unwrap_or("");
    id.as_ref()?; // notifications get no response
    let result = match method {
        "initialize" => json!({
            "protocolVersion": req["params"]["protocolVersion"].as_str().unwrap_or("2025-03-26"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "cutlight", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "Cutlight video editor. If no project is open (start screen), use list_projects then open_project, or new_project. Then get_timeline and get_project. Times are seconds on the timeline. The user may have pasted a script (get_script): compare it with get_transcript and fix mistakes with set_transcript/update_segment. Use screenshot to see the UI. HTML animations: see add_html."
        }),
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => {
            let name = req["params"]["name"].as_str().unwrap_or("");
            call(core, name, &req["params"]["arguments"])
        }
        "ping" => json!({}),
        _ => {
            return Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}}));
        }
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

pub fn start(core: Shared) {
    std::thread::spawn(move || {
        let mut server = None;
        for port in 47321..47341u16 {
            if let Ok(s) = tiny_http::Server::http(("127.0.0.1", port)) {
                core.mcp_port.store(port, Ordering::Relaxed);
                server = Some(s);
                break;
            }
        }
        let Some(server) = server else { return };
        core.emit_state();
        for mut req in server.incoming_requests() {
            let core = core.clone();
            std::thread::spawn(move || {
                let hdr = |n: &'static str| req.headers().iter().find(|h| h.field.equiv(n)).map(|h| h.value.to_string());
                let host_ok = hdr("Host").map(|h| h.starts_with("127.0.0.1") || h.starts_with("localhost")).unwrap_or(false);
                let has_origin = hdr("Origin").is_some();
                if has_origin || !host_ok {
                    let _ = req.respond(tiny_http::Response::empty(403));
                    return;
                }
                if req.method() != &tiny_http::Method::Post {
                    let _ = req.respond(tiny_http::Response::empty(405));
                    return;
                }
                let mut body = String::new();
                let _ = std::io::Read::read_to_string(req.as_reader(), &mut body);
                let parsed: Result<Value, _> = serde_json::from_str(&body);
                let reply = match parsed {
                    Ok(Value::Array(items)) => {
                        let out: Vec<Value> = items.iter().filter_map(|r| rpc(&core, r)).collect();
                        if out.is_empty() { None } else { Some(Value::Array(out)) }
                    }
                    Ok(v) => rpc(&core, &v),
                    Err(_) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}})),
                };
                match reply {
                    Some(v) => {
                        let h = tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap();
                        let _ = req.respond(tiny_http::Response::from_string(v.to_string()).with_header(h));
                    }
                    None => {
                        let _ = req.respond(tiny_http::Response::empty(202));
                    }
                }
            });
        }
    });
}
