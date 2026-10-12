//! App preferences (not the mouse's): `%APPDATA%\a9-panel\app.txt` with `key=value` lines.

use std::fs;
use std::os::windows::process::CommandExt;
use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "A9-Panel";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone)]
pub struct Settings {
    pub start_in_tray: bool,
    /// "en", "pt" or empty (follow Windows).
    pub language: String,
    pub game_mode: bool,
    /// Comma separated program names.
    pub games: String,
    pub game_rate_hz: u32,
    pub game_sensor_mode: u8,
    /// (polling Hz, sensor mode) to put back when the game closes; set while game mode is active.
    pub restore: Option<(u32, u8)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_in_tray: false,
            language: String::new(),
            game_mode: false,
            games: crate::gamemode::DEFAULT_GAMES.into(),
            game_rate_hz: 8000,
            game_sensor_mode: 2,
            restore: None,
        }
    }
}

fn path() -> std::path::PathBuf {
    crate::backup::dir().join("app.txt")
}

pub fn load() -> Settings {
    let txt = fs::read_to_string(path()).unwrap_or_default();
    let get = |k: &str| txt.lines().find_map(|l| l.trim().strip_prefix(k)?.strip_prefix('=').map(String::from));
    let d = Settings::default();
    Settings {
        start_in_tray: get("start_in_tray").as_deref() == Some("1"),
        language: get("language").unwrap_or_default(),
        game_mode: get("game_mode").as_deref() == Some("1"),
        games: get("games").unwrap_or(d.games),
        game_rate_hz: get("game_rate").and_then(|v| v.parse().ok()).unwrap_or(d.game_rate_hz),
        game_sensor_mode: get("game_sensor").and_then(|v| v.parse().ok()).unwrap_or(d.game_sensor_mode),
        restore: get("restore").and_then(|v| {
            let (r, m) = v.split_once(',')?;
            Some((r.parse().ok()?, m.parse().ok()?))
        }),
    }
}

pub fn save(s: &Settings) -> Result<(), String> {
    fs::create_dir_all(crate::backup::dir()).map_err(|e| e.to_string())?;
    let restore = s.restore.map_or(String::new(), |(r, m)| format!("{r},{m}"));
    let txt = format!(
        "start_in_tray={}\nlanguage={}\ngame_mode={}\ngames={}\ngame_rate={}\ngame_sensor={}\nrestore={}\n",
        s.start_in_tray as u8, s.language, s.game_mode as u8, s.games, s.game_rate_hz, s.game_sensor_mode, restore
    );
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
