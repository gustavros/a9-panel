//! Full mouse state (what the Hub loads on its "Data Loading" screen) and the possible changes.

use crate::backup;
use crate::device::Mouse;
use crate::protocol::*;

#[derive(Clone, Debug, Default)]
pub struct State {
    pub battery: Option<Battery>,
    pub version: String,
    pub dongle_version: String,
    pub cid_mid: (u8, u8),
    pub rate_hz: u32,
    pub dpi_count: u8,
    pub dpi_current: u8,
    pub dpi: Vec<(u32, u32)>,
    pub debounce_ms: u8,
    pub motion_sync: bool,
    pub sleep_s: u32,
    pub linear: bool,
    pub ripple: bool,
    pub sensor_sleep_on: bool,
    pub sensor_sleep_s: u32,
    pub sensor_mode: u8,
    pub silent_height: u8,
    pub far_distance: bool,
    pub dongle_light: u8,
    pub profile: u8,
    pub buttons: Vec<[u8; 4]>,
}

#[derive(Clone, Debug)]
pub enum Change {
    Rate(u32),
    Dpi { level: usize, x: u32, y: u32 },
    DpiCount(u8),
    DpiCurrent(u8),
    Debounce(u8),
    MotionSync(bool),
    SleepS(u32),
    Linear(bool),
    Ripple(bool),
    SensorSleep(bool),
    SensorSleepS(u32),
    SensorMode(u8),
    SilentHeight(u8),
    FarDistance(bool),
    DongleLight(u8),
    Profile(u8),
    Button { index: usize, class: u8, v1: u8, v2: u8 },
    Macro { slot: usize, events: Vec<MacroEvent> },
    FactoryReset,
    Undo,
}

/// Reads only the battery (for the tray icon, without waking everything else).
pub fn battery(m: &Mouse) -> Result<Battery, String> {
    let r = m.cmd(CMD_GET_BATTERY, 0, &[])?;
    let mut full = vec![0u8; DATA];
    full.extend(r);
    parse_battery(&full).ok_or("Incomplete battery reply".into())
}

fn ver(d: &[u8]) -> String {
    format!("{:x}.{:02x}", d[0], d[1])
}

/// Reads everything, calling `progress` from 0 to 100.
pub fn load(m: &Mouse, progress: &dyn Fn(i32)) -> Result<State, String> {
    let mut s = State { battery: battery(m).ok(), ..Default::default() };
    progress(8);
    s.version = ver(&m.cmd(CMD_GET_VERSION, 0, &[])?);
    s.dongle_version = m.cmd(CMD_GET_DONGLE_VERSION, 0, &[]).map(|d| format!("{}.{}.{}", d[0], d[1], d[2])).unwrap_or_default();
    let c = m.cmd(CMD_GET_CID_MID, 0, &[])?;
    s.cid_mid = (c[0], c[1]);
    progress(20);

    let b = m.read(ADDR_RATE_DPI, 10)?;
    s.rate_hz = rate_hz(b[0]).unwrap_or(0);
    s.dpi_count = b[2].clamp(1, DPI_LEVELS_MAX as u8);
    s.dpi_current = b[4].min(s.dpi_count - 1);
    progress(30);

    for addr in ADDR_DPI_TABLES {
        let t = m.read(addr, 8)?;
        for e in t.chunks(4) {
            s.dpi.push(decode_dpi(e).unwrap_or((800, 800)));
        }
    }
    progress(50);

    let p = m.read(ADDR_PARAMS, 10)?;
    s.debounce_ms = p[0];
    s.motion_sync = p[2] != 0;
    s.sleep_s = p[4] as u32 * 10;
    s.linear = p[6] != 0;
    s.ripple = p[8] != 0;
    let sn = m.read(ADDR_SENSOR, 6)?;
    s.sensor_sleep_on = sn[0] != 0;
    s.sensor_sleep_s = sn[2] as u32 * 10;
    s.sensor_mode = sn[4];
    s.silent_height = m.read(ADDR_SILENT_HEIGHT, 2)?[0];
    progress(70);

    s.far_distance = m.cmd(CMD_GET_FAR_DISTANCE, 0, &[])?[0] != 0;
    s.dongle_light = m.cmd(CMD_GET_DONGLE_LIGHT, 0, &[]).map(|d| d[0]).unwrap_or(0);
    s.profile = m.cmd(CMD_GET_PROFILE, 0, &[])?[0];
    progress(80);

    let k = m.read(ADDR_KEYS, 24)?;
    s.buttons = k.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    progress(100);
    Ok(s)
}

pub fn macro_addr(slot: usize) -> u16 {
    ADDR_MACROS + MACRO_STRIDE * slot as u16
}

