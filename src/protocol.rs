//! 16-byte packets of the ATK receiver and the format of each setting
//! (see docs/protocol.md). Nothing here talks to hardware: it only builds and parses bytes.

pub const REPORT_ID: u8 = 8;
pub const PACKET_LEN: usize = 16;
pub const DATA: usize = 5;
/// Maximum data bytes per packet.
pub const CHUNK: usize = 10;

pub const CMD_GET_BATTERY: u8 = 4;
pub const CMD_SET_EEPROM: u8 = 7;
pub const CMD_GET_EEPROM: u8 = 8;
pub const CMD_RESTORE_FACTORY: u8 = 9;
pub const CMD_GET_PROFILE: u8 = 14;
pub const CMD_SET_PROFILE: u8 = 15;
pub const CMD_GET_CID_MID: u8 = 16;
pub const CMD_GET_VERSION: u8 = 18;
pub const CMD_SET_DONGLE_LIGHT: u8 = 20;
pub const CMD_GET_DONGLE_LIGHT: u8 = 21;
pub const CMD_SET_FAR_DISTANCE: u8 = 22;
pub const CMD_GET_FAR_DISTANCE: u8 = 23;
pub const CMD_GET_DONGLE_VERSION: u8 = 29;

// EEPROM addresses (the Hub's `At.Address` map)
pub const ADDR_RATE_DPI: u16 = 0;
pub const ADDR_SILENT_HEIGHT: u16 = 10;
pub const ADDR_DPI_TABLES: [u16; 4] = [12, 20, 28, 36];
pub const ADDR_KEYS: u16 = 96;
pub const ADDR_PARAMS: u16 = 169;
pub const ADDR_SENSOR: u16 = 181;
pub const ADDR_MACROS: u16 = 768;
pub const MACRO_STRIDE: u16 = 384;
pub const MACRO_SLOTS: usize = 12;
pub const MACRO_MAX_EVENTS: usize = 70;

/// (Hz, code stored in the mouse)
pub const RATES: [(u32, u8); 7] = [(125, 8), (250, 4), (500, 2), (1000, 1), (2000, 16), (4000, 32), (8000, 64)];

fn checksum(head: &[u8]) -> u8 {
    let sum: u32 = REPORT_ID as u32 + head.iter().map(|&b| b as u32).sum::<u32>();
    85u8.wrapping_sub(sum as u8)
}

pub fn build(cmd: u8, addr: u16, len: u8, data: &[u8]) -> [u8; PACKET_LEN] {
    let mut p = [0u8; PACKET_LEN];
    p[0] = cmd;
    p[2..4].copy_from_slice(&addr.to_be_bytes());
    p[4] = len;
    p[DATA..DATA + data.len()].copy_from_slice(data);
    p[15] = checksum(&p[..15]);
    p
}

/// Request without data (status 0), checksum in byte 15.
pub fn request(cmd: u8, addr: u16, len: u8) -> [u8; PACKET_LEN] {
    build(cmd, addr, len, &[])
}

/// EEPROM write request (up to 10 bytes).
pub fn write_eeprom(addr: u16, data: &[u8]) -> [u8; PACKET_LEN] {
    assert!(data.len() <= CHUNK);
    build(CMD_SET_EEPROM, addr, data.len() as u8, data)
}

/// Data part of a response (from byte 5 on).
pub fn payload(r: &[u8], len: usize) -> Option<&[u8]> {
    r.get(DATA..DATA + len)
}

/// Writes `value` and `85 - value` (how the EEPROM stores almost everything).
pub fn set_pair(buf: &mut [u8], at: usize, v: u8) {
    buf[at] = v;
    buf[at + 1] = 85u8.wrapping_sub(v);
}

/// CRC of the 4-byte blocks (DPI, buttons): `85 - (a + b + c)`.
pub fn crc3(a: u8, b: u8, c: u8) -> u8 {
    85u8.wrapping_sub(a.wrapping_add(b).wrapping_add(c))
}

pub fn rate_code(hz: u32) -> Option<u8> {
    RATES.iter().find(|r| r.0 == hz).map(|r| r.1)
}

pub fn rate_hz(code: u8) -> Option<u32> {
    RATES.iter().find(|r| r.1 == code).map(|r| r.0)
}

