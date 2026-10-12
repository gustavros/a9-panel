#![windows_subsystem = "windows"]

mod assets;
mod backup;
mod device;
mod gamemode;
mod i18n;
mod keys;
mod model;
mod protocol;
mod settings;
mod tray;

use i18n::{t, tf};
use model::{Change, State};
use protocol::*;
use slint::{ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

slint::include_modules!();

/// Work for the mouse. A single thread runs everything in order, so there are never two conversations at once.
enum Job {
    Load { overlay: bool },
    Battery,
    Apply { change: Change, label: String, reload: bool },
    ReadMacro(usize),
}

enum Msg {
    Progress(i32),
    Loaded(Result<State, String>),
    Battery(Result<Battery, String>),
    /// (label, was it a macro)
    Applied(Result<(String, bool), String>),
    Macro(usize, Result<Vec<MacroEvent>, String>),
    Done,
}

fn worker(jobs: mpsc::Receiver<Job>, tx: mpsc::Sender<Msg>) {
    let mut mouse: Option<device::Mouse> = None;
    for job in jobs {
        let m = match mouse.take().map_or_else(device::Mouse::open, Ok) {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.send(match job {
                    Job::Load { .. } => Msg::Loaded(Err(e)),
                    Job::Battery => Msg::Battery(Err(e)),
                    Job::Apply { .. } => Msg::Applied(Err(e)),
                    Job::ReadMacro(slot) => Msg::Macro(slot, Err(e)),
                });
                let _ = tx.send(Msg::Done);
                continue;
            }
        };
        let ok = match job {
            Job::Load { overlay } => {
                let p = tx.clone();
                let progress = move |v: i32| {
                    if overlay {
                        let _ = p.send(Msg::Progress(v));
                    }
                };
                let res = model::load(&m, &progress);
                let ok = res.is_ok();
                let _ = tx.send(Msg::Loaded(res));
                ok
            }
            Job::Battery => {
                let res = model::battery(&m);
                let ok = res.is_ok();
                let _ = tx.send(Msg::Battery(res));
                ok
            }
            Job::Apply { change, label, reload } => {
                let is_macro = matches!(change, Change::Macro { .. });
                let res = model::apply(&m, &change);
                let ok = res.is_ok();
                let _ = tx.send(Msg::Applied(res.map(|_| (label, is_macro))));
                if ok && reload {
                    let _ = tx.send(Msg::Loaded(model::load(&m, &|_| {})));
                }
                ok
            }
            Job::ReadMacro(slot) => {
                let res = model::read_macro(&m, slot);
                let ok = res.is_ok();
                let _ = tx.send(Msg::Macro(slot, res));
                ok
            }
        };
        // on error, reopen next time (the receiver may have been unplugged)
        if ok {
            mouse = Some(m);
        }
        let _ = tx.send(Msg::Done);
    }
}

fn strings(v: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(v.into_iter().map(SharedString::from).collect::<Vec<_>>()))
}

/// Plain actions (everything but "Macro"); after them the button list has M1..M12.
const BASIC: usize = ACTIONS.len() - 1;

fn button_row(i: usize, b: &[u8; 4]) -> ButtonRow {
    let (action, slot, mode) = if b[0] == CLASS_MACRO {
        let slot = (b[1] as usize).min(MACRO_SLOTS - 1);
        (BASIC + slot, slot, MACRO_MODES.iter().position(|m| m.0 == b[2]).unwrap_or(0))
    } else {
        let a = ACTIONS[..BASIC].iter().position(|a| a.0 == b[0] && a.1 == b[1] && a.2 == b[2]);
        (a.unwrap_or(0), 0, 0)
    };
    ButtonRow {
        name: t(BUTTON_NAMES[i]).into(),
        action: action as i32,
        macro_slot: slot as i32,
        macro_mode: mode as i32,
    }
}

fn action_label(class: u8, v1: u8, v2: u8) -> String {
    let name = protocol::action_name(&[class, v1, v2]);
    t(&name)
}

