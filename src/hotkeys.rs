//! Global hotkeys (Windows RegisterHotKey). Defaults:
//! F9 record on/off, F10 play/pause, F12 stop, Ctrl+P pause,
//! F7 step into, F8 step over.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::model::AppOptions;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotCmd {
    PlayOrPause,
    Stop,
    ToggleRecord,
    Pause,
    StepInto,
    StepOver,
}

const ID_PLAY: i32 = 1;
const ID_STOP: i32 = 2;
const ID_RECORD: i32 = 3;
const ID_PAUSE: i32 = 4;
const ID_STEP_IN: i32 = 5;
const ID_STEP_OVER: i32 = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Combo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: String,
}

impl Combo {
    pub fn parse(s: &str) -> Option<Self> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut key: Option<String> = None;
        for part in s.split('+') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            match p.to_ascii_uppercase().as_str() {
                "CTRL" | "CONTROL" => ctrl = true,
                "SHIFT" => shift = true,
                "ALT" => alt = true,
                other => {
                    if key.is_some() {
                        return None;
                    }
                    key = Some(normalize_key(other));
                }
            }
        }
        let key = key?;
        if key.is_empty() {
            return None;
        }
        Some(Self {
            ctrl,
            shift,
            alt,
            key,
        })
    }

    pub fn matches(&self, ctrl: bool, shift: bool, alt: bool, key: &str) -> bool {
        self.ctrl == ctrl
            && self.shift == shift
            && self.alt == alt
            && self.key.eq_ignore_ascii_case(&normalize_key(key))
    }

    #[cfg(windows)]
    fn win_mod_vk(&self) -> Option<(u32, u32)> {
        let vk = vk_of(&self.key)?;
        let mut mods: u32 = MOD_NOREPEAT;
        if self.ctrl {
            mods |= MOD_CONTROL;
        }
        if self.shift {
            mods |= MOD_SHIFT;
        }
        if self.alt {
            mods |= MOD_ALT;
        }
        Some((mods, vk))
    }
}

fn normalize_key(s: &str) -> String {
    let u = s.to_ascii_uppercase();
    match u.as_str() {
        "RETURN" => "ENTER".into(),
        "ESCAPE" => "ESC".into(),
        "CONTROL" => "CTRL".into(),
        other => other.to_string(),
    }
}

pub fn bindings_from_options(opt: &AppOptions) -> [(HotCmd, String); 6] {
    [
        (HotCmd::PlayOrPause, opt.hk_play.clone()),
        (HotCmd::Stop, opt.hk_stop.clone()),
        (HotCmd::ToggleRecord, opt.hk_record.clone()),
        (HotCmd::Pause, opt.hk_pause.clone()),
        (HotCmd::StepInto, opt.hk_step_into.clone()),
        (HotCmd::StepOver, opt.hk_step_over.clone()),
    ]
}

pub struct Hotkeys {
    rx: Receiver<HotCmd>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    /// Which of the 6 AMK commands were registered as OS-global hotkeys.
    global: [bool; 6],
}

impl Hotkeys {
    pub fn start(opt: &AppOptions) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let mut global = [false; 6];
        let join = spawn_listener(opt, tx, stop.clone(), &mut global);
        Self {
            rx,
            stop,
            join,
            global,
        }
    }

    pub fn poll(&self) -> Vec<HotCmd> {
        let mut out = Vec::new();
        while let Ok(c) = self.rx.try_recv() {
            out.push(c);
        }
        out
    }

    pub fn is_global(&self, cmd: HotCmd) -> bool {
        self.global[cmd_index(cmd)]
    }

    pub fn rebind(&mut self, opt: &AppOptions) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
        let (tx, rx) = mpsc::channel();
        self.rx = rx;
        self.stop = Arc::new(AtomicBool::new(false));
        let mut global = [false; 6];
        self.join = spawn_listener(opt, tx, self.stop.clone(), &mut global);
        self.global = global;
    }
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn cmd_index(cmd: HotCmd) -> usize {
    match cmd {
        HotCmd::PlayOrPause => 0,
        HotCmd::Stop => 1,
        HotCmd::ToggleRecord => 2,
        HotCmd::Pause => 3,
        HotCmd::StepInto => 4,
        HotCmd::StepOver => 5,
    }
}

fn cmd_from_id(id: i32) -> Option<HotCmd> {
    Some(match id {
        ID_PLAY => HotCmd::PlayOrPause,
        ID_STOP => HotCmd::Stop,
        ID_RECORD => HotCmd::ToggleRecord,
        ID_PAUSE => HotCmd::Pause,
        ID_STEP_IN => HotCmd::StepInto,
        ID_STEP_OVER => HotCmd::StepOver,
        _ => return None,
    })
}

