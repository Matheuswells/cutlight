# Cutlight

A lightweight, AI-first video editor built with Tauri (Rust + webview). Windows and macOS.

- **Timeline editing**: import, cut, join, trim, move, speed, transitions, multiple video/audio tracks, detach / delete audio or video, organize (close gaps, tidy).
- **Fast preview**: low-res proxies, thumbnail filmstrips and waveforms are built in the background.
- **Export**: Instagram (Reel, Story, 4:5, 1:1, 1.91:1), TikTok, Shorts, YouTube presets; quality, audio bitrate / sample rate, loudness normalisation, GPU encoding. One optimised ffmpeg pass.
- **HTML animations**: Claude (or you) writes self-contained HTML; it previews live and is rendered frame-exact into the video.
- **Claude control (MCP)**: a local MCP server exposes every editor function, screenshots and UI control. Cutlight shows the exact connect command in the *Claude* tab, e.g.

<img width="1919" height="1079" alt="Captura de tela 2026-10-16 192641" src="https://github.com/user-attachments/assets/74f7bec1-9b1f-4937-ad5c-cd8fb354bb9c" />
<img width="1919" height="1079" alt="Captura de tela 2026-10-16 192715" src="https://github.com/user-attachments/assets/4689f29b-167a-4eaf-ae7e-6950e52d65a2" />


  ```
  claude mcp add --transport http cutlight http://127.0.0.1:47321/mcp
  ```
- **Script + Whisper**: paste your script; the Whisper transcript is compared against it so Claude can fix mistakes.

## Develop

```
npm install
pwsh scripts/fetch-ffmpeg.ps1      # Windows (or scripts/fetch-ffmpeg.sh on macOS)
npx tauri dev
```

Release: `npx tauri build` — ffmpeg/ffprobe are bundled as sidecars, nothing else to install.

## HTML animation contract

Self-contained document, transparent background, sized with `vw`/`vh`. Animate with CSS animations / Web Animations,
or define `window.__seek(t)` (seconds from clip start). Inline images as `data:` URIs.

## Licensing note

Bundling an ffmpeg build with libx264 makes the distributed app GPL. Use an LGPL ffmpeg (hardware encoders only) if you need a permissive licence.
