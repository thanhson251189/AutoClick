use crate::hotkeys::{bindings_from_options, Combo};
use crate::model::{Action, ActionKind, MouseBtn};
use rdev::{listen, Button, Event, EventType};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

/// The recorder callback runs inside the OS input hook: a panic there cannot
/// unwind (it aborts the process), so locks must survive a poisoned state.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct Recorder {
    running: Arc<AtomicBool>,
    hook_started: Arc<AtomicBool>,
    first_event: Arc<AtomicBool>,
    /// Global hotkey combos, so playback never re-triggers them (Ctrl+P pause).
    skip: Arc<Mutex<Vec<Combo>>>,
    tx: Sender<Action>,
    rx: Receiver<Action>,
}

impl Recorder {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            running: Arc::new(AtomicBool::new(false)),
            hook_started: Arc::new(AtomicBool::new(false)),
            first_event: Arc::new(AtomicBool::new(true)),
            skip: Arc::new(Mutex::new(Vec::new())),
            tx,
            rx,
        }
    }

    /// Keys bound to app hotkeys must not be recorded as steps: replaying them
    /// would fire our own global hotkeys mid-run.
    pub fn set_skip_hotkeys(&self, opt: &crate::model::AppOptions) {
        let combos: Vec<Combo> = bindings_from_options(opt)
            .into_iter()
            .filter_map(|(_, s)| Combo::parse(&s))
            .collect();
        *lock(&self.skip) = combos;
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Same gate the OS listen callback uses: after `stop`, events are dropped.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn emit(&self, action: Action) {
        gated_send(&self.running, &self.tx, action);
    }

    pub fn start(&mut self, sample_ms: u64, ignore_px: i32) {
        self.start_inner(sample_ms, ignore_px, true);
    }

    /// Install the OS hook before the first F9 so early keystrokes are not lost.
    pub fn warm_hook(&mut self) {
        self.ensure_hook(30, 2);
    }

    fn start_inner(&mut self, sample_ms: u64, ignore_px: i32, spawn_hook: bool) {
        if self.running.swap(true, Ordering::Relaxed) {
            return;
        }
        self.first_event.store(true, Ordering::Relaxed);
        if spawn_hook {
            self.ensure_hook(sample_ms, ignore_px);
        }
    }

    fn ensure_hook(&mut self, sample_ms: u64, ignore_px: i32) {
        if self.hook_started.swap(true, Ordering::Relaxed) {
            return;
        }
        let flag = self.running.clone();
        let hook_started = self.hook_started.clone();
        let tx = self.tx.clone();
        let skip = self.skip.clone();
        let ctrl = Arc::new(AtomicBool::new(false));
        let alt = Arc::new(AtomicBool::new(false));
        let shift = Arc::new(AtomicBool::new(false));
        let first_event = self.first_event.clone();
        thread::spawn(move || {
            let last_move = std::sync::Mutex::new(((0i32, 0i32), Instant::now()));
            let last_ev = std::sync::Mutex::new(None::<Instant>);
            let tracker = std::sync::Mutex::new(PressTracker::default());
            let callback = move |event: Event| {
                if !flag.load(Ordering::Relaxed) {
                    return;
                }
                let delay = {
                    let now = Instant::now();
                    let mut g = lock(&last_ev);
                    let d = if first_event.swap(false, Ordering::Relaxed) {
                        0
                    } else {
                        inter_event_delay(*g, now)
                    };
                    *g = Some(now);
                    d
                };
                match event.event_type {
                    EventType::MouseMove { x, y } => {
                        let xi = x as i32;
                        let yi = y as i32;
                        let mut guard = lock(&last_move);
                        let (lp, t) = *guard;
                        let tiny = (xi - lp.0).abs() < ignore_px && (yi - lp.1).abs() < ignore_px;
                        let soon = t.elapsed() < Duration::from_millis(sample_ms.max(10));
                        // Always keep the latest pointer so a click is not stuck at (0,0).
                        if tiny || soon {
                            guard.0 = (xi, yi);
                            drop(guard);
                        } else {
                            *guard = ((xi, yi), Instant::now());
                            drop(guard);
                        }
                        // While a button is down the move only feeds drag
                        // detection; the path is replayed by MouseDrag itself.
                        {
                            let mut tr = lock(&tracker);
                            if tr.pending.is_some() {
                                tr.on_move(xi, yi);
                                return;
                            }
                        }
                        if tiny || soon {
                            return;
                        }
                        gated_send(
                            &flag,
                            &tx,
                            Action::new(ActionKind::MouseMove {
                                x: xi,
                                y: yi,
                                ms: 0,
                            })
                            .with_delay(delay),
                        );
                    }
                    EventType::ButtonPress(btn) => {
                        let Some(button) = map_button(btn) else {
                            return;
                        };
                        let tracked = lock(&last_move).0;
                        let pos = click_xy(cursor_pos(), tracked);
                        lock(&last_move).0 = pos;
                        // Nothing is sent yet: click vs drag is decided on release.
                        lock(&tracker).on_press(pos.0, pos.1, button, delay);
                    }
                    EventType::ButtonRelease(btn) => {
                        let Some(button) = map_button(btn) else {
                            return;
                        };
                        let tracked = lock(&last_move).0;
                        let pos = click_xy(cursor_pos(), tracked);
                        lock(&last_move).0 = pos;
                        if let Some((kind, delay)) = lock(&tracker).on_release(pos.0, pos.1, button)
                        {
                            gated_send(&flag, &tx, Action::new(kind).with_delay(delay));
                        }
                    }
                    EventType::Wheel { delta_y, .. } => {
                        if delta_y != 0 {
                            gated_send(
                                &flag,
                                &tx,
                                Action::new(ActionKind::MouseWheel {
                                    delta: delta_y as i32,
                                })
                                .with_delay(delay),
                            );
                        }
                    }
                    EventType::KeyPress(k) => {
                        if set_modifier(&ctrl, &alt, &shift, k, true) {
                            return;
                        }
                        let name = key_name(k);
                        let skipped = {
                            let skip = lock(&skip);
                            skip.iter().any(|c| {
                                c.matches(
                                    ctrl.load(Ordering::Relaxed),
                                    shift.load(Ordering::Relaxed),
                                    alt.load(Ordering::Relaxed),
                                    name.as_deref().unwrap_or(""),
                                )
                            })
                        };
                        if skipped {
                            return;
                        }
                        if let Some(kind) = recorded_from_key(
                            name,
                            event.name.clone(),
                            ctrl.load(Ordering::Relaxed),
                            alt.load(Ordering::Relaxed),
                            shift.load(Ordering::Relaxed),
                        ) {
                            gated_send(&flag, &tx, Action::new(kind).with_delay(delay));
                        }
                    }
                    EventType::KeyRelease(k) => {
                        set_modifier(&ctrl, &alt, &shift, k, false);
                    }
                }
            };
            if let Err(e) = listen(callback) {
                eprintln!("recorder listen error: {e:?}");
                hook_started.store(false, Ordering::Relaxed);
            }
        });
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }

    pub fn drain(&mut self) -> Vec<Action> {
        let mut out = Vec::new();
        while let Ok(a) = self.rx.try_recv() {
            out.push(a);
        }
        merge_typed(out)
    }
}

