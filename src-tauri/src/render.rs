//! Final render: the timeline becomes one ffmpeg filter graph, run as a single optimized pass.
//!  * every clip is its own input with fast input-seeking (-ss/-t) so unused media is never decoded
//!  * layers composite with `overlay`, transitions are alpha/position animations on the incoming clip
//!  * audio is mixed once, optionally loudness-normalised
//!  * HTML animation clips are rasterised frame-by-frame by the UI and piped into ffmpeg (cached)

use crate::core::Shared;
use crate::media::quiet;
use crate::model::*;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::Ordering;
use tauri::Emitter;

fn set_status(core: &Shared, state: &str, progress: f64, message: &str, output: &str) {
    {
        let mut r = core.render.lock().unwrap();
        r.state = state.into();
        r.progress = progress;
        r.message = message.into();
        if !output.is_empty() {
            r.output = output.into();
        }
    }
    core.emit_state();
}

pub fn start(core: &Shared, out: String) -> Result<Value, String> {
    if core.render.lock().unwrap().state == "running" {
        return Err("a render is already running".into());
    }
    if core.project.lock().unwrap().duration() <= 0.0 {
        return Err("the timeline is empty".into());
    }
    core.cancel.store(false, Ordering::Relaxed);
    set_status(core, "running", 0.0, "Preparing…", &out);
    let c = core.clone();
    let out2 = out.clone();
    std::thread::spawn(move || match job(&c, &out2) {
        Ok(()) => set_status(&c, "done", 1.0, "Finished", &out2),
        Err(e) if e == "cancelled" => set_status(&c, "cancelled", 0.0, "Cancelled", &out2),
        Err(e) => set_status(&c, "error", 0.0, &e, &out2),
    });
    Ok(json!({"started": true, "output": out}))
}

pub fn cancel(core: &Shared) {
    core.cancel.store(true, Ordering::Relaxed);
    // unblock any waiting HTML render
    let mut d = core.html_done.lock().unwrap();
    for (_, tx) in d.drain() {
        let _ = tx.send(Err("cancelled".into()));
    }
}

fn even(v: f64) -> i64 {
    ((v.round() as i64).max(2)) & !1
}

struct Placed {
    w: i64,
    h: i64,
    crop: Option<(i64, i64)>,
    x0: i64,
    y0: i64,
}

fn place(c: &Clip, mw: f64, mh: f64, tw: f64, th: f64) -> Placed {
    let sw = tw * c.scale;
    let sh = th * c.scale;
    let (mw, mh) = if mw <= 0.0 || mh <= 0.0 { (tw, th) } else { (mw, mh) };
    let (w, h, crop) = if c.fit == "fill" {
        let f = (sw / mw).max(sh / mh);
        (even(mw * f), even(mh * f), Some((even(sw), even(sh))))
    } else {
        let f = (sw / mw).min(sh / mh);
        (even(mw * f), even(mh * f), None)
    };
    let (vw, vh) = crop.unwrap_or((w, h));
    Placed {
        w,
        h,
        crop,
        x0: (c.x * tw - vw as f64 / 2.0).round() as i64,
        y0: (c.y * th - vh as f64 / 2.0).round() as i64,
    }
}

fn atempo(mut s: f64) -> Vec<String> {
    let mut v = vec![];
    while s > 2.0 {
        v.push("atempo=2.0".to_string());
        s /= 2.0;
    }
    while s < 0.5 {
        v.push("atempo=0.5".to_string());
        s *= 2.0;
    }
    if (s - 1.0).abs() > 1e-3 {
        v.push(format!("atempo={s:.4}"));
    }
    v
}

pub struct Graph {
    pub inputs: Vec<Vec<String>>,
    pub script: String,
}