pub fn read_macro(m: &Mouse, slot: usize) -> Result<Vec<MacroEvent>, String> {
    let base = macro_addr(slot) + 31;
    let n = m.read(base, 1)?[0] as usize;
    if n == 0 || n > MACRO_MAX_EVENTS {
        return Ok(vec![]);
    }
    Ok(parse_macro(&m.read(base, 1 + n * 5)?))
}

/// One history line with the config block, so undo also covers a factory reset.
const CONFIG_SPAN: usize = 190;

pub fn apply(m: &Mouse, c: &Change) -> Result<(), String> {
    match *c {
        Change::Rate(hz) => {
            let code = rate_code(hz).ok_or("Unsupported polling rate")?;
            m.modify(ADDR_RATE_DPI, 10, |b| set_pair(b, 0, code))
        }
        Change::DpiCount(n) => {
            let n = n.clamp(1, DPI_LEVELS_MAX as u8);
            m.modify(ADDR_RATE_DPI, 10, |b| {
                set_pair(b, 2, n);
                if b[4] >= n {
                    set_pair(b, 4, n - 1);
                }
            })
        }
        Change::DpiCurrent(i) => m.modify(ADDR_RATE_DPI, 10, |b| set_pair(b, 4, i)),
        Change::Dpi { level, x, y } => {
            let e = encode_dpi(x, y).ok_or("DPI out of the accepted range")?;
            let addr = *ADDR_DPI_TABLES.get(level / 2).ok_or("Invalid DPI level")?;
            m.modify(addr, 8, |b| b[(level % 2) * 4..][..4].copy_from_slice(&e))
        }
        Change::Debounce(ms) => m.modify(ADDR_PARAMS, 10, |b| set_pair(b, 0, ms)),
        Change::MotionSync(on) => m.modify(ADDR_PARAMS, 10, |b| set_pair(b, 2, on as u8)),
        Change::SleepS(s) => m.modify(ADDR_PARAMS, 10, |b| set_pair(b, 4, (s / 10).clamp(1, 255) as u8)),
        Change::Linear(on) => m.modify(ADDR_PARAMS, 10, |b| set_pair(b, 6, on as u8)),
        Change::Ripple(on) => m.modify(ADDR_PARAMS, 10, |b| set_pair(b, 8, on as u8)),
        Change::SensorSleep(on) => m.modify(ADDR_SENSOR, 6, |b| set_pair(b, 0, on as u8)),
        Change::SensorSleepS(s) => m.modify(ADDR_SENSOR, 6, |b| set_pair(b, 2, (s / 10).clamp(1, 255) as u8)),
        Change::SensorMode(v) => m.modify(ADDR_SENSOR, 6, |b| set_pair(b, 4, v)),
        Change::SilentHeight(v) => m.modify(ADDR_SILENT_HEIGHT, 2, |b| set_pair(b, 0, v)),
        Change::FarDistance(on) => set_and_check(m, CMD_SET_FAR_DISTANCE, CMD_GET_FAR_DISTANCE, on as u8),
        Change::DongleLight(v) => set_and_check(m, CMD_SET_DONGLE_LIGHT, CMD_GET_DONGLE_LIGHT, v),
        Change::Profile(p) => {
            m.cmd(CMD_SET_PROFILE, 1, &[p])?;
            check(m.cmd(CMD_GET_PROFILE, 0, &[])?[0] == p)
        }
        Change::Button { index, class, v1, v2 } => {
            if index >= BUTTON_NAMES.len() {
                return Err("Invalid button".into());
            }
            m.write(ADDR_KEYS + 4 * index as u16, &encode_button(class, v1, v2))
        }
        Change::Macro { slot, ref events } => {
            if slot >= MACRO_SLOTS {
                return Err("Invalid macro slot".into());
            }
            let bytes = macro_bytes(events).ok_or(format!("Macro has more than {MACRO_MAX_EVENTS} events"))?;
            m.write(macro_addr(slot), &bytes)
        }
        Change::FactoryReset => {
            backup::record(0, &m.read(0, CONFIG_SPAN)?);
            m.cmd(CMD_RESTORE_FACTORY, 0, &[]).map(|_| ())
        }
        Change::Undo => backup::undo_last(m),
    }
}

/// Single-value commands (outside the EEPROM): write and verify by reading back.
fn set_and_check(m: &Mouse, set: u8, get: u8, v: u8) -> Result<(), String> {
    m.cmd(set, 10, &[v])?;
    check(m.cmd(get, 0, &[])?[0] == v)
}

fn check(ok: bool) -> Result<(), String> {
    if ok { Ok(()) } else { Err("The mouse did not accept the change".into()) }
}
