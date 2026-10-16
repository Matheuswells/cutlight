//! Assets for animations: Pixabay images/illustrations/vectors/videos, icons (Lucide, Tabler, Simple Icons),
//! and a safe way to embed local images into self-contained HTML.
//!
//! Embedding uses tokens (`cutlight-asset://<file>`) in the HTML. The preview resolves them to local files and
//! the renderer expands them to data: URIs, so Claude never has to carry megabytes of base64 in its context.

use crate::core::Shared;
use crate::{ops, settings};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

pub const TOKEN: &str = "cutlight-asset://";
const ALLOWED_HOSTS: &[&str] = &["pixabay.com", "cdn.pixabay.com", "unpkg.com", "cdn.jsdelivr.net", "cdn.simpleicons.org"];

pub fn assets_dir(core: &Shared) -> PathBuf {
    let d = crate::store::app_dir(core).join("assets");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn host_ok(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").split('@').last().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    ALLOWED_HOSTS.iter().any(|a| host == *a || host.ends_with(&format!(".{a}")))
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(45)).user_agent("Cutlight/0.1").build()
}

fn http_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(c, _) => format!("server answered HTTP {c}"),
        other => other.to_string(),
    }
}

fn get_bytes(url: &str, max: u64) -> Result<Vec<u8>, String> {
    if !host_ok(url) {
        return Err("download blocked: only https URLs from Pixabay and the icon CDNs (unpkg, jsDelivr, simpleicons) are allowed".into());
    }
    let resp = agent().get(url).call().map_err(http_err)?;
    let mut buf = Vec::new();
    resp.into_reader().take(max + 1).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    if buf.len() as u64 > max {
        return Err(format!("file is larger than {} MB", max / 1_000_000));
    }
    Ok(buf)
}

fn ext_of(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let e = path.rsplit('.').next().unwrap_or("").to_lowercase();
    if ["jpg", "jpeg", "png", "webp", "gif", "svg", "mp4", "webm", "mov", "mp3", "wav", "woff2"].contains(&e.as_str()) { e } else { "bin".into() }
}