pub fn build_graph(p: &Project, html_files: &HashMap<String, PathBuf>) -> Result<Graph, String> {
    let st = &p.settings;
    let (tw, th, fps) = (st.width as f64, st.height as f64, st.fps.max(1) as f64);
    let dur = p.duration();
    let draft = st.quality == "draft";
    let mut inputs: Vec<Vec<String>> = vec![];
    let mut lines: Vec<String> = vec![];
    let layout = if st.channels == 1 { "mono" } else { "stereo" };
    let mut alabels: Vec<String> = vec![];

    lines.push(format!("color=c=black:s={}x{}:r={}:d={:.4},format=yuv420p[v0]", st.width, st.height, st.fps, dur));
    let mut cur = "v0".to_string();
    let mut vn = 0;

    // bottom layer first
    for track in p.tracks.iter().rev() {
        let mut clips: Vec<&Clip> = track.clips.iter().collect();
        clips.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        for (idx, c) in clips.iter().enumerate() {
            let m = c.media_id.as_deref().and_then(|i| p.media(i));
            let src = |m: &Media| if draft && !m.proxy.is_empty() { m.proxy.clone() } else { m.path.clone() };
            let next_tr = clips.get(idx + 1).and_then(|n| n.transition.as_ref()).filter(|_| {
                clips.get(idx + 1).map(|n| n.start < c.end() + 0.05).unwrap_or(false)
            });

            // ---- input ----
            let i = inputs.len();
            let mut args: Vec<String> = vec![];
            match c.kind.as_str() {
                "video" | "audio" => {
                    let m = m.ok_or_else(|| format!("clip {} lost its media", c.id))?;
                    if !std::path::Path::new(&m.path).exists() {
                        return Err(format!("source file missing: {}", m.path));
                    }
                    args.extend(["-ss".into(), format!("{:.4}", c.in_point), "-t".into(), format!("{:.4}", c.duration * c.speed + 0.1), "-i".into(), src(m)]);
                }
                "image" => {
                    let m = m.ok_or_else(|| format!("clip {} lost its media", c.id))?;
                    args.extend(["-loop".into(), "1".into(), "-framerate".into(), format!("{fps}"), "-t".into(), format!("{:.4}", c.duration), "-i".into(), m.path.clone()]);
                }
                "html" => {
                    let f = html_files.get(&c.id).ok_or("html clip was not rendered")?;
                    args.extend(["-i".into(), f.to_string_lossy().to_string()]);
                }
                k => return Err(format!("unknown clip kind {k}")),
            }
            let visible = c.kind != "audio" && !track.hidden;
            let has_audio_src = match c.kind.as_str() {
                "audio" => true,
                "video" => c.audio && m.map(|m| m.has_audio).unwrap_or(false),
                _ => false,
            };
            let want_audio = has_audio_src && !track.muted;
            if !visible && !want_audio {
                continue;
            }
            inputs.push(args);

            // ---- video ----
            if visible {
                let (mw, mh) = match c.kind.as_str() {
                    "html" => (tw, th),
                    _ => m.map(|m| (m.width as f64, m.height as f64)).unwrap_or((tw, th)),
                };
                let pl = if c.kind == "html" {
                    Placed { w: st.width as i64, h: st.height as i64, crop: None, x0: 0, y0: 0 }
                } else {
                    place(c, mw, mh, tw, th)
                };
                let mut f: Vec<String> = vec![];
                if c.kind == "video" {
                    f.push(format!("fps={fps}"));
                    f.push(format!("setpts=(PTS-STARTPTS)/{:.5}", c.speed));
                } else {
                    f.push("setpts=PTS-STARTPTS".into());
                }
                if c.kind != "html" {
                    f.push(format!("scale={}:{}:flags=bicubic", pl.w, pl.h));
                    if let Some((cw, ch)) = pl.crop {
                        f.push(format!("crop={cw}:{ch}"));
                    }
                }
                f.push("setsar=1".into());

                let tr = c.transition.as_ref();
                let fade_tr = |t: &Option<&Transition>| t.map(|t| t.kind == "dissolve" || t.kind == "fade-black").unwrap_or(false);
                let fi = c.fade_in.max(if fade_tr(&tr) { tr.unwrap().duration } else { 0.0 });
                let fo = c.fade_out.max(match next_tr {
                    Some(t) if t.kind == "fade-black" => t.duration,
                    _ => 0.0,
                });
                let need_alpha = c.kind == "html" || c.opacity < 0.999 || fi > 0.0 || fo > 0.0;
                if need_alpha {
                    f.push("format=yuva420p".into());
                    if fi > 0.0 {
                        f.push(format!("fade=t=in:st=0:d={fi:.3}:alpha=1"));
                    }
                    if fo > 0.0 {
                        f.push(format!("fade=t=out:st={:.3}:d={fo:.3}:alpha=1", (c.duration - fo).max(0.0)));
                    }
                    if c.opacity < 0.999 {
                        f.push(format!("lut=a='val*{:.3}'", c.opacity));
                    }
                }
                f.push(format!("setpts=PTS+{:.4}/TB", c.start));
                vn += 1;
                lines.push(format!("[{i}:v]{}[l{vn}]", f.join(",")));

                // position (slide transitions animate it)
                let (mut x, mut y) = (pl.x0.to_string(), pl.y0.to_string());
                if let Some(t) = tr {
                    let d = t.duration.max(0.05);
                    let prog = format!("min(1,max(0,(t-{:.4})/{d:.4}))", c.start);
                    let e = format!("(1-(({prog})*({prog})*(3-2*({prog}))))"); // 1 → 0 smoothstep
                    match t.kind.as_str() {
                        "slide-left" => x = format!("'{}+({}-({}))*{e}'", pl.x0, st.width, pl.x0),
                        "slide-right" => x = format!("'{}-({}+{})*{e}'", pl.x0, pl.x0, pl.w),
                        "slide-up" => y = format!("'{}+({}-({}))*{e}'", pl.y0, st.height, pl.y0),
                        "slide-down" => y = format!("'{}-({}+{})*{e}'", pl.y0, pl.y0, pl.h),
                        _ => {}
                    }
                }
                let out = format!("v{vn}");
                lines.push(format!(
                    "[{cur}][l{vn}]overlay=x={x}:y={y}:eof_action=pass:format=auto:enable='between(t,{:.4},{:.4})'[{out}]",
                    c.start,
                    c.end() + 0.001
                ));
                cur = out;
            }

            // ---- audio ----
            if want_audio {
                let mut f: Vec<String> = vec![
                    format!("aresample={}", st.sample_rate),
                    format!("aformat=sample_fmts=fltp:channel_layouts={layout}"),
                    "asetpts=PTS-STARTPTS".into(),
                ];
                f.extend(atempo(c.speed));
                if (c.volume - 1.0).abs() > 1e-3 {
                    f.push(format!("volume={:.3}", c.volume));
                }
                let tr_in = c.transition.as_ref().map(|t| t.duration).unwrap_or(0.0);
                let ai = c.fade_in.max(tr_in);
                let ao = c.fade_out.max(next_tr.map(|t| t.duration).unwrap_or(0.0));
                if ai > 0.0 {
                    f.push(format!("afade=t=in:st=0:d={ai:.3}"));
                }
                if ao > 0.0 {
                    f.push(format!("afade=t=out:st={:.3}:d={ao:.3}", (c.duration - ao).max(0.0)));
                }
                f.push(format!("atrim=0:{:.4}", c.duration));
                if c.start > 0.001 {
                    f.push(format!("adelay={}:all=1", (c.start * 1000.0).round() as i64));
                }
                let lab = format!("a{}", alabels.len());
                lines.push(format!("[{i}:a]{}[{lab}]", f.join(",")));
                alabels.push(lab);
            }
        }
    }
    lines.push(format!("[{cur}]null[vout]"));

    let tail = if st.normalize_audio { "loudnorm=I=-14:TP=-1.5:LRA=11,alimiter=limit=0.97" } else { "alimiter=limit=0.97" };
    if alabels.is_empty() {
        lines.push(format!("anullsrc=r={}:cl={layout}:d={dur:.4}[aout]", st.sample_rate));
    } else {
        let ins: String = alabels.iter().map(|l| format!("[{l}]")).collect();
        lines.push(format!("{ins}amix=inputs={}:normalize=0:dropout_transition=0:duration=longest,{tail}[aout]", alabels.len()));
    }
    Ok(Graph { inputs, script: lines.join(";\n") })
}

