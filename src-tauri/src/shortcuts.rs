//! First run: put Cutlight in the system menu and on the desktop.
//!  * Windows: Start menu + Desktop `.lnk`
//!  * macOS: Desktop link + ~/Applications entry (shows in Launchpad / Spotlight)
//! Installers (NSIS/MSI/DMG) already do this for installed copies; this covers portable and dev runs.
//! Safe to run repeatedly: shortcuts are overwritten, nothing else is touched.

use crate::core::Shared;
use serde_json::{json, Value};

#[cfg(windows)]
pub fn create() -> Result<Value, String> {
    use crate::media::quiet;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = r#"
$ErrorActionPreference='Stop'
$exe=$env:CUTLIGHT_EXE
$w=New-Object -ComObject WScript.Shell
$made=@()
foreach($d in @([Environment]::GetFolderPath('Desktop'), (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'))){
  if(-not (Test-Path $d)){ continue }
  $p=Join-Path $d 'Cutlight.lnk'
  $l=$w.CreateShortcut($p)
  $l.TargetPath=$exe
  $l.WorkingDirectory=(Split-Path $exe)
  $l.IconLocation="$exe,0"
  $l.Description='Cutlight video editor'
  $l.Save()
  $made+=$p
}
$made -join '|'
"#;
    let mut c = std::process::Command::new("powershell");
    quiet(&mut c)
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .env("CUTLIGHT_EXE", &exe);
    let out = c.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let made: Vec<String> = String::from_utf8_lossy(&out.stdout).trim().split('|').filter(|s| !s.is_empty()).map(String::from).collect();
    Ok(json!({ "created": made }))
}

#[cfg(target_os = "macos")]
pub fn create() -> Result<Value, String> {
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // .../Cutlight.app/Contents/MacOS/cutlight -> .../Cutlight.app
    let bundle = exe.ancestors().nth(3).filter(|p| p.extension().map(|e| e == "app").unwrap_or(false)).map(PathBuf::from);
    let Some(bundle) = bundle else { return Err("not running from a .app bundle (shortcuts are created for the installed app)".into()) };
    let home = PathBuf::from(std::env::var("HOME").map_err(|e| e.to_string())?);
    let mut made = vec![];
    let mut link = |dir: PathBuf, name: &str| {
        if !dir.exists() {
            let _ = std::fs::create_dir_all(&dir);
        }
        let p = dir.join(name);
        let _ = std::fs::remove_file(&p); // replace an old link; never touches a real app
        if p.exists() && !p.is_symlink() {
            return; // a real copy is already there
        }
        if symlink(&bundle, &p).is_ok() {
            made.push(p.to_string_lossy().to_string());
        }
    };
    link(home.join("Desktop"), "Cutlight");
    if !bundle.starts_with("/Applications") && !bundle.starts_with(home.join("Applications")) {
        link(home.join("Applications"), "Cutlight.app");
    }
    Ok(json!({ "created": made }))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn create() -> Result<Value, String> {
    use std::path::PathBuf;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let home = PathBuf::from(std::env::var("HOME").map_err(|e| e.to_string())?);
    let dir = home.join(".local/share/applications");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join("cutlight.desktop");
    std::fs::write(&p, format!("[Desktop Entry]\nType=Application\nName=Cutlight\nComment=Video editor\nExec=\"{}\"\nTerminal=false\nCategories=AudioVideo;Video;\n", exe.display()))
        .map_err(|e| e.to_string())?;
    Ok(json!({ "created": [p.to_string_lossy()] }))
}

/// Run once, on the first launch, in the background.
pub fn first_run(core: &Shared) {
    let marker = crate::store::app_dir(core).join("shortcuts_done");
    if marker.exists() {
        return;
    }
    let core = core.clone();
    std::thread::spawn(move || match create() {
        Ok(v) => {
            let _ = std::fs::write(&marker, v.to_string());
            std::thread::sleep(std::time::Duration::from_secs(4)); // let the window finish loading
            core.toast("Shortcuts created on the desktop and in the app menu");
        }
        Err(_) => {
            // not fatal (e.g. no Desktop folder); try again next launch
        }
    });
}
