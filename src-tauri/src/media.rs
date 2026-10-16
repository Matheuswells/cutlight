use crate::model::Media;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone)]
pub struct Ffmpeg {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub encoders: Vec<String>,
    /// ffmpeg < 7 only knows -filter_complex_script (newer: -/filter_complex)
    pub legacy_script: bool,
}

pub fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn find_tool(name: &str) -> PathBuf {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    if let Ok(me) = std::env::current_exe() {
        if let Some(dir) = me.parent() {
            let p = dir.join(&exe);
            if p.exists() {
                return p;
            }
            // macOS bundle: Contents/MacOS next to the app binary is covered above
        }
    }
    PathBuf::from(exe) // fall back to PATH
}

impl Ffmpeg {
    pub fn locate() -> Ffmpeg {
        let ffmpeg = find_tool("ffmpeg");
        let ffprobe = find_tool("ffprobe");
        let mut cmd = Command::new(&ffmpeg);
        quiet(&mut cmd).args(["-hide_banner", "-encoders"]);
        let out = cmd.output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let encoders = out
            .lines()
            .filter_map(|l| l.split_whitespace().nth(1).map(|s| s.to_string()))
            .collect();
        let mut v = Command::new(&ffmpeg);
        quiet(&mut v).arg("-version");
        let ver = v.output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let major: u32 = ver
            .split("version ")
            .nth(1)
            .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(99); // git builds ("N-12345-…") are new
        Ffmpeg { ffmpeg, ffprobe, encoders, legacy_script: major < 7 }
    }

    pub fn has(&self, enc: &str) -> bool {
        self.encoders.iter().any(|e| e == enc)
    }

    pub fn cmd(&self) -> Command {
        let mut c = Command::new(&self.ffmpeg);
        quiet(&mut c).args(["-hide_banner", "-loglevel", "error", "-y"]);
        c
    }

    /// Hardware H.264 encoders worth trying, in order.
    pub fn hw_encoders(&self) -> Vec<&'static str> {
        ["h264_videotoolbox", "h264_nvenc", "h264_qsv", "h264_amf"]
            .into_iter()
            .filter(|e| self.has(e))
            .collect()
    }

    /// Software/Hardware H.264 encoder used for proxies (speed first).
    pub fn proxy_video_args(&self) -> Vec<String> {
        if self.has("libx264") {
            ["-c:v", "libx264", "-preset", "veryfast", "-crf", "28", "-g", "24", "-pix_fmt", "yuv420p"]
                .map(String::from)
                .to_vec()
        } else if let Some(h) = self.hw_encoders().first() {
            vec!["-c:v".into(), h.to_string(), "-b:v".into(), "2M".into(), "-pix_fmt".into(), "yuv420p".into()]
        } else {
            ["-c:v", "mpeg4", "-q:v", "6", "-pix_fmt", "yuv420p"].map(String::from).to_vec()
        }
    }

    pub fn probe(&self, path: &str) -> Result<Media, String> {
        let mut c = Command::new(&self.ffprobe);
        quiet(&mut c).args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format", path]);
        let out = c.output().map_err(|e| format!("ffprobe not found: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
        let streams = v["streams"].as_array().cloned().unwrap_or_default();
        let vs = streams.iter().find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1);
        let a = streams.iter().find(|s| s["codec_type"] == "audio");
        let fmt_name = v["format"]["format_name"].as_str().unwrap_or("");
        let mut duration = v["format"]["duration"].as_str().and_then(|d| d.parse::<f64>().ok()).unwrap_or(0.0);
        if duration <= 0.0 {
            duration = vs
                .and_then(|s| s["duration"].as_str())
                .and_then(|d| d.parse().ok())
                .unwrap_or(0.0);
        }
        let codec = vs.or(a).and_then(|s| s["codec_name"].as_str()).unwrap_or("").to_string();
        let is_image = vs.is_some()
            && a.is_none()
            && (fmt_name.contains("image2") || fmt_name.contains("_pipe") || fmt_name == "png_pipe" || fmt_name == "webp_pipe")
            && !codec.eq("gif");
        let (mut w, mut h) = (0u32, 0u32);
        let mut fps = 0.0;
        if let Some(s) = vs {
            w = s["width"].as_u64().unwrap_or(0) as u32;
            h = s["height"].as_u64().unwrap_or(0) as u32;
            let fr = s["avg_frame_rate"].as_str().unwrap_or("0/1");
            let mut it = fr.split('/');
            let n: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
            let d: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(1.0);
            if d > 0.0 {
                fps = n / d;
            }
            let rot = s["side_data_list"]
                .as_array()
                .and_then(|l| l.iter().find_map(|x| x["rotation"].as_f64()))
                .or_else(|| s["tags"]["rotate"].as_str().and_then(|r| r.parse().ok()))
                .unwrap_or(0.0)
                .abs() as i64;
            if rot == 90 || rot == 270 {
                std::mem::swap(&mut w, &mut h);
            }
        }
        let kind = if is_image {
            "image"
        } else if vs.is_some() {
            "video"
        } else if a.is_some() {
            "audio"
        } else {
            return Err("No audio or video streams found".into());
        };
        let name = Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        Ok(Media {
            path: path.to_string(),
            name,
            kind: kind.into(),
            duration: if is_image { 5.0 } else { duration },
            width: w,
            height: h,
            fps: if fps > 0.0 { fps } else { 30.0 },
            has_video: vs.is_some(),
            has_audio: a.is_some(),
            codec,
            size_bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            status: "working".into(),
            ..Default::default()
        })
    }
}

