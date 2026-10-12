//! Third-party UI assets (the mouse picture and the OPPO Sans font used by ATK Hub).
//! They are downloaded from the ATK Hub CDN on first run and cached in `%APPDATA%\a9-panel\assets`,
//! so neither the repository nor the `.exe` ships them.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

const MOUSE_URL: &str = "https://bpcdn.atkgear.com/device/ossAssets/a9nearlink/rose-red.webp";
const FONT_URL: &str = "https://bpcdn.atkgear.com/hub-v3/production/3.2.27/static/OPPO-Sans-4.0-CLWyZFQr.woff";

fn dir() -> PathBuf {
    crate::backup::dir().join("assets")
}

fn mouse_path() -> PathBuf {
    dir().join("mouse.webp")
}

fn font_path() -> PathBuf {
    dir().join("OPPOSans.ttf")
}

/// Downloads with the `curl.exe` that ships with Windows 10+ (no HTTP crate needed).
fn download(url: &str, to: &Path) -> Result<(), String> {
    let out = std::process::Command::new("curl.exe")
        .args(["-fsSL", "--max-time", "60", "-o"])
        .arg(to)
        .arg(url)
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Fetches whatever is missing. Safe to call every start: it does nothing once the cache exists.
pub fn ensure() -> Result<(), String> {
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    if !mouse_path().exists() {
        let tmp = dir().join("mouse.part");
        download(MOUSE_URL, &tmp)?;
        std::fs::rename(&tmp, mouse_path()).map_err(|e| e.to_string())?;
    }
    if !font_path().exists() {
        let tmp = dir().join("font.woff");
        download(FONT_URL, &tmp)?;
        let woff = std::fs::read(&tmp).map_err(|e| e.to_string())?;
        let ttf = woff_to_ttf(&woff).ok_or("Invalid font file")?;
        std::fs::write(font_path(), ttf).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(tmp);
    }
    Ok(())
}

/// Makes "OPPO Sans 4.0" available to every Slint text. Without it Slint falls back to a system font.
pub fn register_font() {
    let Ok(data) = std::fs::read(font_path()) else { return };
    use slint::fontique_011::fontique;
    let blob = fontique::Blob::new(std::sync::Arc::new(data));
    slint::fontique_011::shared_collection().register_fonts(blob, None);
}

pub fn mouse_image() -> Option<slint::Image> {
    let data = std::fs::read(mouse_path()).ok()?;
    let img = image::load_from_memory_with_format(&data, image::ImageFormat::WebP).ok()?.to_rgba8();
    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(img.as_raw(), img.width(), img.height());
    Some(slint::Image::from_rgba8(buf))
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// WOFF 1.0 -> TTF/OTF: unpacks each table (zlib when compressed) and rebuilds the sfnt directory.
fn woff_to_ttf(w: &[u8]) -> Option<Vec<u8>> {
    if w.get(..4)? != b"wOFF" {
        return None;
    }
    let flavor = be32(w, 4)?;
    let num = be16(w, 12)? as usize;
    let mut tables = Vec::with_capacity(num);
    for i in 0..num {
        let e = 44 + i * 20;
        let tag: [u8; 4] = w.get(e..e + 4)?.try_into().ok()?;
        let (off, comp, orig, sum) = (be32(w, e + 4)? as usize, be32(w, e + 8)? as usize, be32(w, e + 12)? as usize, be32(w, e + 16)?);
        let raw = w.get(off..off + comp)?;
        let data = if comp == orig { raw.to_vec() } else { miniz_oxide::inflate::decompress_to_vec_zlib(raw).ok()? };
        tables.push((tag, sum, data));
    }
    tables.sort_by_key(|t| t.0);
    let es = (usize::BITS - 1 - num.max(1).leading_zeros()) as u16;
    let range = (1u16 << es) * 16;
    let mut out = Vec::new();
    out.extend_from_slice(&flavor.to_be_bytes());
    for v in [num as u16, range, es, num as u16 * 16 - range] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    let mut offset = 12 + 16 * num;
    let mut body = Vec::new();
    for (tag, sum, data) in &tables {
        out.extend_from_slice(tag);
        for v in [*sum, (offset + body.len()) as u32, data.len() as u32] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(data);
        body.resize(body.len().div_ceil(4) * 4, 0);
    }
    offset += body.len();
    out.extend(body);
    debug_assert_eq!(out.len(), offset);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn woff_with_one_plain_table() {
        // header (44 bytes) + 1 directory entry (20) + 5 bytes of table data
        let mut w = vec![0u8; 64];
        w[..4].copy_from_slice(b"wOFF");
        w[4..8].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        w[12..14].copy_from_slice(&1u16.to_be_bytes());
        w[44..48].copy_from_slice(b"name");
        w[48..52].copy_from_slice(&64u32.to_be_bytes());
        w[52..56].copy_from_slice(&5u32.to_be_bytes());
        w[56..60].copy_from_slice(&5u32.to_be_bytes());
        w.extend_from_slice(b"hello");
        let t = woff_to_ttf(&w).unwrap();
        assert_eq!(&t[..6], &[0, 1, 0, 0, 0, 1]);
        assert_eq!(&t[12..16], b"name");
        assert_eq!(be32(&t, 20), Some(28));
        assert_eq!(&t[28..33], b"hello");
        assert_eq!(t.len() % 4, 0);
        assert_eq!(woff_to_ttf(b"nope"), None);
    }
}
