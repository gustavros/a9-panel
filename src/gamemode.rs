//! Automatic game mode: finds out whether one of the configured games is running.
//! Only reads the process list (like Task Manager); it never touches the game.

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

/// A program with a visible window, and its icon as RGBA pixels (width, height, bytes).
pub struct Program {
    pub name: String,
    pub icon: Option<(u32, u32, Vec<u8>)>,
}

/// Large icon of an executable, as RGBA.
fn icon_rgba(path: &[u16]) -> Option<(u32, u32, Vec<u8>)> {
    use std::ffi::c_void;
    use windows_sys::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS,
    };
    use windows_sys::Win32::UI::Shell::ExtractIconExW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};
    // SAFETY: every struct is zeroed and sized as the API wants; each handle is released before returning.
    unsafe {
        let mut hicon = std::ptr::null_mut();
        if ExtractIconExW(path.as_ptr(), 0, &mut hicon, std::ptr::null_mut(), 1) < 1 || hicon.is_null() {
            return None;
        }
        let mut info: ICONINFO = std::mem::zeroed();
        let mut out = None;
        if GetIconInfo(hicon, &mut info) != 0 {
            let mut bm: BITMAP = std::mem::zeroed();
            if GetObjectW(info.hbmColor, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut c_void) != 0 {
                let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
                let mut bi: BITMAPINFO = std::mem::zeroed();
                bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                bi.bmiHeader.biWidth = w as i32;
                bi.bmiHeader.biHeight = -(h as i32); // top-down
                bi.bmiHeader.biPlanes = 1;
                bi.bmiHeader.biBitCount = 32;
                bi.bmiHeader.biCompression = BI_RGB;
                let mut px = vec![0u8; (w * h * 4) as usize];
                let dc = GetDC(std::ptr::null_mut());
                let n = GetDIBits(dc, info.hbmColor, 0, h, px.as_mut_ptr() as *mut c_void, &mut bi, DIB_RGB_COLORS);
                ReleaseDC(std::ptr::null_mut(), dc);
                if n != 0 {
                    // BGRA -> RGBA; old icons without alpha come back fully transparent
                    let no_alpha = px.chunks(4).all(|p| p[3] == 0);
                    for p in px.chunks_mut(4) {
                        p.swap(0, 2);
                        if no_alpha {
                            p[3] = 255;
                        }
                    }
                    out = Some((w, h, px));
                }
            }
            DeleteObject(info.hbmColor);
            DeleteObject(info.hbmMask);
        }
        DestroyIcon(hicon);
        out
    }
}

/// Programs that have a visible window right now (what the user can see), without duplicates.
pub fn open_programs() -> Vec<Program> {
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
    };
    unsafe extern "system" fn each(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
        if IsWindowVisible(hwnd) != 0 && GetWindowTextLengthW(hwnd) > 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            (*(lparam as *mut Vec<u32>)).push(pid);
        }
        1
    }
    let mut pids: Vec<u32> = Vec::new();
    // SAFETY: the callback only pushes into the Vec we pass for the duration of the call.
    unsafe { EnumWindows(Some(each), &mut pids as *mut _ as isize) };
    let me = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    let mut found: Vec<Program> = Vec::new();
    // SAFETY: same snapshot walk as `running`.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return found;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e) != 0;
        while ok {
            if pids.contains(&e.th32ProcessID) {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..len]);
                let low = name.to_lowercase();
                // shell pieces are never games
                let shell = ["explorer.exe", "applicationframehost.exe", "textinputhost.exe", "systemsettings.exe"];
                if Some(&low) != me.as_ref()
                    && !shell.contains(&low.as_str())
                    && !found.iter().any(|p| p.name.eq_ignore_ascii_case(&name))
                {
                    let mut icon = None;
                    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, e.th32ProcessID);
                    if !h.is_null() {
                        let mut buf = [0u16; 1024];
                        let mut n = buf.len() as u32;
                        if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) != 0 {
                            buf[n as usize] = 0;
                            icon = icon_rgba(&buf[..=n as usize]);
                        }
                        CloseHandle(h);
                    }
                    found.push(Program { name, icon });
                }
            }
            ok = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    found.sort_by_key(|p| p.name.to_lowercase());
    found
}

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
    fn open_programs_has_no_duplicates() {
        let v = open_programs();
        let mut u: Vec<String> = v.iter().map(|p| p.name.to_lowercase()).collect();
        u.dedup();
        assert_eq!(u.len(), v.len());
    }

    #[test]
    fn finds_a_running_process() {
        // the test runner itself is always running
        let me = std::env::current_exe().unwrap().file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(running(&[me.to_uppercase()]), Some(me.to_uppercase()));
        assert_eq!(running(&["surely-not-running-12345.exe".into()]), None);
    }
}
