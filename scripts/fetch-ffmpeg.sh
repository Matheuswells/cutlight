#!/usr/bin/env bash
# macOS: downloads static ffmpeg + ffprobe (evermeet.cx builds) into src-tauri/binaries with Tauri's target-triple names.
# Apple Silicon and Intel both need a copy: aarch64-apple-darwin and x86_64-apple-darwin
# (the evermeet build is x86_64 and runs on Apple Silicon via Rosetta; swap in a native arm64 build for best speed).
set -euo pipefail
cd "$(dirname "$0")/../src-tauri/binaries"
for tool in ffmpeg ffprobe; do
  curl -L "https://evermeet.cx/ffmpeg/getrelease/$tool/zip" -o "$tool.zip"
  unzip -o "$tool.zip" && rm "$tool.zip"
  cp "$tool" "$tool-x86_64-apple-darwin"
  cp "$tool" "$tool-aarch64-apple-darwin"
  chmod +x "$tool-"*
  rm "$tool"
done
echo "ffmpeg ready in src-tauri/binaries"