/// Lists that depend on the language.
fn fill_lists(store: &Store, picker: &[(MacroKind, u8, String)]) {
    store.set_action_names(strings(
        ACTIONS[..BASIC].iter().map(|a| t(a.3)).chain((1..=MACRO_SLOTS).map(|i| format!("Macro M{i}"))),
    ));
    store.set_macro_modes(strings(MACRO_MODES.iter().map(|m| t(m.1))));
    store.set_sensor_modes(strings(SENSOR_MODES.iter().map(|s| t(s))));
    store.set_dongle_lights(strings(DONGLE_LIGHTS.iter().map(|s| t(s))));
    store.set_key_names(strings(picker.iter().map(|p| p.2.clone())));
}

fn show_state(store: &Store, s: &State) {
    store.set_connected(true);
    if let Some(b) = s.battery {
        store.set_battery(b.level as i32);
        store.set_charging(b.charging);
        store.set_voltage(format!("{:.2} V", b.millivolts as f32 / 1000.0).into());
    }
    store.set_version(s.version.clone().into());
    store.set_dongle_version(if s.dongle_version.is_empty() { "-".into() } else { s.dongle_version.clone().into() });
    store.set_model_id(format!("CID {} · MID {}", s.cid_mid.0, s.cid_mid.1).into());
    store.set_rate_index(RATES.iter().position(|r| r.0 == s.rate_hz).map_or(-1, |i| i as i32));
    store.set_sensor_mode(if (s.sensor_mode as usize) < SENSOR_MODES.len() { s.sensor_mode as i32 } else { -1 });
    store.set_sensor_sleep_on(s.sensor_sleep_on);
    store.set_sensor_sleep_s(s.sensor_sleep_s as i32);
    let levels: Vec<DpiLevel> =
        s.dpi.iter().take(s.dpi_count as usize).map(|&(x, y)| DpiLevel { x: x as i32, y: y as i32 }).collect();
    store.set_dpi_levels(ModelRc::new(VecModel::from(levels)));
    store.set_dpi_count(s.dpi_count as i32);
    store.set_dpi_current(s.dpi_current as i32);
    store.set_debounce(s.debounce_ms as i32);
    store.set_motion_sync(s.motion_sync);
    store.set_sleep_s(s.sleep_s as i32);
    store.set_linear(s.linear);
    store.set_ripple(s.ripple);
    store.set_silent_index(SILENT_HEIGHTS.iter().position(|h| h.0 == s.silent_height).map_or(-1, |i| i as i32));
    store.set_far_distance(s.far_distance);
    store.set_dongle_light(if (s.dongle_light as usize) < DONGLE_LIGHTS.len() { s.dongle_light as i32 } else { -1 });
    store.set_profile(s.profile as i32);
    let rows: Vec<ButtonRow> = s.buttons.iter().enumerate().map(|(i, b)| button_row(i, b)).collect();
    store.set_buttons(ModelRc::new(VecModel::from(rows)));
}

fn show_macro(store: &Store, events: &[MacroEvent]) {
    let rows: Vec<MacroRow> =
        events.iter().map(|e| MacroRow { text: keys::describe(e).into(), delay: e.delay_ms as i32 }).collect();
    store.set_macro_events(ModelRc::new(VecModel::from(rows)));
}

fn tooltip(b: Option<Battery>) -> String {
    match b {
        Some(b) if b.charging => tf("A9 Ultimate: {}% (charging)", b.level),
        Some(b) => tf("A9 Ultimate: {}%", b.level),
        None => t("A9 Ultimate: no response (move the mouse)"),
    }
}

struct Ui {
    pending: usize,
    macro_events: Vec<MacroEvent>,
    last_key: Option<Instant>,
    last_state: Option<State>,
    last_battery: Option<Battery>,
    /// status shows a save/error message that should not be cleared by the reload that follows
    sticky: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let prefs = Rc::new(RefCell::new(settings::load()));
    let lang = if prefs.borrow().language.is_empty() { i18n::system() } else { i18n::current_from(&prefs.borrow().language) };
    i18n::set(lang);