/// Insert a recorded action, collapsing a multi-click run (click, double,
/// triple…) into one action even when the presses arrive in different drain
/// batches. Returns true when the action was inserted, false when it replaced
/// the previous click of the same run.
pub(crate) fn insert_recorded(actions: &mut Vec<Action>, at: usize, a: Action) -> bool {
    if let ActionKind::MouseClick {
        clicks,
        x,
        y,
        button,
    } = a.kind
    {
        if clicks >= 2 {
            if let Some(prev) = at.checked_sub(1).and_then(|i| actions.get(i)) {
                if let ActionKind::MouseClick {
                    clicks: pc,
                    x: px,
                    y: py,
                    button: pb,
                } = prev.kind
                {
                    if px == x && py == y && pb == button && u16::from(pc) + 1 == u16::from(clicks)
                    {
                        let merged = Action::new(ActionKind::MouseClick {
                            button,
                            x,
                            y,
                            clicks,
                        })
                        .with_delay(prev.delay_ms);
                        actions[at - 1] = merged;
                        return false;
                    }
                }
            }
        }
    }
    actions.insert(at.min(actions.len()), a);
    true
}

/// Gap from the previous recorded event to this one (0 for the first).
pub(crate) fn inter_event_delay(prev: Option<Instant>, now: Instant) -> u64 {
    match prev {
        None => 0,
        Some(t) => now.saturating_duration_since(t).as_millis() as u64,
    }
}