fn token_for(file: &std::path::Path) -> String {
    format!("{TOKEN}{}", file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
}

// ---------------------------------------------------------------------------------------------
// Pixabay
// ---------------------------------------------------------------------------------------------

pub fn pixabay_status(core: &Shared) -> Value {
    let ok = !settings::load(core).pixabay_key.trim().is_empty();
    json!({"configured": ok, "how": if ok { "ready" } else { "Ask the user to paste a free Pixabay API key in Cutlight: Claude tab > Pixabay (get one at pixabay.com/api/docs). Never request it in chat." }})
}

pub fn pixabay_search(core: &Shared, a: &Value) -> Result<Value, String> {
    let key = settings::load(core).pixabay_key;
    if key.trim().is_empty() {
        return Err("Pixabay API key not set. Ask the user to add it in Cutlight (Claude tab > Pixabay). Free at pixabay.com/api/docs".into());
    }
    let q = a["query"].as_str().ok_or("query required")?;
    let kind = a["kind"].as_str().unwrap_or("photo");
    let per_page = a["perPage"].as_u64().unwrap_or(12).clamp(3, 30).to_string();
    let video = kind == "video";
    let url = if video { "https://pixabay.com/api/videos/" } else { "https://pixabay.com/api/" };
    let mut req = agent()
        .get(url)
        .query("key", key.trim())
        .query("q", q)
        .query("per_page", &per_page)
        .query("safesearch", "true");
    if !video {
        req = req.query("image_type", match kind { "illustration" => "illustration", "vector" => "vector", _ => "photo" });
        if let Some(o) = a["orientation"].as_str() {
            req = req.query("orientation", o);
        }
    }
    if let Some(w) = a["minWidth"].as_u64() {
        req = req.query("min_width", &w.to_string());
    }
    let v: Value = req.call().map_err(http_err)?.into_json().map_err(|e| e.to_string())?;
    let hits: Vec<Value> = v["hits"]
        .as_array()
        .map(|l| {
            l.iter()
                .map(|h| {
                    if video {
                        let vid = &h["videos"];
                        let best = ["large", "medium", "small"].iter().map(|k| &vid[*k]).find(|x| x["url"].as_str().map(|u| !u.is_empty()).unwrap_or(false));
                        json!({"id": h["id"], "tags": h["tags"], "user": h["user"], "pageUrl": h["pageURL"], "duration": h["duration"],
                               "url": best.map(|b| b["url"].clone()), "width": best.map(|b| b["width"].clone()), "height": best.map(|b| b["height"].clone())})
                    } else {
                        json!({"id": h["id"], "tags": h["tags"], "user": h["user"], "pageUrl": h["pageURL"], "type": h["type"],
                               "url": h["largeImageURL"], "previewUrl": h["webformatURL"], "vectorUrl": h["vectorURL"],
                               "width": h["imageWidth"], "height": h["imageHeight"]})
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({"total": v["totalHits"], "hits": hits, "license": "Pixabay Content License: free for commercial use, no attribution required. Download, don't hotlink."}))
}

/// Download an allowed URL into the assets folder. Images/videos/audio are also imported into the media pool.
pub fn import_asset(core: &Shared, a: &Value) -> Result<Value, String> {
    let url = a["url"].as_str().ok_or("url required")?;
    let ext = ext_of(url);
    if ext == "bin" {
        return Err("unsupported file type (jpg, png, webp, gif, svg, mp4, webm, mov, mp3, wav, woff2)".into());
    }
    let max = if matches!(ext.as_str(), "mp4" | "webm" | "mov") { 120_000_000 } else { 30_000_000 };
    let bytes = get_bytes(url, max)?;
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    let file = assets_dir(core).join(format!("{:016x}.{ext}", h.finish()));
    std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
    let path = file.to_string_lossy().to_string();
    let mut out = json!({"path": path, "token": token_for(&file), "bytes": bytes.len()});
    if !matches!(ext.as_str(), "svg" | "woff2") {
        let r = ops::run(core, "import_media", &json!({"path": path}))?;
        out["mediaId"] = r["mediaId"].clone();
    }
    out["usage"] = json!(format!("In HTML use <img src=\"{}\"> (or CSS url()). Cutlight embeds it at render time.", token_for(&file)));
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------------------------

fn slug(s: &str) -> String {
    s.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>().trim_matches('-').to_string()
}

pub fn get_icon(a: &Value) -> Result<Value, String> {
    let set = a["set"].as_str().unwrap_or("lucide");
    let name = slug(a["name"].as_str().ok_or("name required")?);
    if name.is_empty() {
        return Err("name required".into());
    }
    let url = match set {
        "lucide" => format!("https://unpkg.com/lucide-static@latest/icons/{name}.svg"),
        "tabler" => format!("https://unpkg.com/@tabler/icons@latest/icons/outline/{name}.svg"),
        "simple-icons" | "brands" => format!("https://unpkg.com/simple-icons@latest/icons/{}.svg", name.replace('-', "")),
        _ => return Err("set must be lucide | tabler | simple-icons".into()),
    };
    let bytes = get_bytes(&url, 200_000).map_err(|e| {
        if e.contains("404") {
            format!("icon '{name}' not found in {set}. Try another name (lucide/tabler use kebab-case like 'arrow-right'; simple-icons uses the brand slug like 'github', 'openai', 'youtube').")
        } else {
            e
        }
    })?;
    let mut svg = String::from_utf8_lossy(&bytes).to_string();
    // drop license comments / XML prolog before the root element
    if let Some(i) = svg.find("<svg") {
        svg = svg[i..].to_string();
    }
    let color = a["color"].as_str().unwrap_or("currentColor");
    let size = a["size"].as_u64().unwrap_or(0);
    if set == "simple-icons" || set == "brands" {
        svg = svg.replacen("<svg ", &format!("<svg fill=\"{color}\" "), 1);
    } else {
        svg = svg.replace("currentColor", color);
        if let Some(w) = a["strokeWidth"].as_f64() {
            svg = svg.replace("stroke-width=\"2\"", &format!("stroke-width=\"{w}\""));
        }
    }
    if size > 0 {
        // drop the fixed width/height then set ours on the root element
        let (head, rest) = svg.split_once('>').map(|(h, r)| (h.to_string(), r.to_string())).unwrap_or((svg.clone(), String::new()));
        let mut head = head;
        for attr in ["width", "height"] {
            if let Some(i) = head.find(&format!(" {attr}=\"")) {
                if let Some(j) = head[i + 2 + attr.len()..].find('"') {
                    head.replace_range(i..i + 2 + attr.len() + j + 1, "");
                }
            }
        }
        svg = format!("{head} width=\"{size}\" height=\"{size}\">{rest}");
    }
    Ok(json!({"svg": svg.trim(), "set": set, "name": name}))
}

// ---------------------------------------------------------------------------------------------
// Embedding
// ---------------------------------------------------------------------------------------------

/// Make a local image/svg/font usable inside HTML: copies (downscaling big rasters) into the assets folder, returns a token.
pub fn asset_token(core: &Shared, a: &Value) -> Result<Value, String> {
    let src: PathBuf = if let Some(u) = a["url"].as_str() {
        let r = import_asset(core, &json!({"url": u}))?;
        PathBuf::from(r["path"].as_str().unwrap_or_default())
    } else {
        PathBuf::from(a["path"].as_str().ok_or("path or url required")?)
    };
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !src.exists() {
        return Err("file not found".into());
    }
    if src.starts_with(assets_dir(core)) && ext != "png" && ext != "jpg" && ext != "jpeg" && ext != "webp" {
        return Ok(json!({"token": token_for(&src)}));
    }
    let bytes = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
    let raster = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp");
    let max_w = a["maxWidth"].as_u64().unwrap_or(1600);
    let mut h = DefaultHasher::new();
    (src.to_string_lossy().to_string(), bytes, max_w).hash(&mut h);
    if raster {
        let out_ext = if ext == "png" { "png" } else { "jpg" }; // keep transparency for png
        let out = assets_dir(core).join(format!("{:016x}.{out_ext}", h.finish()));
        if !out.exists() {
            let mut c = core.ff.cmd();
            c.arg("-i").arg(&src).args(["-vf", &format!("scale='min({max_w},iw)':-2"), "-q:v", "4"]).arg(&out);
            crate::media::run(c)?;
        }
        Ok(json!({"token": token_for(&out), "bytes": std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)}))
    } else {
        if bytes > 8_000_000 {
            return Err("file too large to embed".into());
        }
        let out = assets_dir(core).join(format!("{:016x}.{ext}", h.finish()));
        std::fs::copy(&src, &out).map_err(|e| e.to_string())?;
        Ok(json!({"token": token_for(&out)}))
    }
}

/// Render time: replace every token by a data: URI so the HTML is fully self-contained.
pub fn expand_tokens(core: &Shared, html: &str) -> String {
    if !html.contains(TOKEN) {
        return html.to_string();
    }
    let dir = assets_dir(core);
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(TOKEN) {
        out.push_str(&rest[..i]);
        let after = &rest[i + TOKEN.len()..];
        let end = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')).unwrap_or(after.len());
        let name = &after[..end];
        let path = dir.join(name);
        let mime = match name.rsplit('.').next().unwrap_or("") {
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "woff2" => "font/woff2",
            _ => "application/octet-stream",
        };
        match std::fs::read(&path) {
            Ok(b) => out.push_str(&format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(b))),
            Err(_) => out.push_str(&format!("{TOKEN}{name}")),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}