    // read-only diagnostics: writes what was read to %APPDATA%\a9-panel\dump.txt and exits
    if std::env::args().any(|a| a == "--dump") {
        let out = device::Mouse::open().and_then(|m| {
            let s = model::load(&m, &|_| {})?;
            let macros: Vec<_> = (0..MACRO_SLOTS).map(|i| model::read_macro(&m, i).map(|e| e.len())).collect();
            Ok(format!("{s:#?}\nmacros (events per slot): {macros:?}\n"))
        });
        let _ = std::fs::create_dir_all(backup::dir());
        std::fs::write(backup::dir().join("dump.txt"), out.unwrap_or_else(|e| format!("error: {e}\n")))?;
        return Ok(());
    }

    // first run: fetch the mouse picture and the font (cached afterwards)
    if let Err(e) = assets::ensure() {
        eprintln!("assets: {e}");
    }

    let ui = App::new()?;
    let _ = slint::select_bundled_translation(lang);
    assets::register_font();
    let store = ui.global::<Store>();
    if let Some(img) = assets::mouse_image() {
        store.set_mouse_image(img);
    }
    // closing only hides; the app stays in the tray
    ui.window().on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    let picker = Rc::new(RefCell::new(keys::picker()));
    fill_lists(&store, &picker.borrow());
    store.set_basic_actions(BASIC as i32);
    store.set_macro_names(strings((1..=MACRO_SLOTS).map(|i| format!("M{i}"))));
    store.set_silent_names(strings(SILENT_HEIGHTS.iter().map(|s| s.1.to_string())));
    store.set_languages(strings(i18n::LANGUAGES.iter().map(|l| l.1.to_string())));
    store.set_language(i18n::LANGUAGES.iter().position(|l| l.0 == lang).unwrap_or(0) as i32);
    store.set_undo_count(backup::count() as i32);
    store.set_start_in_tray(prefs.borrow().start_in_tray);
    store.set_autostart(settings::autostart_enabled());
    {
        let p = prefs.borrow();
        store.set_game_mode(p.game_mode);
        store.set_game_list(p.games.clone().into());
        store.set_game_rate_index(RATES.iter().position(|r| r.0 == p.game_rate_hz).map_or(-1, |i| i as i32));
        store.set_game_sensor_mode(p.game_sensor_mode as i32);
    }
    store.set_status(t("Looking for the mouse...").into());

    let (job_tx, job_rx) = mpsc::channel::<Job>();
    let (msg_tx, msg_rx) = mpsc::channel::<Msg>();
    std::thread::spawn(move || worker(job_rx, msg_tx));

    let st = Rc::new(RefCell::new(Ui {
        pending: 0,
        macro_events: vec![],
        last_key: None,
        last_state: None,
        last_battery: None,
        sticky: false,
    }));
    let queue: Rc<dyn Fn(Job)> = {
        let (st, weak) = (st.clone(), ui.as_weak());
        Rc::new(move |job: Job| {
            st.borrow_mut().pending += 1;
            if let Some(ui) = weak.upgrade() {
                let s = ui.global::<Store>();
                s.set_busy(true);
                if let Job::Load { overlay: true } = job {
                    s.set_progress(0);
                    s.set_loading(true);
                }
            }
            let _ = job_tx.send(job);
        })
    };
    let apply: Rc<dyn Fn(Change, String)> = {
        let (q, weak, st) = (queue.clone(), ui.as_weak(), st.clone());
        Rc::new(move |change: Change, label: String| {
            if let Some(ui) = weak.upgrade() {
                ui.global::<Store>().set_status(tf("Applying {}...", &label).into());
            }
            st.borrow_mut().sticky = true;
            q(Job::Apply { change, label, reload: true });
        })
    };