/// AMK-like double-click window and drag threshold.
const DBL_CLICK_MS: u64 = 350;
const DRAG_MIN_PX: i32 = 5;

struct PendingPress {
    x: i32,
    y: i32,
    button: MouseBtn,
    run: u8,
    delay: u64,
    moved: bool,
}

/// Click/drag state machine: a press is only classified when the button comes
/// back up — no movement makes a (multi-)click, movement makes a drag. This is
/// what lets the recorder capture drags like AMK instead of dropping them.
#[derive(Default)]
struct PressTracker {
    last_click: Option<(i32, i32, Instant, u8)>,
    pending: Option<PendingPress>,
}

impl PressTracker {
    fn on_press(&mut self, x: i32, y: i32, button: MouseBtn, delay: u64) {
        let run = match self.last_click {
            Some((lx, ly, t, c))
                if lx == x && ly == y && t.elapsed() < Duration::from_millis(DBL_CLICK_MS) =>
            {
                c.saturating_add(1)
            }
            _ => 1,
        };
        self.last_click = Some((x, y, Instant::now(), run));
        self.pending = Some(PendingPress {
            x,
            y,
            button,
            run,
            delay,
            moved: false,
        });
    }

    fn on_move(&mut self, x: i32, y: i32) {
        if let Some(p) = self.pending.as_mut() {
            if (x - p.x).abs() > DRAG_MIN_PX || (y - p.y).abs() > DRAG_MIN_PX {
                p.moved = true;
            }
        }
    }

    fn on_release(&mut self, x: i32, y: i32, button: MouseBtn) -> Option<(ActionKind, u64)> {
        let p = self.pending.take()?;
        if p.button != button {
            // Chorded buttons: keep the pending press for its real release.
            self.pending = Some(p);
            return None;
        }
        let kind = if p.moved {
            ActionKind::MouseDrag {
                button: p.button,
                x1: p.x,
                y1: p.y,
                x2: x,
                y2: y,
                ms: 0,
            }
        } else {
            ActionKind::MouseClick {
                button: p.button,
                x: p.x,
                y: p.y,
                clicks: p.run,
            }
        };
        Some((kind, p.delay))
    }
}

fn merge_typed(cleaned: Vec<Action>) -> Vec<Action> {
    let mut merged: Vec<Action> = Vec::new();
    for a in cleaned {
        if let ActionKind::TypeText { text, interval_ms } = &a.kind {
            if let Some(prev) = merged.last_mut() {
                if let ActionKind::TypeText {
                    text: p,
                    interval_ms: pi,
                } = &mut prev.kind
                {
                    let gap = if a.delay_ms > 0 {
                        a.delay_ms
                    } else {
                        *interval_ms
                    };
                    if *pi == 0 {
                        *pi = gap;
                    } else {
                        // `pi` averages the n-1 gaps already inside `p`.
                        let n = p.chars().count().max(1) as u64;
                        *pi = (*pi * (n - 1) + gap) / n;
                    }
                    p.push_str(text);
                    continue;
                }
            }
        }
        merged.push(a);
    }
    merged
}

/// Prefer a live cursor sample; fall back to the last mouse-move we saw.
pub(crate) fn click_xy(cursor: Option<(i32, i32)>, last_move: (i32, i32)) -> (i32, i32) {
    cursor.unwrap_or(last_move)
}

