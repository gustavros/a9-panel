//! Write history: before each write, the old bytes are saved to
//! `%APPDATA%\a9-panel\history.txt` (one line per write: `seconds address hex`).
//! "Undo" writes the last line back and removes it.

use crate::device::Mouse;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("a9-panel")
}

fn file() -> PathBuf {
    dir().join("history.txt")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// A failure to save history does not block the write to the mouse, but is logged.
pub fn record(addr: u16, old: &[u8]) {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let _ = fs::create_dir_all(dir());
    let res = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file())
        .and_then(|mut f| writeln!(f, "{ts} {addr} {}", hex(old)));
    if let Err(e) = res {
        eprintln!("history: {e}");
    }
}

fn lines() -> Vec<String> {
    fs::read_to_string(file()).map(|s| s.lines().filter(|l| !l.trim().is_empty()).map(String::from).collect()).unwrap_or_default()
}

pub fn count() -> usize {
    lines().len()
}

fn parse(line: &str) -> Option<(u16, Vec<u8>)> {
    let mut it = line.split_whitespace().skip(1);
    Some((it.next()?.parse().ok()?, unhex(it.next()?)?))
}

pub fn undo_last(m: &Mouse) -> Result<(), String> {
    let mut all = lines();
    let last = all.pop().ok_or("Nothing to undo")?;
    let (addr, old) = parse(&last).ok_or("Invalid history line")?;
    m.write_raw(addr, &old)?;
    fs::write(file(), all.iter().map(|l| format!("{l}\n")).collect::<String>()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_round_trip() {
        assert_eq!(parse("1700000000 96 01010053"), Some((96, vec![1, 1, 0, 0x53])));
        assert_eq!(unhex(&hex(&[0, 255, 16])), Some(vec![0, 255, 16]));
        assert_eq!(parse("garbage"), None);
    }
}