fn video_args(core: &Shared, st: &Settings, enc: &str) -> Vec<String> {
    let (preset, crf) = match st.quality.as_str() {
        "draft" => ("veryfast", 28),
        "high" => ("slow", 18),
        "max" => ("slower", 14),
        _ => ("fast", 22),
    };
    let br = st.video_bitrate_k;
    let mut a: Vec<String> = vec![];
    let s = |x: &str| x.to_string();
    match enc {
        "libx264" => {
            a.extend([s("-c:v"), s("libx264"), s("-preset"), s(preset), s("-profile:v"), s("high")]);
            if br > 0 {
                a.extend([s("-b:v"), format!("{br}k"), s("-maxrate"), format!("{}k", br * 3 / 2), s("-bufsize"), format!("{}k", br * 2)]);
            } else {
                a.extend([s("-crf"), crf.to_string()]);
            }
        }
        "h264_nvenc" => {
            a.extend([s("-c:v"), s("h264_nvenc"), s("-preset"), s("p5"), s("-rc"), s("vbr")]);
            if br > 0 { a.extend([s("-b:v"), format!("{br}k")]) } else { a.extend([s("-cq"), (crf + 1).to_string(), s("-b:v"), s("0")]) }
        }
        "h264_qsv" => {
            a.extend([s("-c:v"), s("h264_qsv"), s("-preset"), s("medium")]);
            if br > 0 { a.extend([s("-b:v"), format!("{br}k")]) } else { a.extend([s("-global_quality"), crf.to_string()]) }
        }
        "h264_amf" => {
            a.extend([s("-c:v"), s("h264_amf"), s("-quality"), s("quality")]);
            if br > 0 { a.extend([s("-b:v"), format!("{br}k")]) } else { a.extend([s("-rc"), s("cqp"), s("-qp_i"), crf.to_string(), s("-qp_p"), crf.to_string()]) }
        }
        "h264_videotoolbox" => {
            a.extend([s("-c:v"), s("h264_videotoolbox")]);
            if br > 0 { a.extend([s("-b:v"), format!("{br}k")]) } else { a.extend([s("-q:v"), ((100.0 - 2.0 * crf as f64).clamp(30.0, 90.0) as i32).to_string()]) }
        }
        _ => a.extend([s("-c:v"), s("mpeg4"), s("-q:v"), s("3")]),
    }
    let _ = core;
    a.extend([s("-pix_fmt"), s("yuv420p"), s("-colorspace"), s("bt709"), s("-color_primaries"), s("bt709"), s("-color_trc"), s("bt709")]);
    a
}