    // ---- page callbacks
    let q = queue.clone();
    store.on_refresh(move || q(Job::Load { overlay: true }));
    let a = apply.clone();
    store.on_set_rate(move |i| {
        if let Some(r) = RATES.get(i as usize) {
            a(Change::Rate(r.0), tf("polling {} Hz", r.0));
        }
    });
    let a = apply.clone();
    store.on_set_sensor_mode(move |i| {
        if let Some(name) = SENSOR_MODES.get(i as usize) {
            a(Change::SensorMode(i as u8), format!("{}: {}", t("sensor mode"), t(name)));
        }
    });
    let a = apply.clone();
    store.on_set_dpi(move |level, x, y| {
        let (x, y) = (snap_dpi(x.max(0) as u32), snap_dpi(y.max(0) as u32));
        let label = if x == y { format!("DPI {} = {x}", level + 1) } else { format!("DPI {} = {x} x {y}", level + 1) };
        a(Change::Dpi { level: level as usize, x, y }, label);
    });
    let a = apply.clone();
    store.on_set_dpi_count(move |n| a(Change::DpiCount(n as u8), tf("{} DPI levels", n)));
    let a = apply.clone();
    store.on_set_dpi_current(move |i| a(Change::DpiCurrent(i as u8), tf("DPI level {}", i + 1)));
    let a = apply.clone();
    store.on_set_param(move |key, v| {
        let (c, label) = match key.as_str() {
            "debounce" => (Change::Debounce(v.clamp(0, 255) as u8), tf("debounce {} ms", v)),
            "motion" => (Change::MotionSync(v != 0), "Motion Sync".into()),
            "linear" => (Change::Linear(v != 0), t("straight-line correction")),
            "ripple" => (Change::Ripple(v != 0), t("ripple control")),
            "sleep" => (Change::SleepS(v.max(10) as u32), tf("sleep after {} s", v)),
            "ssleep" => (Change::SensorSleep(v != 0), t("sensor sleep")),
            "ssleep-s" => (Change::SensorSleepS(v.max(10) as u32), tf("sensor sleeps after {} s", v)),
            "silent" => match SILENT_HEIGHTS.get(v as usize) {
                Some(h) => (Change::SilentHeight(h.0), tf("lift-off {} mm", h.1)),
                None => return,
            },
            "far" => (Change::FarDistance(v != 0), t("long distance mode")),
            "dongle" => (Change::DongleLight(v as u8), t("receiver light")),
            "profile" => (Change::Profile(v as u8), tf("config {}", v + 1)),
            _ => return,
        };
        a(c, label);
    });
    let a = apply.clone();
    store.on_set_button(move |i, action, _slot, mode| {
        let action = action.max(0) as usize;
        let (class, v1, v2) = if action >= BASIC {
            if action - BASIC >= MACRO_SLOTS {
                return;
            }
            (CLASS_MACRO, (action - BASIC) as u8, MACRO_MODES.get(mode as usize).map_or(1, |m| m.0))
        } else {
            let a = ACTIONS[action];
            (a.0, a.1, a.2)
        };
        let label = format!("{}: {}", t(BUTTON_NAMES[i as usize]), action_label(class, v1, v2));
        a(Change::Button { index: i as usize, class, v1, v2 }, label);
    });
    let (q, st2) = (queue.clone(), st.clone());
    store.on_restore_buttons(move || {
        st2.borrow_mut().sticky = true;
        for (i, d) in BUTTON_DEFAULTS.iter().enumerate() {
            let change = Change::Button { index: i, class: d[0], v1: d[1], v2: d[2] };
            q(Job::Apply { change, label: t("buttons back to default"), reload: i == BUTTON_DEFAULTS.len() - 1 });
        }
    });