pub fn cache_dir_for(root: &Path, path: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    if let Ok(m) = std::fs::metadata(path) {
        m.len().hash(&mut h);
        if let Ok(t) = m.modified() {
            t.hash(&mut h);
        }
    }
    let d = root.join(format!("{:016x}", h.finish()));
    let _ = std::fs::create_dir_all(&d);
    d
}

pub struct Artifacts {
    pub proxy: String,
    pub strip: String,
    pub wave: String,
}

/// Builds proxy video/audio, a thumbnail filmstrip and a waveform. Idempotent (cached on disk).
pub fn build_artifacts(ff: &Ffmpeg, m: &Media, dir: &Path) -> Result<Artifacts, String> {
    let mut art = Artifacts { proxy: String::new(), strip: String::new(), wave: String::new() };

    // --- proxy ---
    if m.kind == "video" {
        let browser_ok = m.codec == "h264" && m.height <= 540;
        let ext_ok = matches!(
            Path::new(&m.path).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
            Some("mp4") | Some("m4v") | Some("mov")
        );
        if browser_ok && ext_ok {
            art.proxy = m.path.clone();
        } else {
            let out = dir.join("proxy.mp4");
            if !out.exists() {
                let tmp = dir.join("proxy.tmp.mp4");
                let mut c = ff.cmd();
                c.args(["-i", &m.path]);
                if m.height > 540 || m.width > 960 {
                    c.args(["-vf", "scale='min(960,iw)':-2:force_original_aspect_ratio=decrease,scale=trunc(iw/2)*2:trunc(ih/2)*2"]);
                } else {
                    c.args(["-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2"]);
                }
                c.args(ff.proxy_video_args());
                if m.has_audio {
                    c.args(["-c:a", "aac", "-b:a", "96k", "-ac", "2"]);
                } else {
                    c.arg("-an");
                }
                c.args(["-movflags", "+faststart"]).arg(&tmp);
                run(c)?;
                std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
            }
            art.proxy = out.to_string_lossy().to_string();
        }
    } else if m.kind == "audio" {
        let out = dir.join("proxy.m4a");
        if !out.exists() {
            let tmp = dir.join("proxy.tmp.m4a");
            let mut c = ff.cmd();
            c.args(["-i", &m.path, "-vn", "-c:a", "aac", "-b:a", "128k", "-ac", "2", "-f", "ipod"]).arg(&tmp);
            run(c)?;
            std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
        }
        art.proxy = out.to_string_lossy().to_string();
    } else {
        art.proxy = m.path.clone();
    }

    // --- filmstrip ---
    if m.has_video {
        let out = dir.join("strip.jpg");
        if !out.exists() {
            let n = if m.kind == "image" { 1 } else { ((m.duration / 2.0).ceil() as u32).clamp(1, 40) };
            let step = if m.kind == "image" { 1.0 } else { (m.duration / n as f64).max(0.1) };
            let mut c = ff.cmd();
            c.args(["-i", &art.proxy]).args([
                "-vf",
                &format!("fps=1/{step:.3},scale=-2:54,tile={n}x1"),
                "-frames:v",
                "1",
                "-q:v",
                "5",
            ]);
            c.arg(&out);
            if run(c).is_err() {
                let _ = std::fs::remove_file(&out);
            }
        }
        if out.exists() {
            art.strip = out.to_string_lossy().to_string();
        }
    }

    // --- waveform (20 peaks / second, u8) ---
    if m.has_audio {
        let out = dir.join("wave.bin");
        if !out.exists() {
            let mut c = ff.cmd();
            c.args(["-i", &m.path, "-vn", "-ac", "1", "-ar", "4000", "-f", "f32le", "-"]);
            c.stdout(Stdio::piped()).stderr(Stdio::null());
            let mut child = c.spawn().map_err(|e| e.to_string())?;
            let mut stdout = child.stdout.take().unwrap();
            let mut peaks: Vec<u8> = Vec::new();
            let mut buf = vec![0u8; 200 * 4]; // 200 samples @4kHz = 50 ms
            loop {
                let mut got = 0;
                while got < buf.len() {
                    match stdout.read(&mut buf[got..]) {
                        Ok(0) => break,
                        Ok(n) => got += n,
                        Err(_) => break,
                    }
                }
                if got == 0 {
                    break;
                }
                let mut mx = 0f32;
                for ch in buf[..got - got % 4].chunks_exact(4) {
                    mx = mx.max(f32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]]).abs());
                }
                peaks.push((mx.min(1.0).sqrt() * 255.0) as u8);
                if got < buf.len() {
                    break;
                }
            }
            let _ = child.wait();
            std::fs::write(&out, &peaks).map_err(|e| e.to_string())?;
        }
        art.wave = out.to_string_lossy().to_string();
    }
    Ok(art)
}

pub fn run(mut c: Command) -> Result<(), String> {
    c.stdin(Stdio::null());
    let o = c.output().map_err(|e| format!("ffmpeg failed to start: {e}"))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg error").to_string())
    }
}
