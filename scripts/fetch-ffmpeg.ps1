# Downloads a slim ffmpeg + ffprobe (Windows x64) into src-tauri/binaries with the names Tauri expects.
# Uses the "essentials" GPL build from gyan.dev (includes libx264, NVENC/QSV/AMF).
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$dest = Join-Path $root 'src-tauri\binaries'
$tmp = Join-Path $env:TEMP 'cutlight-ffmpeg'
New-Item -ItemType Directory -Force $tmp, $dest | Out-Null
$zip = Join-Path $tmp 'ffmpeg.zip'
Invoke-WebRequest 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip' -OutFile $zip
Expand-Archive $zip -DestinationPath $tmp -Force
$bin = Get-ChildItem $tmp -Recurse -Filter ffmpeg.exe | Select-Object -First 1
Copy-Item $bin.FullName (Join-Path $dest 'ffmpeg-x86_64-pc-windows-msvc.exe') -Force
Copy-Item (Join-Path $bin.DirectoryName 'ffprobe.exe') (Join-Path $dest 'ffprobe-x86_64-pc-windows-msvc.exe') -Force
Write-Host 'ffmpeg ready in src-tauri/binaries'
