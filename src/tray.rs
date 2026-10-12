//! Tray icon: the battery percentage drawn on a dark rounded square.

use tray_icon::Icon;

/// 3x5 digits, one bit per column (bit 2 = left).
const FONT: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

const ICON: usize = 32;

fn in_rounded_square(x: usize, y: usize) -> bool {
    let (r, n) = (7.0f32, ICON as f32);
    let (cx, cy) = ((x as f32 + 0.5).clamp(r, n - r), (y as f32 + 0.5).clamp(r, n - r));
    let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
    dx * dx + dy * dy <= r * r
}

fn paint(rgba: &mut [u8], x: usize, y: usize, c: [u8; 4]) {
    rgba[(y * ICON + x) * 4..][..4].copy_from_slice(&c);
}

pub fn battery_icon(level: Option<u8>, charging: bool) -> Icon {
    let mut rgba = vec![0u8; ICON * ICON * 4];
    for y in 0..ICON {
        for x in 0..ICON {
            if in_rounded_square(x, y) {
                paint(&mut rgba, x, y, [0x14, 0x15, 0x16, 0xFF]);
            }
        }
    }
    let color = match (level, charging) {
        (None, _) => [0x8A, 0x8F, 0x98, 0xFF],
        (_, true) => [0x4C, 0xB7, 0x82, 0xFF],
        (Some(l), _) if l <= 15 => [0xEB, 0x57, 0x57, 0xFF],
        (Some(l), _) if l < 30 => [0xF2, 0xC9, 0x4C, 0xFF],
        _ => [0xF7, 0xF8, 0xF8, 0xFF],
    };
    let digits: Vec<usize> = match level {
        Some(l) => l.min(100).to_string().bytes().map(|b| (b - b'0') as usize).collect(),
        None => vec![],
    };
    if digits.is_empty() {
        // no reading: a dash in the middle
        for y in 14..18 {
            for x in 9..23 {
                paint(&mut rgba, x, y, color);
            }
        }
    } else {
        let scale = match digits.len() {
            1 => 6,
            2 => 5,
            _ => 3,
        };
        let width = digits.len() * 3 * scale + (digits.len() - 1);
        let (x0, y0) = ((ICON - width) / 2, (ICON - 5 * scale) / 2);
        for (i, d) in digits.iter().enumerate() {
            for (row, bits) in FONT[*d].iter().enumerate() {
                for col in (0..3).filter(|c| bits & (4 >> c) != 0) {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            paint(&mut rgba, x0 + i * (3 * scale + 1) + col * scale + sx, y0 + row * scale + sy, color);
                        }
                    }
                }
            }
        }
    }
    Icon::from_rgba(rgba, ICON as u32, ICON as u32).expect("invalid icon")
}