fn cursor_pos() -> Option<(i32, i32)> {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct Point {
            x: i32,
            y: i32,
        }
        #[link(name = "user32")]
        extern "system" {
            fn GetCursorPos(pt: *mut Point) -> i32;
        }
        unsafe {
            let mut pt = Point { x: 0, y: 0 };
            if GetCursorPos(&mut pt) != 0 {
                return Some((pt.x, pt.y));
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn gated_send(running: &AtomicBool, tx: &Sender<Action>, action: Action) {
    if running.load(Ordering::Relaxed) {
        let _ = tx.send(action);
    }
}

/// None for the side mouse buttons (X1/X2): the model has no slot for them and
/// replaying them as left clicks would click the wrong target.
fn map_button(b: Button) -> Option<MouseBtn> {
    match b {
        Button::Left => Some(MouseBtn::Left),
        Button::Right => Some(MouseBtn::Right),
        Button::Middle => Some(MouseBtn::Middle),
        _ => None,
    }
}

fn set_modifier(
    ctrl: &AtomicBool,
    alt: &AtomicBool,
    shift: &AtomicBool,
    k: rdev::Key,
    down: bool,
) -> bool {
    use rdev::Key::*;
    match k {
        ControlLeft | ControlRight => {
            ctrl.store(down, Ordering::Relaxed);
            true
        }
        Alt | AltGr => {
            alt.store(down, Ordering::Relaxed);
            true
        }
        ShiftLeft | ShiftRight => {
            shift.store(down, Ordering::Relaxed);
            true
        }
        _ => false,
    }
}

fn is_printable_text(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| !c.is_control())
}

/// Turn a physical key + optional Unicode name into a script action.
pub(crate) fn recorded_from_key(
    key: Option<String>,
    printed: Option<String>,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> Option<ActionKind> {
    if let Some(name) = key.as_deref() {
        // Do not store the global control keys as steps.
        if matches!(name, "F7" | "F8" | "F9" | "F10" | "F12") && !ctrl && !alt {
            return None;
        }
    }
    if ctrl || alt {
        let name = key?;
        let mut s = String::new();
        if ctrl {
            s.push_str("Ctrl+");
        }
        if alt {
            s.push_str("Alt+");
        }
        if shift {
            s.push_str("Shift+");
        }
        s.push_str(&name);
        return Some(ActionKind::KeyPress { key: s });
    }
    if let Some(text) = printed.filter(|t| is_printable_text(t)) {
        return Some(ActionKind::TypeText {
            text,
            interval_ms: 0,
        });
    }
    Some(ActionKind::KeyPress { key: key? })
}

fn key_name(k: rdev::Key) -> Option<String> {
    use rdev::Key::*;
    Some(match k {
        KeyA => "A".into(),
        KeyB => "B".into(),
        KeyC => "C".into(),
        KeyD => "D".into(),
        KeyE => "E".into(),
        KeyF => "F".into(),
        KeyG => "G".into(),
        KeyH => "H".into(),
        KeyI => "I".into(),
        KeyJ => "J".into(),
        KeyK => "K".into(),
        KeyL => "L".into(),
        KeyM => "M".into(),
        KeyN => "N".into(),
        KeyO => "O".into(),
        KeyP => "P".into(),
        KeyQ => "Q".into(),
        KeyR => "R".into(),
        KeyS => "S".into(),
        KeyT => "T".into(),
        KeyU => "U".into(),
        KeyV => "V".into(),
        KeyW => "W".into(),
        KeyX => "X".into(),
        KeyY => "Y".into(),
        KeyZ => "Z".into(),
        Num0 => "0".into(),
        Num1 => "1".into(),
        Num2 => "2".into(),
        Num3 => "3".into(),
        Num4 => "4".into(),
        Num5 => "5".into(),
        Num6 => "6".into(),
        Num7 => "7".into(),
        Num8 => "8".into(),
        Num9 => "9".into(),
        Escape => "Escape".into(),
        Return => "Enter".into(),
        Tab => "Tab".into(),
        Space => "Space".into(),
        Backspace => "Backspace".into(),
        Delete => "Delete".into(),
        ControlLeft | ControlRight => "Ctrl".into(),
        Alt | AltGr => "Alt".into(),
        ShiftLeft | ShiftRight => "Shift".into(),
        MetaLeft | MetaRight => "Win".into(),
        UpArrow => "Up".into(),
        DownArrow => "Down".into(),
        LeftArrow => "Left".into(),
        RightArrow => "Right".into(),
        F1 => "F1".into(),
        F2 => "F2".into(),
        F3 => "F3".into(),
        F4 => "F4".into(),
        F5 => "F5".into(),
        F6 => "F6".into(),
        F7 => "F7".into(),
        F8 => "F8".into(),
        F9 => "F9".into(),
        F10 => "F10".into(),
        F11 => "F11".into(),
        F12 => "F12".into(),
        Home => "Home".into(),
        End => "End".into(),
        PageUp => "PageUp".into(),
        PageDown => "PageDown".into(),
        CapsLock => "CapsLock".into(),
        Minus => "-".into(),
        Equal => "=".into(),
        LeftBracket => "[".into(),
        RightBracket => "]".into(),
        SemiColon => ";".into(),
        Quote => "'".into(),
        BackSlash => "\\".into(),
        IntlBackslash => "\\".into(),
        BackQuote => "`".into(),
        Comma => ",".into(),
        Dot => ".".into(),
        Slash => "/".into(),
        Insert => "Insert".into(),
        PrintScreen => "PrintScreen".into(),
        ScrollLock => "ScrollLock".into(),
        Pause => "Pause".into(),
        NumLock => "NumLock".into(),
        KpReturn => "Enter".into(),
        KpMinus => "Num-".into(),
        KpPlus => "Num+".into(),
        KpMultiply => "Num*".into(),
        KpDivide => "Num/".into(),
        Kp0 => "Num0".into(),
        Kp1 => "Num1".into(),
        Kp2 => "Num2".into(),
        Kp3 => "Num3".into(),
        Kp4 => "Num4".into(),
        Kp5 => "Num5".into(),
        Kp6 => "Num6".into(),
        Kp7 => "Num7".into(),
        Kp8 => "Num8".into(),
        Kp9 => "Num9".into(),
        KpDelete => "NumDel".into(),
        Function => "Fn".into(),
        Unknown(code) => format!("VK{code}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_drops_further_listen_events() {
        let mut rec = Recorder::new();
        rec.start_inner(30, 2, false);
        assert!(rec.is_running());
        rec.emit(Action::new(ActionKind::MouseMove {
            x: 10,
            y: 20,
            ms: 0,
        }));
        assert_eq!(rec.drain().len(), 1);

        rec.stop();
        assert!(!rec.is_running());
        rec.emit(Action::new(ActionKind::MouseMove {
            x: 30,
            y: 40,
            ms: 0,
        }));
        rec.emit(Action::new(ActionKind::KeyPress { key: "A".into() }));
        assert!(
            rec.drain().is_empty(),
            "listen path must not append after stop"
        );
    }

    #[test]
    fn start_is_idempotent_while_running() {
        let mut rec = Recorder::new();
        rec.start_inner(30, 2, false);
        rec.start_inner(30, 2, false);
        assert!(rec.is_running());
        rec.stop();
        assert!(!rec.is_running());
        rec.start_inner(30, 2, false);
        assert!(rec.is_running());
        rec.emit(Action::new(ActionKind::MouseWheel { delta: 1 }));
        assert_eq!(rec.drain().len(), 1);
        rec.stop();
    }

    #[test]
    fn click_uses_live_cursor_not_origin() {
        assert_eq!(click_xy(Some((800, 600)), (0, 0)), (800, 600));
        assert_eq!(click_xy(None, (12, 34)), (12, 34));
    }

    #[test]
    fn records_letters_and_punctuation() {
        assert_eq!(key_name(rdev::Key::KeyA).as_deref(), Some("A"));
        assert_eq!(key_name(rdev::Key::Minus).as_deref(), Some("-"));
        assert_eq!(key_name(rdev::Key::Unknown(123)).as_deref(), Some("VK123"));
    }

    #[test]
    fn ctrl_letter_becomes_combo() {
        let k = recorded_from_key(Some("C".into()), Some("c".into()), true, false, false);
        match k {
            Some(ActionKind::KeyPress { key }) => assert_eq!(key, "Ctrl+C"),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn plain_char_becomes_type_text() {
        let k = recorded_from_key(Some("A".into()), Some("a".into()), false, false, false);
        match k {
            Some(ActionKind::TypeText { text, .. }) => assert_eq!(text, "a"),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn first_event_has_zero_delay_later_events_keep_gap() {
        let t0 = Instant::now();
        assert_eq!(inter_event_delay(None, t0), 0);
        let t1 = t0 + Duration::from_millis(250);
        assert_eq!(inter_event_delay(Some(t0), t1), 250);
        let t2 = t1 + Duration::from_millis(80);
        assert_eq!(inter_event_delay(Some(t1), t2), 80);
    }

    #[test]
    fn merge_typed_keeps_inter_key_interval() {
        let a = Action::new(ActionKind::TypeText {
            text: "h".into(),
            interval_ms: 0,
        })
        .with_delay(400);
        let b = Action::new(ActionKind::TypeText {
            text: "i".into(),
            interval_ms: 0,
        })
        .with_delay(120);
        let out = merge_typed(vec![a, b]);
        assert_eq!(out.len(), 1);
        match &out[0].kind {
            ActionKind::TypeText { text, interval_ms } => {
                assert_eq!(text, "hi");
                assert_eq!(*interval_ms, 120);
            }
            other => panic!("{:?}", other),
        }
        assert_eq!(out[0].delay_ms, 400);
    }

    #[test]
    fn merge_typed_averages_the_real_gap_count() {
        let mk = |t: &str, d: u64| {
            Action::new(ActionKind::TypeText {
                text: t.into(),
                interval_ms: 0,
            })
            .with_delay(d)
        };
        // Gaps 120 and 300 must average to 210, not (120*2 + 300)/3 = 180.
        let out = merge_typed(vec![mk("h", 400), mk("i", 120), mk("e", 300)]);
        match &out[0].kind {
            ActionKind::TypeText { text, interval_ms } => {
                assert_eq!(text, "hie");
                assert_eq!(*interval_ms, 210);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn side_mouse_buttons_are_not_recorded_as_left_clicks() {
        assert_eq!(map_button(rdev::Button::Left), Some(MouseBtn::Left));
        assert_eq!(map_button(rdev::Button::Right), Some(MouseBtn::Right));
        assert_eq!(map_button(rdev::Button::Middle), Some(MouseBtn::Middle));
        assert_eq!(map_button(rdev::Button::Unknown(1)), None);
        assert_eq!(map_button(rdev::Button::Unknown(2)), None);
    }

    #[test]
    fn press_release_without_move_is_a_click() {
        let mut tr = PressTracker::default();
        tr.on_press(10, 20, MouseBtn::Left, 120);
        let (kind, delay) = tr.on_release(11, 21, MouseBtn::Left).expect("click");
        match kind {
            ActionKind::MouseClick {
                button,
                x,
                y,
                clicks,
            } => assert_eq!((button, x, y, clicks), (MouseBtn::Left, 10, 20, 1)),
            other => panic!("{other:?}"),
        }
        assert_eq!(delay, 120);
    }

    #[test]
    fn moved_press_release_is_a_drag() {
        let mut tr = PressTracker::default();
        tr.on_press(10, 10, MouseBtn::Left, 30);
        tr.on_move(60, 15);
        let (kind, _) = tr.on_release(100, 40, MouseBtn::Left).expect("drag");
        match kind {
            ActionKind::MouseDrag {
                button,
                x1,
                y1,
                x2,
                y2,
                ms: _,
            } => assert_eq!((button, x1, y1, x2, y2), (MouseBtn::Left, 10, 10, 100, 40)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn tiny_wiggle_stays_a_click_not_a_drag() {
        let mut tr = PressTracker::default();
        tr.on_press(10, 10, MouseBtn::Left, 0);
        tr.on_move(13, 12); // 3px / 2px — below the 5px drag threshold
        let (kind, _) = tr.on_release(14, 12, MouseBtn::Left).expect("click");
        assert!(matches!(kind, ActionKind::MouseClick { .. }));
    }

    #[test]
    fn quick_second_press_counts_as_double_click() {
        let mut tr = PressTracker::default();
        tr.on_press(5, 5, MouseBtn::Left, 0);
        let (kind, _) = tr.on_release(5, 5, MouseBtn::Left).expect("click");
        assert!(matches!(kind, ActionKind::MouseClick { clicks: 1, .. }));
        tr.on_press(5, 5, MouseBtn::Left, 50);
        let (kind, _) = tr.on_release(5, 5, MouseBtn::Left).expect("double");
        assert!(matches!(kind, ActionKind::MouseClick { clicks: 2, .. }));
    }

    #[test]
    fn chorded_release_keeps_pending_for_its_own_button() {
        let mut tr = PressTracker::default();
        tr.on_press(1, 1, MouseBtn::Left, 0);
        assert!(tr.on_release(1, 1, MouseBtn::Right).is_none());
        let (kind, _) = tr.on_release(1, 1, MouseBtn::Left).expect("left click");
        assert!(matches!(kind, ActionKind::MouseClick { .. }));
    }

    #[test]
    fn multi_click_run_collapses_across_drain_batches() {
        let mut actions = Vec::new();
        let mk = |clicks: u8| {
            Action::new(ActionKind::MouseClick {
                button: MouseBtn::Left,
                x: 10,
                y: 20,
                clicks,
            })
            .with_delay(500)
        };
        assert!(insert_recorded(&mut actions, 0, mk(1)));
        // The second press lands in a later drain batch: it must replace the
        // single click, not play 1 + 2 = 3 clicks.
        assert!(!insert_recorded(&mut actions, 1, mk(2)));
        assert_eq!(actions.len(), 1);
        match &actions[0].kind {
            ActionKind::MouseClick { clicks, .. } => assert_eq!(*clicks, 2),
            other => panic!("{other:?}"),
        }
        assert_eq!(actions[0].delay_ms, 500);
        // A third press upgrades the action to a triple click.
        assert!(!insert_recorded(&mut actions, 1, mk(3)));
        match &actions[0].kind {
            ActionKind::MouseClick { clicks, .. } => assert_eq!(*clicks, 3),
            other => panic!("{other:?}"),
        }
        assert_eq!(actions[0].delay_ms, 500);
    }

    #[test]
    fn insert_recorded_keeps_other_rows_and_clamps_the_index() {
        let mut actions = vec![Action::new(ActionKind::Delay { ms: 1 })];
        let a = Action::new(ActionKind::MouseClick {
            button: MouseBtn::Left,
            x: 1,
            y: 1,
            clicks: 2,
        })
        .with_delay(5);
        // Out-of-range index (empty script after delete-all) must clamp, not panic.
        assert!(insert_recorded(&mut actions, 9, a));
        assert_eq!(actions.len(), 2);
        match &actions[1].kind {
            ActionKind::MouseClick { clicks, .. } => assert_eq!(*clicks, 2),
            other => panic!("{other:?}"),
        }
        // A different position between the presses blocks the merge.
        let mut actions = vec![mk_click(1, 1, 1), Action::new(ActionKind::Delay { ms: 1 })];
        assert!(insert_recorded(&mut actions, 2, mk_click(1, 1, 2)));
        assert_eq!(actions.len(), 3);
    }

    fn mk_click(x: i32, y: i32, clicks: u8) -> Action {
        Action::new(ActionKind::MouseClick {
            button: MouseBtn::Left,
            x,
            y,
            clicks,
        })
    }
}
