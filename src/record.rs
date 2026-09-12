use crate::model::{Action, ActionKind, MouseBtn};
use rdev::{listen, Button, Event, EventType};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub struct Recorder {
    running: Arc<AtomicBool>,
    tx: Sender<Action>,
    rx: Receiver<Action>,
}

impl Recorder {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            running: Arc::new(AtomicBool::new(false)),
            tx,
            rx,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn start(&mut self, sample_ms: u64, ignore_px: i32) {
        if self.running.load(Ordering::Relaxed) {
            return;
        }
        self.running.store(true, Ordering::Relaxed);
        let flag = self.running.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let last_move = std::sync::Mutex::new(((0i32, 0i32), Instant::now()));
            let last_click = std::sync::Mutex::new(None::<(i32, i32, Instant)>);
            let callback = move |event: Event| {
                if !flag.load(Ordering::Relaxed) {
                    return;
                }
                match event.event_type {
                    EventType::MouseMove { x, y } => {
                        let xi = x as i32;
                        let yi = y as i32;
                        let mut guard = last_move.lock().unwrap();
                        let (lp, t) = *guard;
                        if (xi - lp.0).abs() < ignore_px && (yi - lp.1).abs() < ignore_px {
                            return;
                        }
                        if t.elapsed() < Duration::from_millis(sample_ms.max(10)) {
                            return;
                        }
                        *guard = ((xi, yi), Instant::now());
                        let _ = tx.send(Action::new(ActionKind::MouseMove { x: xi, y: yi }));
                    }
                    EventType::ButtonPress(btn) => {
                        // position unknown from this event on some backends; last move used
                        let pos = last_move.lock().unwrap().0;
                        let button = map_button(btn);
                        let mut lc = last_click.lock().unwrap();
                        let is_dbl = if let Some((x, y, t)) = *lc {
                            x == pos.0 && y == pos.1 && t.elapsed() < Duration::from_millis(350)
                        } else {
                            false
                        };
                        *lc = Some((pos.0, pos.1, Instant::now()));
                        let clicks = if is_dbl { 2 } else { 1 };
                        let _ = tx.send(
                            Action::new(ActionKind::MouseClick {
                                button,
                                x: pos.0,
                                y: pos.1,
                                clicks,
                            })
                            .with_delay(0),
                        );
                    }
                    EventType::Wheel { delta_y, .. } => {
                        if delta_y != 0 {
                            let _ = tx.send(Action::new(ActionKind::MouseWheel {
                                delta: delta_y as i32,
                            }));
                        }
                    }
                    EventType::KeyPress(k) => {
                        if let Some(name) = key_name(k) {
                            let _ = tx.send(Action::new(ActionKind::KeyPress { key: name }));
                        }
                    }
                    _ => {}
                }
            };
            if let Err(e) = listen(callback) {
                eprintln!("recorder listen error: {e:?}");
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
        // collapse consecutive double-click artifacts: click + double
        let mut cleaned: Vec<Action> = Vec::new();
        for a in out {
            if let ActionKind::MouseClick {
                clicks: 2,
                x,
                y,
                button,
            } = a.kind
            {
                if let Some(prev) = cleaned.last() {
                    if let ActionKind::MouseClick {
                        clicks: 1,
                        x: px,
                        y: py,
                        button: pb,
                    } = prev.kind
                    {
                        if px == x && py == y && pb == button {
                            cleaned.pop();
                        }
                    }
                }
                cleaned.push(Action::new(ActionKind::MouseClick {
                    button,
                    x,
                    y,
                    clicks: 2,
                }));
            } else {
                cleaned.push(a);
            }
        }
        cleaned
    }
}

fn map_button(b: Button) -> MouseBtn {
    match b {
        Button::Right => MouseBtn::Right,
        Button::Middle => MouseBtn::Middle,
        _ => MouseBtn::Left,
    }
}

fn key_name(k: rdev::Key) -> Option<String> {
    use rdev::Key::*;
    Some(
        match k {
            KeyA => "A",
            KeyB => "B",
            KeyC => "C",
            KeyD => "D",
            KeyE => "E",
            KeyF => "F",
            KeyG => "G",
            KeyH => "H",
            KeyI => "I",
            KeyJ => "J",
            KeyK => "K",
            KeyL => "L",
            KeyM => "M",
            KeyN => "N",
            KeyO => "O",
            KeyP => "P",
            KeyQ => "Q",
            KeyR => "R",
            KeyS => "S",
            KeyT => "T",
            KeyU => "U",
            KeyV => "V",
            KeyW => "W",
            KeyX => "X",
            KeyY => "Y",
            KeyZ => "Z",
            Num0 => "0",
            Num1 => "1",
            Num2 => "2",
            Num3 => "3",
            Num4 => "4",
            Num5 => "5",
            Num6 => "6",
            Num7 => "7",
            Num8 => "8",
            Num9 => "9",
            Escape => "Escape",
            Return => "Enter",
            Tab => "Tab",
            Space => "Space",
            Backspace => "Backspace",
            Delete => "Delete",
            ControlLeft | ControlRight => "Ctrl",
            Alt | AltGr => "Alt",
            ShiftLeft | ShiftRight => "Shift",
            MetaLeft | MetaRight => "Win",
            UpArrow => "Up",
            DownArrow => "Down",
            LeftArrow => "Left",
            RightArrow => "Right",
            F1 => "F1",
            F2 => "F2",
            F3 => "F3",
            F4 => "F4",
            F5 => "F5",
            F6 => "F6",
            F7 => "F7",
            F8 => "F8",
            F9 => "F9",
            F10 => "F10",
            F11 => "F11",
            F12 => "F12",
            Home => "Home",
            End => "End",
            PageUp => "PageUp",
            PageDown => "PageDown",
            _ => return None,
        }
        .to_string(),
    )
}