    // ---- macros
    let (q, weak) = (queue.clone(), ui.as_weak());
    store.on_open_macro(move |slot| {
        if let Some(ui) = weak.upgrade() {
            let s = ui.global::<Store>();
            s.set_macro_slot(slot);
            s.set_recording(false);
            s.set_status(tf("Reading M{}...", slot + 1).into());
        }
        q(Job::ReadMacro(slot as usize));
    });
    let edit_macro = {
        let (st, weak) = (st.clone(), ui.as_weak());
        move |f: &dyn Fn(&mut Vec<MacroEvent>)| {
            let mut g = st.borrow_mut();
            f(&mut g.macro_events);
            g.macro_events.truncate(MACRO_MAX_EVENTS);
            if let Some(ui) = weak.upgrade() {
                let s = ui.global::<Store>();
                show_macro(&s, &g.macro_events);
                s.set_macro_dirty(true);
            }
        }
    };
    let (e, st2) = (edit_macro.clone(), st.clone());
    store.on_macro_key(move |text, down| {
        let Some((kind, code)) = keys::from_text(&text) else { return };
        let now = Instant::now();
        let gap = st2.borrow().last_key.map(|t| now.duration_since(t).as_millis().clamp(1, 65535) as u16);
        st2.borrow_mut().last_key = Some(now);
        e(&|ev| {
            // the wait is stored on the previous event: "after this one, wait X ms"
            if let (Some(g), Some(last)) = (gap, ev.last_mut()) {
                last.delay_ms = g;
            }
            ev.push(MacroEvent { kind, down, code, delay_ms: 10 });
        });
    });
    let (e, pk) = (edit_macro.clone(), picker.clone());
    store.on_macro_add(move |idx, delay| {
        let Some(&(kind, code, _)) = pk.borrow().get(idx as usize) else { return };
        let d = delay.clamp(1, 65535) as u16;
        e(&|ev| {
            ev.push(MacroEvent { kind, down: true, code, delay_ms: d });
            if kind != MacroKind::Scroll {
                ev.push(MacroEvent { kind, down: false, code, delay_ms: d });
            }
        });
    });
    let e = edit_macro.clone();
    store.on_macro_remove(move |i| {
        e(&|ev| {
            if (i as usize) < ev.len() {
                ev.remove(i as usize);
            }
        })
    });
    let e = edit_macro.clone();
    store.on_macro_delay(move |i, d| {
        e(&|ev| {
            if let Some(x) = ev.get_mut(i as usize) {
                x.delay_ms = d.clamp(1, 65535) as u16;
            }
        })
    });
    let (e, st2) = (edit_macro.clone(), st.clone());
    store.on_macro_clear(move || {
        st2.borrow_mut().last_key = None;
        e(&|ev| ev.clear());
    });
    let (a, st2, weak) = (apply.clone(), st.clone(), ui.as_weak());
    store.on_save_macro(move || {
        let Some(ui) = weak.upgrade() else { return };
        let slot = ui.global::<Store>().get_macro_slot() as usize;
        let events = st2.borrow().macro_events.clone();
        a(Change::Macro { slot, events }, tf("macro M{}", slot + 1));
    });

    // ---- config and app
    let a = apply.clone();
    store.on_undo(move || a(Change::Undo, t("undo")));
    let a = apply.clone();
    store.on_factory_reset(move || a(Change::FactoryReset, t("factory reset")));
    store.on_open_folder(|| {
        let _ = std::fs::create_dir_all(backup::dir());
        let _ = std::process::Command::new("explorer").arg(backup::dir()).spawn();
    });
    let (p, weak) = (prefs.clone(), ui.as_weak());
    store.on_set_start_in_tray(move |v| {
        p.borrow_mut().start_in_tray = v;
        let res = settings::save(&p.borrow());
        if let Some(ui) = weak.upgrade() {
            let s = ui.global::<Store>();
            s.set_start_in_tray(v);
            if let Err(e) = res {
                s.set_status(tf("Could not save the setting: {}", e).into());
            }
        }
    });
    let weak = ui.as_weak();
    store.on_set_autostart(move |v| {
        let res = settings::set_autostart(v);
        if let Some(ui) = weak.upgrade() {
            let s = ui.global::<Store>();
            s.set_autostart(settings::autostart_enabled());
            if let Err(e) = res {
                s.set_status(tf("Could not change start with Windows: {}", e).into());
            }
        }
    });