// ---------- DPI (PAW3950 Ultra sensor) ----------

pub const DPI_MIN: u32 = 10;
pub const DPI_MAX: u32 = 42000;
pub const DPI_LEVELS_MAX: usize = 8;

/// Rounds to the step the sensor accepts: 10 up to 10000, 50 up to 30000, 100 above.
pub fn snap_dpi(v: u32) -> u32 {
    let v = v.clamp(DPI_MIN, DPI_MAX);
    let step = match v {
        0..=10000 => 10,
        10001..=30000 => 50,
        _ => 100,
    };
    let s = (v + step / 2) / step * step;
    // 10001..10049 does not exist on the sensor: falls to 10050
    if s > 10000 && s < 10050 { 10050 } else { s }.clamp(DPI_MIN, DPI_MAX)
}

/// (steps, flags) of one axis. Flags: 2 = high range, 1 = doubled.
fn dpi_axis(v: u32) -> Option<(u32, u8)> {
    match v {
        10..=10000 if v % 10 == 0 => Some((v / 10 - 1, 0)),
        10001..=30000 if v % 50 == 0 && v >= 10050 => Some(((v - 10050) / 50, 2)),
        30001..=DPI_MAX if v % 100 == 0 => Some(((v / 2 - 10050) / 50, 3)),
        _ => None,
    }
}

/// Encodes one DPI level: [x, y, extra, crc].
pub fn encode_dpi(x: u32, y: u32) -> Option<[u8; 4]> {
    let (sx, fx) = dpi_axis(x)?;
    let (sy, fy) = dpi_axis(y)?;
    let extra = (((sy >> 8) & 3) as u8) << 6 | (((sx >> 8) & 3) as u8) << 2 | fx | fy << 4;
    let (bx, by) = ((sx & 0xFF) as u8, (sy & 0xFF) as u8);
    Some([bx, by, extra, crc3(bx, by, extra)])
}

/// Decodes one DPI level into (x, y).
pub fn decode_dpi(e: &[u8]) -> Option<(u32, u32)> {
    let (x, y, ex) = (*e.first()? as u32, *e.get(1)? as u32, *e.get(2)? as u32);
    let axis = |low: u32, high: u32, range: u32, double: u32| {
        let s = low | high << 8;
        let v = if range != 0 { s * 50 + 10050 } else { (s + 1) * 10 };
        if double != 0 { v * 2 } else { v }
    };
    Some((axis(x, (ex & 12) >> 2, ex & 2, ex & 1), axis(y, (ex & 192) >> 6, ex & 32, ex & 16)))
}

// ---------- Buttons ----------

/// Button order in the mouse (key0..key5).
pub const BUTTON_NAMES: [&str; 6] = ["Left button", "Right button", "Middle button", "Back", "Forward", "DPI button"];
/// Factory default of each button.
pub const BUTTON_DEFAULTS: [[u8; 3]; 6] = [[1, 1, 0], [1, 2, 0], [1, 4, 0], [1, 8, 0], [1, 16, 0], [2, 1, 0]];

/// Known actions: (class, value1, value2, name). For macros value1 is the slot.
pub const ACTIONS: [(u8, u8, u8, &str); 14] = [
    (0, 0, 0, "Disabled"),
    (1, 1, 0, "Left click"),
    (1, 2, 0, "Right click"),
    (1, 4, 0, "Middle click"),
    (1, 8, 0, "Back"),
    (1, 16, 0, "Forward"),
    (2, 1, 0, "DPI cycle"),
    (2, 2, 0, "DPI +"),
    (2, 3, 0, "DPI −"),
    (3, 1, 0, "Scroll left"),
    (3, 2, 0, "Scroll right"),
    (11, 1, 0, "Wheel up"),
    (11, 2, 0, "Wheel down"),
    (6, 0, 1, "Macro"),
];
pub const CLASS_MACRO: u8 = 6;

/// Macro repeat modes on a button (value2).
pub const MACRO_MODES: [(u8, &str); 3] = [(1, "Once"), (254, "While held"), (253, "Until pressed again")];

pub fn encode_button(class: u8, v1: u8, v2: u8) -> [u8; 4] {
    [class, v1, v2, crc3(class, v1, v2)]
}

