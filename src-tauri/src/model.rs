use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub preset: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// draft | good | high | max
    pub quality: String,
    /// 0 = quality based (CRF), otherwise target bitrate in kbps
    pub video_bitrate_k: u32,
    pub audio_bitrate_k: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub normalize_audio: bool,
    pub hardware: bool,
    /// mp4 | mov
    pub format: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            preset: "reel".into(),
            width: 1080,
            height: 1920,
            fps: 30,
            quality: "good".into(),
            video_bitrate_k: 0,
            audio_bitrate_k: 192,
            sample_rate: 48000,
            channels: 2,
            normalize_audio: false,
            hardware: true,
            format: "mp4".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Media {
    pub id: String,
    pub path: String,
    pub name: String,
    /// video | audio | image
    pub kind: String,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_video: bool,
    pub has_audio: bool,
    pub codec: String,
    pub size_bytes: u64,
    /// probing | working | ready | error
    pub status: String,
    pub error: String,
    pub proxy: String,
    pub strip: String,
    pub wave: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    /// dissolve | fade-black | slide-left | slide-right | slide-up | slide-down
    pub kind: String,
    pub duration: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Clip {
    pub id: String,
    /// video | audio | image | html
    pub kind: String,
    pub media_id: Option<String>,
    pub label: String,
    pub start: f64,
    pub in_point: f64,
    pub duration: f64,
    pub speed: f64,
    pub volume: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    /// audio enabled (video clips carry their own audio)
    pub audio: bool,
    /// fit | fill
    pub fit: String,
    pub opacity: f64,
    pub scale: f64,
    pub x: f64,
    pub y: f64,
    pub transition: Option<Transition>,
    pub html: Option<String>,
}

impl Default for Clip {
    fn default() -> Self {
        Clip {
            id: String::new(),
            kind: "video".into(),
            media_id: None,
            label: String::new(),
            start: 0.0,
            in_point: 0.0,
            duration: 1.0,
            speed: 1.0,
            volume: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            audio: true,
            fit: "fit".into(),
            opacity: 1.0,
            scale: 1.0,
            x: 0.5,
            y: 0.5,
            transition: None,
            html: None,
        }
    }
}

impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Track {
    pub id: String,
    /// video | audio
    pub kind: String,
    pub name: String,
    pub muted: bool,
    pub hidden: bool,
    pub locked: bool,
    pub clips: Vec<Clip>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Segment {
    pub id: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub settings: Settings,
    pub media: Vec<Media>,
    /// Top of the list = top layer in the UI
    pub tracks: Vec<Track>,
    /// The script the creator pasted so Claude can fix transcription errors
    pub script: String,
    pub transcript: Vec<Segment>,
    /// The look of the video: a DESIGN.md (colours, fonts, spacing, shapes, do's and don'ts)
    pub design: String,
    /// Selected style + tone preset id (see guide::STYLES)
    pub style: String,
    /// What the video is about (topic, audience, platform, tone, length, CTA); essential for from-scratch videos
    pub brief: String,
    /// Notes pinned to moments of the video ("fix this at 0:42")
    pub notes: Vec<Note>,
    pub next_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Note {
    pub id: String,
    pub t: f64,
    pub text: String,
    pub done: bool,
}

impl Default for Project {
    fn default() -> Self {
        let mk = |id: &str, kind: &str, name: &str| Track {
            id: id.into(),
            kind: kind.into(),
            name: name.into(),
            ..Default::default()
        };
        Project {
            version: 1,
            name: "Untitled".into(),
            settings: Settings::default(),
            media: vec![],
            tracks: vec![
                mk("t1", "video", "V2"),
                mk("t2", "video", "V1"),
                mk("t3", "audio", "A1"),
                mk("t4", "audio", "A2"),
            ],
            script: String::new(),
            transcript: vec![],
            design: String::new(),
            style: "modern".into(),
            brief: String::new(),
            notes: vec![],
            next_id: 10,
        }
    }
}

impl Project {
    pub fn new_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{}{}", prefix, self.next_id)
    }

    pub fn duration(&self) -> f64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.end())
            .fold(0.0, f64::max)
    }

    pub fn media(&self, id: &str) -> Option<&Media> {
        self.media.iter().find(|m| m.id == id)
    }
}

pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub w: u32,
    pub h: u32,
    pub fps: u32,
    pub note: &'static str,
    pub max_s: u32,
}

pub const PRESETS: &[Preset] = &[
    Preset { id: "reel", label: "Reel", group: "Instagram", w: 1080, h: 1920, fps: 30, note: "9:16 · up to 3 min", max_s: 180 },
    Preset { id: "story", label: "Story", group: "Instagram", w: 1080, h: 1920, fps: 30, note: "9:16 · 60 s per card", max_s: 60 },
    Preset { id: "feed-portrait", label: "Feed portrait", group: "Instagram", w: 1080, h: 1350, fps: 30, note: "4:5", max_s: 900 },
    Preset { id: "feed-square", label: "Feed square", group: "Instagram", w: 1080, h: 1080, fps: 30, note: "1:1", max_s: 900 },
    Preset { id: "feed-landscape", label: "Feed landscape", group: "Instagram", w: 1080, h: 566, fps: 30, note: "1.91:1", max_s: 900 },
    Preset { id: "tiktok", label: "TikTok", group: "Social", w: 1080, h: 1920, fps: 30, note: "9:16", max_s: 600 },
    Preset { id: "yt-shorts", label: "YouTube Shorts", group: "Social", w: 1080, h: 1920, fps: 30, note: "9:16 · up to 3 min", max_s: 180 },
    Preset { id: "yt-1080", label: "YouTube 1080p", group: "Video", w: 1920, h: 1080, fps: 30, note: "16:9", max_s: 0 },
    Preset { id: "yt-4k", label: "YouTube 4K", group: "Video", w: 3840, h: 2160, fps: 30, note: "16:9", max_s: 0 },
    Preset { id: "hd-720", label: "HD 720p", group: "Video", w: 1280, h: 720, fps: 30, note: "16:9", max_s: 0 },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}
