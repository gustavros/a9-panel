//! Key names and conversion from Slint's key text to HID codes.

use crate::i18n::t;
use crate::protocol::{MacroEvent, MacroKind};
use slint::platform::Key;

/// (HID code, name)
const KEYS: [(u8, &str); 68] = [
    (4, "A"), (5, "B"), (6, "C"), (7, "D"), (8, "E"), (9, "F"), (10, "G"), (11, "H"), (12, "I"),
    (13, "J"), (14, "K"), (15, "L"), (16, "M"), (17, "N"), (18, "O"), (19, "P"), (20, "Q"),
    (21, "R"), (22, "S"), (23, "T"), (24, "U"), (25, "V"), (26, "W"), (27, "X"), (28, "Y"),
    (29, "Z"), (30, "1"), (31, "2"), (32, "3"), (33, "4"), (34, "5"), (35, "6"), (36, "7"),
    (37, "8"), (38, "9"), (39, "0"), (40, "Enter"), (41, "Esc"), (42, "Backspace"), (43, "Tab"),
    (44, "Space"), (45, "-"), (46, "="), (47, "["), (48, "]"), (49, "\\"), (51, ";"), (52, "'"),
    (53, "`"), (54, ","), (55, "."), (56, "/"), (58, "F1"), (59, "F2"), (60, "F3"), (61, "F4"),
    (62, "F5"), (63, "F6"), (64, "F7"), (65, "F8"), (66, "F9"), (67, "F10"), (68, "F11"),
    (69, "F12"), (79, "→"), (80, "←"), (81, "↓"), (82, "↑"),
];

const MODIFIERS: [(u8, &str); 4] = [(1, "Ctrl"), (2, "Shift"), (4, "Alt"), (8, "Win")];
const MOUSE: [(u8, &str); 5] = [(1, "Left click"), (2, "Right click"), (4, "Middle click"), (8, "Back"), (16, "Forward")];

/// Names to pick by hand (common keys first). Index -> (kind, code).
pub fn picker() -> Vec<(MacroKind, u8, String)> {
    let mut v: Vec<_> = KEYS.iter().map(|(c, n)| (MacroKind::Key, *c, t(n))).collect();
    v.extend(MODIFIERS.iter().map(|(c, n)| (MacroKind::Modifier, *c, n.to_string())));
    v.extend(MOUSE.iter().map(|(c, n)| (MacroKind::Mouse, *c, t(n))));
    v.push((MacroKind::Scroll, 1, t("Wheel up")));
    v.push((MacroKind::Scroll, 255, t("Wheel down")));
    v
}

pub fn describe(e: &MacroEvent) -> String {
    let find = |table: &[(u8, &str)]| table.iter().find(|k| k.0 == e.code).map_or(format!("#{}", e.code), |k| t(k.1));
    let name = match e.kind {
        MacroKind::Key => find(&KEYS),
        MacroKind::Modifier => find(&MODIFIERS),
        MacroKind::Mouse => find(&MOUSE),
        MacroKind::Scroll => return t(if e.code == 1 { "Wheel up" } else { "Wheel down" }),
    };
    if e.down { format!("{name} ↓ {}", t("press")) } else { format!("{name} ↑ {}", t("release")) }
}

/// Converts the text Slint sends in a key event.
pub fn from_text(text: &str) -> Option<(MacroKind, u8)> {
    let special: [(Key, MacroKind, u8); 22] = [
        (Key::Control, MacroKind::Modifier, 1),
        (Key::Shift, MacroKind::Modifier, 2),
        (Key::Alt, MacroKind::Modifier, 4),
        (Key::Meta, MacroKind::Modifier, 8),
        (Key::Return, MacroKind::Key, 40),
        (Key::Escape, MacroKind::Key, 41),
        (Key::Backspace, MacroKind::Key, 42),
        (Key::Tab, MacroKind::Key, 43),
        (Key::RightArrow, MacroKind::Key, 79),
        (Key::LeftArrow, MacroKind::Key, 80),
        (Key::DownArrow, MacroKind::Key, 81),
        (Key::UpArrow, MacroKind::Key, 82),
        (Key::F1, MacroKind::Key, 58),
        (Key::F2, MacroKind::Key, 59),
        (Key::F3, MacroKind::Key, 60),
        (Key::F4, MacroKind::Key, 61),
        (Key::F5, MacroKind::Key, 62),
        (Key::F6, MacroKind::Key, 63),
        (Key::F7, MacroKind::Key, 64),
        (Key::F8, MacroKind::Key, 65),
        (Key::F9, MacroKind::Key, 66),
        (Key::F10, MacroKind::Key, 67),
    ];
    for (k, kind, code) in special {
        let s: slint::SharedString = k.into();
        if s.as_str() == text {
            return Some((kind, code));
        }
    }
    let c = text.chars().next()?.to_ascii_uppercase();
    let name = if c == ' ' { "Space".to_string() } else { c.to_string() };
    KEYS.iter().find(|k| k.1 == name).map(|k| (MacroKind::Key, k.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_special_keys() {
        assert_eq!(from_text("a"), Some((MacroKind::Key, 4)));
        assert_eq!(from_text("Z"), Some((MacroKind::Key, 29)));
        assert_eq!(from_text(" "), Some((MacroKind::Key, 44)));
        let shift: slint::SharedString = Key::Shift.into();
        assert_eq!(from_text(&shift), Some((MacroKind::Modifier, 2)));
        let e = MacroEvent { kind: MacroKind::Key, down: true, code: 4, delay_ms: 10 };
        assert_eq!(describe(&e), "A ↓ press");
    }
}
