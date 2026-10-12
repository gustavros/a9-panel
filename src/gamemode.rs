//! Automatic game mode: finds out whether one of the configured games is running.
//! Only reads the process list (like Task Manager); it never touches the game.

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

pub const DEFAULT_GAMES: &str = "VALORANT-Win64-Shipping.exe";

/// "VALORANT-Win64-Shipping.exe, cs2" -> ["VALORANT-Win64-Shipping.exe", "cs2.exe"]
pub fn parse_list(s: &str) -> Vec<String> {
    s.split([',', ';', '\n'])
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(|g| if g.to_ascii_lowercase().ends_with(".exe") { g.to_string() } else { format!("{g}.exe") })
        .collect()
}

/// First configured game that is running right now, if any.
pub fn running(games: &[String]) -> Option<String> {
    if games.is_empty() {
        return None;
    }
    // SAFETY: plain Win32 snapshot walk; the entry struct is zeroed and sized as the API requires,
    // and the handle is closed before returning.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        if Process32FirstW(snap, &mut e) != 0 {
            loop {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..len]);
                if let Some(g) = games.iter().find(|g| g.eq_ignore_ascii_case(&name)) {
                    found = Some(g.clone());
                    break;
                }
                if Process32NextW(snap, &mut e) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_game_list() {
        assert_eq!(parse_list(" VALORANT-Win64-Shipping.exe, cs2 ;; "), vec!["VALORANT-Win64-Shipping.exe", "cs2.exe"]);
        assert!(parse_list("").is_empty());
    }

    #[test]
    fn finds_a_running_process() {
        // the test runner itself is always running
        let me = std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(running(&[me.to_uppercase()]), Some(me.to_uppercase()));
        assert_eq!(running(&["surely-not-running-12345.exe".into()]), None);
    }
}