pub fn action_name(b: &[u8]) -> String {
    match b {
        [CLASS_MACRO, slot, ..] => format!("Macro M{}", slot + 1),
        [c, v1, v2, ..] => ACTIONS
            .iter()
            .find(|a| a.0 == *c && a.1 == *v1 && a.2 == *v2)
            .map_or_else(|| format!("Other ({c},{v1},{v2})"), |a| a.3.to_string()),
        _ => "?".into(),
    }
}

// ---------- Macros ----------

/// Fixed header the Hub writes before the event count.
pub fn macro_header() -> [u8; 31] {
    let mut h = [255u8; 31];
    h[0] = 8;
    h[1..9].fill(2);
    h
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MacroKind {
    Key,
    Modifier,
    Mouse,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroEvent {
    pub kind: MacroKind,
    pub down: bool,
    /// HID key code, modifier mask, mouse button or 1/255 for the wheel.
    pub code: u8,
    pub delay_ms: u16,
}

impl MacroEvent {
    pub fn encode(&self) -> [u8; 5] {
        let state = match (self.kind, self.down) {
            (MacroKind::Scroll, _) => 0,
            (_, true) => 0x80,
            (_, false) => 0x40,
        };
        let kind = match self.kind {
            MacroKind::Modifier => 0b000,
            MacroKind::Key => 0b001,
            MacroKind::Mouse => 0b100,
            MacroKind::Scroll => 0b110,
        };
        let d = self.delay_ms.max(1).to_be_bytes();
        [state | kind, self.code, 0, d[0], d[1]]
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        let [flags, code, _, d0, d1] = *b.get(..5)? else { return None };
        let kind = match flags & 7 {
            0b000 => MacroKind::Modifier,
            0b100 => MacroKind::Mouse,
            0b110 => MacroKind::Scroll,
            _ => MacroKind::Key,
        };
        Some(Self { kind, down: flags & 0x80 != 0, code, delay_ms: u16::from_be_bytes([d0, d1]) })
    }
}

/// Full bytes of a macro slot: header, count, events, crc, padded to a multiple of 10.
pub fn macro_bytes(events: &[MacroEvent]) -> Option<Vec<u8>> {
    if events.len() > MACRO_MAX_EVENTS {
        return None;
    }
    let mut body = vec![events.len() as u8];
    for e in events {
        body.extend_from_slice(&e.encode());
    }
    let sum: u32 = body.iter().map(|&b| b as u32).sum();
    body.push(85u8.wrapping_sub(sum as u8));
    let mut all = macro_header().to_vec();
    all.extend(body);
    all.resize(all.len().div_ceil(CHUNK) * CHUNK, 0);
    Some(all)
}

/// Parses events from bytes starting at the count (slot address + 31).
pub fn parse_macro(from_count: &[u8]) -> Vec<MacroEvent> {
    let n = from_count.first().copied().unwrap_or(0) as usize;
    if n == 0 || n > MACRO_MAX_EVENTS {
        return vec![];
    }
    from_count[1..].chunks(5).take(n).filter_map(MacroEvent::decode).collect()
}

// ---------- Other ----------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Battery {
    pub level: u8,
    pub charging: bool,
    pub millivolts: u16,
}

pub fn parse_battery(r: &[u8]) -> Option<Battery> {
    let d = payload(r, 4)?;
    Some(Battery { level: d[0], charging: d[1] != 0, millivolts: u16::from_be_bytes([d[2], d[3]]) })
}

/// Lift-off distance on Ultimate models: byte 1..11 = 0.7 mm to 1.7 mm (the Hub's "SilenceHeightUltimate" screen).
pub const SILENT_HEIGHTS: [(u8, &str); 11] = [
    (1, "0.7"), (2, "0.8"), (3, "0.9"), (4, "1.0"), (5, "1.1"), (6, "1.2"),
    (7, "1.3"), (8, "1.4"), (9, "1.5"), (10, "1.6"), (11, "1.7"),
];
/// Sensor mode (in the Hub's screen order).
pub const SENSOR_MODES: [&str; 3] = ["Basic", "Competitive", "Competitive MAX"];
/// Receiver light (simple mode).
pub const DONGLE_LIGHTS: [&str; 4] = ["Off", "Connection & polling", "Battery", "Low battery only"];

#[cfg(test)]
mod tests {
    use super::*;

    fn sums_to_85(p: &[u8; PACKET_LEN]) -> bool {
        let sum: u32 = REPORT_ID as u32 + p.iter().map(|&b| b as u32).sum::<u32>();
        sum as u8 == 85
    }

    #[test]
    fn checksum_sums_to_85() {
        assert!(sums_to_85(&request(CMD_GET_BATTERY, 0, 0)));
        let p = write_eeprom(0, &[2, 83, 1, 84, 0, 85, 0, 85, 0, 85]);
        assert_eq!(&p[..7], &[7, 0, 0, 0, 10, 2, 83]);
        assert!(sums_to_85(&p));
    }

    #[test]
    fn eeprom_address_is_big_endian() {
        let p = request(CMD_GET_EEPROM, 768 + 31, 1);
        assert_eq!(&p[..5], &[8, 0, 3, 31, 1]);
    }

    #[test]
    fn polling_codes() {
        assert_eq!(rate_code(8000), Some(64));
        assert_eq!(rate_hz(1), Some(1000));
        assert_eq!(rate_code(3000), None);
        let mut b = [1, 84, 1, 84, 0, 85, 0, 85, 0, 85];
        set_pair(&mut b, 0, 64);
        assert_eq!(&b[..2], &[64, 21]);
    }

    #[test]
    fn dpi_1600_matches_the_hub() {
        // the Hub shows 1600 for the owner's mouse: byte 159 on both axes, no flags
        let e = encode_dpi(1600, 1600).unwrap();
        assert_eq!(&e[..3], &[159, 159, 0]);
        assert_eq!(decode_dpi(&e), Some((1600, 1600)));
    }

    #[test]
    fn dpi_round_trip_in_every_range() {
        for v in [10, 50, 400, 800, 10000, 10050, 16000, 30000, 30100, 36000, 42000] {
            let e = encode_dpi(v, v).unwrap();
            assert_eq!(decode_dpi(&e), Some((v, v)), "value {v}");
            assert_eq!(e[3], crc3(e[0], e[1], e[2]));
        }
        let e = encode_dpi(800, 26000).unwrap();
        assert_eq!(decode_dpi(&e), Some((800, 26000)));
        assert_eq!(encode_dpi(5, 5), None);
        assert_eq!(encode_dpi(10020, 800), None);
        assert_eq!(encode_dpi(42100, 800), None);
    }

    #[test]
    fn snap_dpi_lands_on_valid_value() {
        for v in [1, 9, 1623, 10004, 10020, 10040, 29990, 30040, 30060, 41999, 99999] {
            let s = snap_dpi(v);
            assert!(encode_dpi(s, s).is_some(), "{v} -> {s}");
        }
        assert_eq!(snap_dpi(1623), 1620);
        assert_eq!(snap_dpi(30040), 30000);
        assert_eq!(snap_dpi(30060), 30100);
    }

    #[test]
    fn button_with_crc() {
        assert_eq!(encode_button(1, 1, 0), [1, 1, 0, 83]);
        assert_eq!(action_name(&[2, 1, 0, 82]), "DPI cycle");
        assert_eq!(action_name(&[6, 2, 1, 0]), "Macro M3");
    }

    #[test]
    fn macro_round_trip() {
        let ev = [
            MacroEvent { kind: MacroKind::Key, down: true, code: 4, delay_ms: 30 },
            MacroEvent { kind: MacroKind::Key, down: false, code: 4, delay_ms: 30 },
            MacroEvent { kind: MacroKind::Modifier, down: true, code: 1, delay_ms: 1 },
        ];
        let b = macro_bytes(&ev).unwrap();
        assert_eq!(b.len() % CHUNK, 0);
        assert_eq!(&b[..9], &[8, 2, 2, 2, 2, 2, 2, 2, 2]);
        assert_eq!(b[31], 3);
        assert_eq!(&b[32..37], &[0x81, 4, 0, 0, 30]);
        assert_eq!(parse_macro(&b[31..]), ev.to_vec());
        let sum: u32 = b[31..31 + 1 + 15 + 1].iter().map(|&x| x as u32).sum();
        assert_eq!(sum as u8, 85);
    }
}
