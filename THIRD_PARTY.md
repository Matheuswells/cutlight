# Third-party software

- **three.js** r150 (MIT) - bundled in `src-tauri/libs/three.min.js`, license in `src-tauri/libs/three.LICENSE.txt`. Used by HTML animations through `cutlight-lib://three.min.js`.
- **Lucide** icons (ISC) - inline paths in `ui/icons.js`.
- **FFmpeg** (LGPL/GPL depending on the build you bundle) - run as a sidecar process, fetched with `scripts/fetch-ffmpeg.*`.
- **whisper.cpp** via `whisper-rs` (MIT/Unlicense) - speech to text. Models are downloaded separately.