fn job(core: &Shared, out: &str) -> Result<(), String> {
    let p = core.project.lock().unwrap().clone();
    let st = p.settings.clone();
    let dur = p.duration();

    // 1) HTML animations → cached transparent clips
    let mut html_files: HashMap<String, PathBuf> = HashMap::new();
    let htmls: Vec<Clip> = p
        .tracks
        .iter()
        .filter(|t| !t.hidden)
        .flat_map(|t| t.clips.iter())
        .filter(|c| c.kind == "html")
        .cloned()
        .collect();
    for (n, c) in htmls.iter().enumerate() {
        set_status(core, "running", 0.0, &format!("Rendering animation {}/{}…", n + 1, htmls.len()), "");
        let f = render_html_clip(core, c, &st)?;
        html_files.insert(c.id.clone(), f);
    }
    if core.cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }

    // 2) main pass (hardware first if enabled, then software)
    let g = build_graph(&p, &html_files)?;
    let script_path = core.cache_root.join("render_graph.txt");
    std::fs::create_dir_all(&core.cache_root).ok();
    std::fs::write(&script_path, &g.script).map_err(|e| e.to_string())?;

    let mut candidates: Vec<String> = vec![];
    if st.hardware && st.quality != "max" {
        if let Some(h) = core.ff.hw_encoders().first() {
            candidates.push(h.to_string());
        }
    }
    candidates.push(if core.ff.has("libx264") { "libx264".into() } else { "mpeg4".into() });

    let mut last_err = String::new();
    for enc in candidates {
        let _ = std::fs::remove_file(out);
        match run_ffmpeg(core, &g, &script_path, &st, &enc, out, dur) {
            Ok(()) => return Ok(()),
            Err(e) if e == "cancelled" => return Err(e),
            Err(e) => {
                last_err = e;
                set_status(core, "running", 0.0, &format!("{enc} unavailable, retrying…"), "");
            }
        }
    }
    Err(last_err)
}