    // ---- automatic game mode settings
    let save_prefs = {
        let (p, weak) = (prefs.clone(), ui.as_weak());
        move || {
            if let Err(e) = settings::save(&p.borrow()) {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<Store>().set_status(tf("Could not save the setting: {}", e).into());
                }
            }
        }
    };
    let (p, sv, weak) = (prefs.clone(), save_prefs.clone(), ui.as_weak());
    store.on_set_game_mode(move |on| {
        p.borrow_mut().game_mode = on;
        sv();
        if let Some(ui) = weak.upgrade() {
            ui.global::<Store>().set_game_mode(on);
        }
    });
    let (p, sv, weak) = (prefs.clone(), save_prefs.clone(), ui.as_weak());
    store.on_set_game_list(move |list| {
        let games = gamemode::parse_list(&list).join(", ");
        p.borrow_mut().games = if games.is_empty() { gamemode::DEFAULT_GAMES.into() } else { games };
        sv();
        if let Some(ui) = weak.upgrade() {
            ui.global::<Store>().set_game_list(p.borrow().games.clone().into());
        }
    });
    let (p, sv, weak) = (prefs.clone(), save_prefs.clone(), ui.as_weak());
    store.on_set_game_rate(move |i| {
        let Some(r) = RATES.get(i as usize) else { return };
        p.borrow_mut().game_rate_hz = r.0;
        sv();
        if let Some(ui) = weak.upgrade() {
            ui.global::<Store>().set_game_rate_index(i);
        }
    });
    let (p, sv, weak) = (prefs.clone(), save_prefs.clone(), ui.as_weak());
    store.on_set_game_sensor(move |i| {
        if i < 0 || i as usize >= SENSOR_MODES.len() {
            return;
        }
        p.borrow_mut().game_sensor_mode = i as u8;
        sv();
        if let Some(ui) = weak.upgrade() {
            ui.global::<Store>().set_game_sensor_mode(i);
        }
    });

    // ---- frameless window
    let weak = ui.as_weak();
    ui.on_minimize(move || {
        if let Some(ui) = weak.upgrade() {
            ui.window().set_minimized(true);
        }
    });
    let weak = ui.as_weak();
    ui.on_close_app(move || {
        if let Some(ui) = weak.upgrade() {
            ui.global::<Store>().set_recording(false);
            ui.hide().ok();
        }
    });
    let weak = ui.as_weak();
    ui.on_drag(move |dx, dy| {
        if let Some(ui) = weak.upgrade() {
            let w = ui.window();
            let (p, s) = (w.position(), w.scale_factor());
            w.set_position(slint::PhysicalPosition::new(p.x + (dx * s) as i32, p.y + (dy * s) as i32));
        }
    });

    // ---- tray
    let menu = Menu::new();
    let open = MenuItem::new(t("Open panel"), true, None);
    let refresh = MenuItem::new(t("Refresh now"), true, None);
    let quit = MenuItem::new(t("Quit"), true, None);
    menu.append_items(&[&open, &refresh, &quit])?;
    let tray = Rc::new(
        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(t("A9 Ultimate: reading..."))
            .with_icon(tray::battery_icon(None, false))
            .build()?,
    );
    let show_window = |ui: &App| {
        ui.show().ok();
        ui.window().set_minimized(false);
    };

    // ---- language switch: Slint texts change by themselves, Rust-built ones are rebuilt here
    let (p, st2, pk, weak, tr2) = (prefs.clone(), st.clone(), picker.clone(), ui.as_weak(), tray.clone());
    let (open2, refresh2, quit2) = (open.clone(), refresh.clone(), quit.clone());
    store.on_set_language(move |i| {
        let Some(&(code, _)) = i18n::LANGUAGES.get(i as usize) else { return };
        i18n::set(code);
        let _ = slint::select_bundled_translation(code);
        p.borrow_mut().language = code.to_string();
        let _ = settings::save(&p.borrow());
        *pk.borrow_mut() = keys::picker();
        open2.set_text(t("Open panel"));
        refresh2.set_text(t("Refresh now"));
        quit2.set_text(t("Quit"));
        let g = st2.borrow();
        let _ = tr2.set_tooltip(Some(tooltip(g.last_battery)));
        if let Some(ui) = weak.upgrade() {
            let s = ui.global::<Store>();
            s.set_language(i);
            fill_lists(&s, &pk.borrow());
            if let Some(state) = &g.last_state {
                show_state(&s, state);
            }
            show_macro(&s, &g.macro_events);
        }
    });

    // ---- bridge between the mouse thread, the tray and the window (all on the main thread)
    let weak = ui.as_weak();
    let (open_id, refresh_id, quit_id) = (open.id().clone(), refresh.id().clone(), quit.id().clone());
    let (q, st2) = (queue.clone(), st.clone());
    let set_battery = move |tray: &tray_icon::TrayIcon, b: Option<Battery>| {
        let _ = tray.set_tooltip(Some(tooltip(b)));
        if let Some(b) = b {
            let _ = tray.set_icon(Some(tray::battery_icon(Some(b.level), b.charging)));
        }
    };
    let pump = slint::Timer::default();
    pump.start(slint::TimerMode::Repeated, Duration::from_millis(60), move || {
        let Some(ui) = weak.upgrade() else { return };
        let s = ui.global::<Store>();
        while let Ok(msg) = msg_rx.try_recv() {
            match msg {
                Msg::Progress(p) => s.set_progress(p),
                Msg::Loaded(Ok(state)) => {
                    show_state(&s, &state);
                    set_battery(&tray, state.battery);
                    s.set_loading(false);
                    let mut g = st2.borrow_mut();
                    if !g.sticky {
                        s.set_status("".into());
                    }
                    g.last_battery = state.battery.or(g.last_battery);
                    g.last_state = Some(state);
                }
                Msg::Loaded(Err(e)) => {
                    s.set_loading(false);
                    s.set_connected(false);
                    s.set_status(t(&e).into());
                    set_battery(&tray, None);
                }
                Msg::Battery(Ok(b)) => {
                    s.set_battery(b.level as i32);
                    s.set_charging(b.charging);
                    set_battery(&tray, Some(b));
                    st2.borrow_mut().last_battery = Some(b);
                }
                Msg::Battery(Err(_)) => set_battery(&tray, None),
                Msg::Applied(Ok((label, is_macro))) => {
                    let msg: SharedString = tf("Saved: {}", label).into();
                    s.set_status(msg.clone());
                    // the notice goes away by itself, like the Hub's toasts
                    let (w, st3) = (ui.as_weak(), st2.clone());
                    slint::Timer::single_shot(Duration::from_secs(4), move || {
                        if let Some(ui) = w.upgrade() {
                            let s = ui.global::<Store>();
                            if s.get_status() == msg {
                                s.set_status("".into());
                                st3.borrow_mut().sticky = false;
                            }
                        }
                    });
                    s.set_undo_count(backup::count() as i32);
                    if is_macro {
                        s.set_macro_dirty(false);
                    }
                }
                Msg::Applied(Err(e)) => {
                    s.set_status(tf("Error: {}", t(&e)).into());
                    s.set_undo_count(backup::count() as i32);
                    // reload so the screen goes back to the mouse's real values
                    q(Job::Load { overlay: false });
                }
                Msg::Macro(slot, res) => {
                    if s.get_macro_slot() as usize == slot {
                        let mut g = st2.borrow_mut();
                        g.last_key = None;
                        match res {
                            Ok(ev) => g.macro_events = ev,
                            Err(e) => {
                                s.set_status(t(&e).into());
                                g.macro_events.clear();
                            }
                        }
                        show_macro(&s, &g.macro_events);
                        s.set_macro_dirty(false);
                    }
                }
                Msg::Done => {
                    let mut g = st2.borrow_mut();
                    g.pending = g.pending.saturating_sub(1);
                    s.set_busy(g.pending > 0);
                }
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == open_id {
                show_window(&ui);
            } else if ev.id == refresh_id {
                q(Job::Load { overlay: false });
            } else if ev.id == quit_id {
                slint::quit_event_loop().ok();
            }
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_window(&ui);
            }
        }
    });

    // game mode: every 5 s check the process list; switch settings while a game runs, put them back after
    let (p, sv, st2, a, weak) = (prefs.clone(), save_prefs.clone(), st.clone(), apply.clone(), ui.as_weak());
    let watcher = slint::Timer::default();
    watcher.start(slint::TimerMode::Repeated, Duration::from_secs(5), move || {
        let (enabled, restore, games, rate, mode) = {
            let p = p.borrow();
            (p.game_mode, p.restore, p.games.clone(), p.game_rate_hz, p.game_sensor_mode)
        };
        if !enabled && restore.is_none() {
            return;
        }
        let game = if enabled { gamemode::running(&gamemode::parse_list(&games)) } else { None };
        let set_active = |name: &str| {
            if let Some(ui) = weak.upgrade() {
                ui.global::<Store>().set_game_active(name.into());
            }
        };
        match (game, restore) {
            (Some(game), None) => {
                let Some(current) = st2.borrow().last_state.clone() else { return };
                p.borrow_mut().restore = Some((current.rate_hz, current.sensor_mode));
                sv();
                let name = game.trim_end_matches(".exe").to_string();
                if current.rate_hz != rate {
                    a(Change::Rate(rate), format!("{}: {}", tf("game mode: {}", &name), tf("polling {} Hz", rate)));
                }
                if current.sensor_mode != mode {
                    a(Change::SensorMode(mode), format!("{}: {}", tf("game mode: {}", &name), t(SENSOR_MODES[mode as usize % 3])));
                }
                set_active(&name);
            }
            (None, Some((old_rate, old_mode))) => {
                p.borrow_mut().restore = None;
                sv();
                if let Some(current) = st2.borrow().last_state.clone() {
                    if current.rate_hz != old_rate && rate_code(old_rate).is_some() {
                        a(Change::Rate(old_rate), format!("{}: {}", t("game closed"), tf("polling {} Hz", old_rate)));
                    }
                    if current.sensor_mode != old_mode {
                        a(Change::SensorMode(old_mode), format!("{}: {}", t("game closed"), t(SENSOR_MODES[old_mode as usize % 3])));
                    }
                }
                set_active("");
            }
            _ => {}
        }
    });

    // battery every minute, without getting in the way if something is queued
    let (q, st2) = (queue.clone(), st.clone());
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_secs(60), move || {
        if st2.borrow().pending == 0 {
            q(Job::Battery);
        }
    });

    queue(Job::Load { overlay: true });
    queue(Job::ReadMacro(0));
    let hidden = std::env::args().any(|a| a == "--tray") || prefs.borrow().start_in_tray;
    // screen checks only: --page=N opens straight on a page
    if let Some(n) = std::env::args().find_map(|a| a.strip_prefix("--page=").and_then(|n| n.parse().ok())) {
        ui.set_page(n);
    }
    // screen checks only: --shot=file.bmp opens off-screen, takes a picture and exits
    if let Some(path) = std::env::args().find_map(|a| a.strip_prefix("--shot=").map(String::from)) {
        ui.window().set_position(slint::PhysicalPosition::new(-5000, 0));
        ui.show()?;
        let weak = ui.as_weak();
        slint::Timer::single_shot(Duration::from_secs(16), move || {
            if let Some(ui) = weak.upgrade() {
                if let Ok(px) = ui.window().take_snapshot() {
                    let _ = std::fs::write(&path, bmp(px.width(), px.height(), px.as_bytes()));
                }
            }
            slint::quit_event_loop().ok();
        });
    } else if !hidden {
        ui.show()?;
    }
    slint::run_event_loop_until_quit()?;
    Ok(())
}

/// 32-bit BMP from RGBA pixels (only for --shot).
fn bmp(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let size = 54 + rgba.len() as u32;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    for v in [size, 0, 54, 40] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for p in rgba.chunks(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    out
}
