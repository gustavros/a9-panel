//! App preferences (not the mouse's): `%APPDATA%\a9-panel\app.txt` with `key=value` lines.

use std::fs;
use std::os::windows::process::CommandExt;
use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "A9-Panel";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Default)]
pub struct Settings {
    pub start_in_tray: bool,
    /// "en", "pt" or empty (follow Windows).
    pub language: String,
}

fn path() -> std::path::PathBuf {
    crate::backup::dir().join("app.txt")
}

pub fn load() -> Settings {
    let txt = fs::read_to_string(path()).unwrap_or_default();
    let get = |k: &str| txt.lines().find_map(|l| l.trim().strip_prefix(k)?.strip_prefix('=').map(String::from));
    Settings {
        start_in_tray: get("start_in_tray").as_deref() == Some("1"),
        language: get("language").unwrap_or_default(),
    }
}

pub fn save(s: &Settings) -> Result<(), String> {
    fs::create_dir_all(crate::backup::dir()).map_err(|e| e.to_string())?;
    let txt = format!("start_in_tray={}\nlanguage={}\n", s.start_in_tray as u8, s.language);
    fs::write(path(), txt).map_err(|e| e.to_string())
}

fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output()
}

pub fn autostart_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", RUN_NAME]).is_ok_and(|o| o.status.success())
}

/// Turns "start with Windows" on or off (current user only).
pub fn set_autostart(on: bool) -> Result<(), String> {
    let out = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let cmd = format!("\"{}\" --tray", exe.display());
        reg(&["add", RUN_KEY, "/v", RUN_NAME, "/t", "REG_SZ", "/d", &cmd, "/f"])
    } else {
        reg(&["delete", RUN_KEY, "/v", RUN_NAME, "/f"])
    }
    .map_err(|e| e.to_string())?;
    if out.status.success() || !on {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}