fn run_ffmpeg(core: &Shared, g: &Graph, script: &PathBuf, st: &Settings, enc: &str, out: &str, dur: f64) -> Result<(), String> {
    let mut c = std::process::Command::new(&core.ff.ffmpeg);
    quiet(&mut c).args(["-hide_banner", "-loglevel", "error", "-y", "-progress", "pipe:1", "-nostats"]);
    for i in &g.inputs {
        c.args(i);
    }
    c.arg(if core.ff.legacy_script { "-filter_complex_script" } else { "-/filter_complex" })
        .arg(script)
        .args(["-map", "[vout]", "-map", "[aout]"]);
    c.args(video_args(core, st, enc));
    c.args(["-r", &st.fps.to_string(), "-fps_mode", "cfr"]);
    c.args(["-c:a", "aac", "-b:a", &format!("{}k", st.audio_bitrate_k), "-ar", &st.sample_rate.to_string(), "-ac", &st.channels.to_string()]);
    if st.format != "mov" {
        c.args(["-movflags", "+faststart"]);
    }
    c.args(["-t", &format!("{dur:.4}")]).arg(out);
    c.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    let mut child = c.spawn().map_err(|e| format!("could not start ffmpeg: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for l in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let label = enc.replace("h264_", "").replace("lib", "");
    loop {
        if core.cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(out);
            return Err("cancelled".into());
        }
        match rx.recv_timeout(std::time::Duration::from_millis(150)) {
            Ok(l) => {
                if let Some(v) = l.strip_prefix("out_time_us=").or_else(|| l.strip_prefix("out_time_ms=")) {
                    if let Ok(us) = v.trim().parse::<f64>() {
                        let pr = (us / 1e6 / dur).clamp(0.0, 0.999);
                        set_status(core, "running", pr, &format!("Encoding ({label})"), "");
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err_thread.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        Err(err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ffmpeg failed").to_string())
    }
}

fn render_html_clip(core: &Shared, c: &Clip, st: &Settings) -> Result<PathBuf, String> {
    let html = c.html.clone().unwrap_or_default();
    let mut h = DefaultHasher::new();
    (&html, st.width, st.height, st.fps, (c.duration * 1000.0) as i64).hash(&mut h);
    let dir = core.cache_root.join("html");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("{:016x}.mov", h.finish()));
    if file.exists() {
        return Ok(file);
    }
    let tmp = dir.join(format!("{:016x}.tmp.mov", h.finish()));
    let mut cmd = std::process::Command::new(&core.ff.ffmpeg);
    quiet(&mut cmd).args(["-hide_banner", "-loglevel", "error", "-y", "-f", "image2pipe", "-framerate", &st.fps.to_string(), "-c:v", "png", "-i", "-"]);
    cmd.args(["-c:v", "qtrle", "-pix_fmt", "argb"]).arg(&tmp);
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let key = format!("h{}", core.seq.fetch_add(1, Ordering::Relaxed));
    core.html_pipes.lock().unwrap().insert(key.clone(), child.stdin.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    core.html_done.lock().unwrap().insert(key.clone(), tx);
    let frames = (c.duration * st.fps as f64).round().max(1.0) as u64;
    let _ = core.app.emit(
        "html-render",
        json!({"key": key, "html": crate::assets::expand_tokens(core, &html), "width": st.width, "height": st.height, "fps": st.fps, "frames": frames}),
    );
    let res = loop {
        if core.cancel.load(Ordering::Relaxed) {
            break Err("cancelled".to_string());
        }
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(r) => break r,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break Err("html renderer disconnected".into()),
        }
    };
    core.html_pipes.lock().unwrap().remove(&key); // closes stdin
    core.html_done.lock().unwrap().remove(&key);
    if res.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(&tmp);
        return res.map(|_| file);
    }
    let o = child.wait_with_output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("animation encode failed: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("")));
    }
    std::fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    Ok(file)
}