fn spawn_listener(
    opt: &AppOptions,
    tx: Sender<HotCmd>,
    stop: Arc<AtomicBool>,
    global: &mut [bool; 6],
) -> Option<JoinHandle<()>> {
    #[cfg(windows)]
    {
        let binds = bindings_from_options(opt);
        let parsed: Vec<(HotCmd, i32, Combo)> = binds
            .iter()
            .filter_map(|(cmd, s)| {
                let combo = Combo::parse(s)?;
                let id = match cmd {
                    HotCmd::PlayOrPause => ID_PLAY,
                    HotCmd::Stop => ID_STOP,
                    HotCmd::ToggleRecord => ID_RECORD,
                    HotCmd::Pause => ID_PAUSE,
                    HotCmd::StepInto => ID_STEP_IN,
                    HotCmd::StepOver => ID_STEP_OVER,
                };
                Some((*cmd, id, combo))
            })
            .collect();
        let parsed = dedupe_combos(parsed);
        // We cannot know register success until the thread runs; publish the
        // result array through a one-shot flag so the UI never waits longer
        // than the registration itself takes.
        let ok = Arc::new(std::sync::Mutex::new([false; 6]));
        let published = Arc::new(AtomicBool::new(false));
        let ok_t = ok.clone();
        let pub_t = published.clone();
        let handle = thread::spawn(move || {
            windows_hotkey_thread(parsed, tx, stop, ok_t, pub_t);
        });
        // Wait only until the thread publishes (RegisterHotKey is fast); the
        // 500 ms ceiling remains for the pathological case.
        for _ in 0..50 {
            if published.load(Ordering::Relaxed) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        if let Ok(g) = ok.lock() {
            *global = *g;
        }
        Some(handle)
    }
    #[cfg(not(windows))]
    {
        let _ = (opt, tx, stop, global);
        None
    }
}

#[cfg(windows)]
const MOD_ALT: u32 = 0x0001;
#[cfg(windows)]
const MOD_CONTROL: u32 = 0x0002;
#[cfg(windows)]
const MOD_SHIFT: u32 = 0x0004;
#[cfg(windows)]
const MOD_NOREPEAT: u32 = 0x4000;
#[cfg(windows)]
const WM_HOTKEY: u32 = 0x0312;
#[cfg(windows)]
const PM_REMOVE: u32 = 0x0001;

#[cfg(windows)]
#[repr(C)]
struct Msg {
    hwnd: *mut core::ffi::c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn RegisterHotKey(hwnd: *mut core::ffi::c_void, id: i32, fs_modifiers: u32, vk: u32) -> i32;
    fn UnregisterHotKey(hwnd: *mut core::ffi::c_void, id: i32) -> i32;
    fn PeekMessageW(
        msg: *mut Msg,
        hwnd: *mut core::ffi::c_void,
        min: u32,
        max: u32,
        remove: u32,
    ) -> i32;
}

#[cfg(windows)]
fn vk_of(key: &str) -> Option<u32> {
    let k = normalize_key(key);
    if k.len() == 1 {
        let c = k.chars().next()?;
        if c.is_ascii_alphanumeric() {
            return Some(c as u32);
        }
    }
    Some(match k.as_str() {
        "F1" => 0x70,
        "F2" => 0x71,
        "F3" => 0x72,
        "F4" => 0x73,
        "F5" => 0x74,
        "F6" => 0x75,
        "F7" => 0x76,
        "F8" => 0x77,
        "F9" => 0x78,
        "F10" => 0x79,
        "F11" => 0x7A,
        "F12" => 0x7B,
        "ESC" => 0x1B,
        "ENTER" => 0x0D,
        "TAB" => 0x09,
        "SPACE" => 0x20,
        "DELETE" | "DEL" => 0x2E,
        _ => return None,
    })
}

/// Two commands on one combo: the second RegisterHotKey would silently fail
/// against our own first registration. Keep the first binding.
#[cfg(windows)]
fn dedupe_combos(parsed: Vec<(HotCmd, i32, Combo)>) -> Vec<(HotCmd, i32, Combo)> {
    let mut seen: Vec<(u32, u32)> = Vec::new();
    parsed
        .into_iter()
        .filter(|(_, _, combo)| {
            if let Some((mods, vk)) = combo.win_mod_vk() {
                let k = (mods & !MOD_NOREPEAT, vk);
                if seen.contains(&k) {
                    return false;
                }
                seen.push(k);
            }
            true
        })
        .collect()
}

#[cfg(windows)]
fn windows_hotkey_thread(
    parsed: Vec<(HotCmd, i32, Combo)>,
    tx: Sender<HotCmd>,
    stop: Arc<AtomicBool>,
    ok: Arc<std::sync::Mutex<[bool; 6]>>,
    published: Arc<AtomicBool>,
) {
    let mut registered: Vec<i32> = Vec::new();
    let mut flags = [false; 6];
    let mut msg = Msg {
        hwnd: std::ptr::null_mut(),
        message: 0,
        wparam: 0,
        lparam: 0,
        time: 0,
        pt_x: 0,
        pt_y: 0,
    };
    // Create this thread's message queue before RegisterHotKey.
    unsafe {
        PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE);
    }
    for (cmd, id, combo) in &parsed {
        if let Some((mods, vk)) = combo.win_mod_vk() {
            let mut used_mods = mods;
            let mut ok_reg =
                unsafe { RegisterHotKey(std::ptr::null_mut(), *id, used_mods, vk) } != 0;
            if !ok_reg && (used_mods & MOD_NOREPEAT) != 0 {
                used_mods &= !MOD_NOREPEAT;
                ok_reg = unsafe { RegisterHotKey(std::ptr::null_mut(), *id, used_mods, vk) } != 0;
            }
            if ok_reg {
                registered.push(*id);
                flags[cmd_index(*cmd)] = true;
            }
        }
    }
    if let Ok(mut g) = ok.lock() {
        *g = flags;
    }
    published.store(true, Ordering::Relaxed);
    while !stop.load(Ordering::Relaxed) {
        unsafe {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_HOTKEY {
                    if let Some(cmd) = cmd_from_id(msg.wparam as i32) {
                        let _ = tx.send(cmd);
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    for id in registered {
        unsafe {
            UnregisterHotKey(std::ptr::null_mut(), id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_amk_default_combos() {
        let f9 = Combo::parse("F9").unwrap();
        assert!(!f9.ctrl && !f9.shift && f9.key == "F9");
        assert!(f9.matches(false, false, false, "F9"));
        assert!(!f9.matches(true, false, false, "F9"));

        let rec = Combo::parse("Shift+F2").unwrap();
        assert!(rec.shift && rec.key == "F2");
        assert!(rec.matches(false, true, false, "F2"));

        let pause = Combo::parse("Ctrl+P").unwrap();
        assert!(pause.ctrl && pause.key == "P");
        assert!(pause.matches(true, false, false, "p"));

        let stop = Combo::parse("F12").unwrap();
        assert!(stop.matches(false, false, false, "F12"));

        let inn = Combo::parse("F7").unwrap();
        assert!(inn.matches(false, false, false, "F7"));
        let over = Combo::parse("F8").unwrap();
        assert!(over.matches(false, false, false, "F8"));
    }

    #[test]
    fn rejects_empty_and_double_keys() {
        assert!(Combo::parse("").is_none());
        assert!(Combo::parse("F9+F10").is_none());
        assert!(Combo::parse("Ctrl+").is_none());
    }

    #[test]
    fn f9_is_record_not_play() {
        let rec = Combo::parse("F9").unwrap();
        let play = Combo::parse("F10").unwrap();
        let stop = Combo::parse("F12").unwrap();
        assert!(rec.matches(false, false, false, "F9"));
        assert!(!rec.matches(false, false, false, "F10"));
        assert_ne!(rec, play);
        assert_ne!(rec, stop);
    }

    #[test]
    fn default_options_bind_f9_to_record() {
        let opt = crate::model::AppOptions::default();
        assert_eq!(opt.hk_record, "F9");
        assert_eq!(opt.hk_play, "F10");
        assert_eq!(opt.hk_stop, "F12");
        let rec = Combo::parse(&opt.hk_record).unwrap();
        assert!(rec.matches(false, false, false, "F9"));
        let binds = bindings_from_options(&opt);
        assert_eq!(binds[0].0, HotCmd::PlayOrPause);
        assert_eq!(binds[0].1, "F10");
        assert_eq!(binds[2].0, HotCmd::ToggleRecord);
        assert_eq!(binds[2].1, "F9");
    }

    #[test]
    fn duplicate_combos_keep_only_the_first_command() {
        let mk = |cmd: HotCmd, s: &str| (cmd, 0, Combo::parse(s).unwrap());
        let parsed = vec![
            mk(HotCmd::PlayOrPause, "F9"),
            mk(HotCmd::ToggleRecord, "F9"),
            mk(HotCmd::Stop, "Ctrl+P"),
            mk(HotCmd::Pause, "ctrl+p"),
        ];
        let out = dedupe_combos(parsed);
        let cmds: Vec<HotCmd> = out.iter().map(|(c, _, _)| *c).collect();
        assert_eq!(cmds, vec![HotCmd::PlayOrPause, HotCmd::Stop]);
    }
}
