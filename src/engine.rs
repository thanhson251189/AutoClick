use crate::capture;
use crate::eval::eval_truth;
use crate::model::{ActionKind, MouseBtn, Script};
use crate::vision;
use enigo::{Button, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct LogLine {
    pub time: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Running,
    Paused,
}

pub struct Engine {
    pub state: Arc<Mutex<RunState>>,
    pub stop: Arc<AtomicBool>,
    pub pause: Arc<AtomicBool>,
    pub current: Arc<AtomicUsize>,
    pub log: Arc<Mutex<Vec<LogLine>>>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RunState::Idle)),
            stop: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            current: Arc::new(AtomicUsize::new(0)),
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn snapshot_state(&self) -> RunState {
        *self.state.lock().unwrap()
    }

    pub fn current_index(&self) -> usize {
        self.current.load(Ordering::Relaxed)
    }

    pub fn logs(&self) -> Vec<LogLine> {
        self.log.lock().unwrap().clone()
    }

    pub fn clear_log(&self) {
        self.log.lock().unwrap().clear();
    }

    fn push_log(&self, text: impl Into<String>) {
        let now = chrono::Local::now().format("%H:%M:%S").to_string();
        self.log.lock().unwrap().push(LogLine {
            time: now,
            text: text.into(),
        });
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.pause.store(false, Ordering::Relaxed);
    }

    pub fn toggle_pause(&self) {
        let cur = self.pause.load(Ordering::Relaxed);
        self.pause.store(!cur, Ordering::Relaxed);
        if let Ok(mut s) = self.state.lock() {
            *s = if cur {
                RunState::Running
            } else {
                RunState::Paused
            };
        }
    }

    pub fn play(
        &self,
        script: Script,
        speed: f32,
        times: u32,
        duration_secs: Option<u64>,
        script_dir: Option<std::path::PathBuf>,
    ) {
        if self.snapshot_state() != RunState::Idle {
            return;
        }
        self.stop.store(false, Ordering::Relaxed);
        self.pause.store(false, Ordering::Relaxed);
        *self.state.lock().unwrap() = RunState::Running;
        self.clear_log();
        self.push_log("Play started");

        let state = self.state.clone();
        let stop = self.stop.clone();
        let pause = self.pause.clone();
        let current = self.current.clone();
        let log = self.log.clone();

        thread::spawn(move || {
            let start = Instant::now();
            let mut round = 0u32;
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if let Some(d) = duration_secs {
                    if start.elapsed().as_secs() >= d {
                        break;
                    }
                } else if times > 0 && round >= times {
                    break;
                }
                round += 1;
                {
                    let mut lg = log.lock().unwrap();
                    lg.push(LogLine {
                        time: chrono::Local::now().format("%H:%M:%S").to_string(),
                        text: format!("Round {}", round),
                    });
                }
                if !run_once(
                    &script,
                    speed,
                    &stop,
                    &pause,
                    &current,
                    &log,
                    script_dir.as_deref(),
                ) {
                    break;
                }
                if times == 1 && duration_secs.is_none() {
                    break;
                }
            }
            current.store(0, Ordering::Relaxed);
            *state.lock().unwrap() = RunState::Idle;
            log.lock().unwrap().push(LogLine {
                time: chrono::Local::now().format("%H:%M:%S").to_string(),
                text: "Play finished".into(),
            });
        });
    }
}

fn wait_pause(pause: &AtomicBool, stop: &AtomicBool) -> bool {
    while pause.load(Ordering::Relaxed) {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        thread::sleep(Duration::from_millis(30));
    }
    !stop.load(Ordering::Relaxed)
}

fn sleep_scaled(ms: u64, speed: f32, pause: &AtomicBool, stop: &AtomicBool) -> bool {
    if ms == 0 {
        return wait_pause(pause, stop);
    }
    let adj = ((ms as f32) / speed.max(0.05)) as u64;
    let start = Instant::now();
    while start.elapsed().as_millis() < adj as u128 {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        if !wait_pause(pause, stop) {
            return false;
        }
        thread::sleep(Duration::from_millis(8));
    }
    true
}

fn fail_safe(enigo: &Enigo) -> bool {
    #[cfg(windows)]
    {
        if let Ok((cx, cy)) = enigo.location() {
            return cx <= 2 && cy <= 2;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enigo;
    }
    false
}

fn run_once(
    script: &Script,
    speed: f32,
    stop: &AtomicBool,
    pause: &AtomicBool,
    current: &AtomicUsize,
    log: &Mutex<Vec<LogLine>>,
    script_dir: Option<&std::path::Path>,
) -> bool {
    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => e,
        Err(err) => {
            log.lock().unwrap().push(LogLine {
                time: chrono::Local::now().format("%H:%M:%S").to_string(),
                text: format!("Enigo init failed: {err}"),
            });
            return false;
        }
    };
    let mut vars: HashMap<String, String> = HashMap::new();
    let actions = &script.actions;
    let n = actions.len();
    let mut labels: HashMap<String, usize> = HashMap::new();
    for (i, a) in actions.iter().enumerate() {
        if let ActionKind::Label { name } = &a.kind {
            labels.insert(name.clone(), i);
        }
    }
    let mut skip_depth: i32 = 0;
    let mut i = 0usize;
    let mut for_stack: Vec<(usize, String, i64, i64)> = Vec::new();
    let mut while_stack: Vec<usize> = Vec::new();

    while i < n {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        if !wait_pause(pause, stop) {
            return false;
        }
        current.store(i, Ordering::Relaxed);
        if fail_safe(&enigo) {
            log.lock().unwrap().push(LogLine {
                time: chrono::Local::now().format("%H:%M:%S").to_string(),
                text: "Fail-safe: top-left corner — stop".into(),
            });
            return false;
        }
        let a = &actions[i];
        if !a.enabled {
            i += 1;
            continue;
        }

        match &a.kind {
            ActionKind::If { expr } => {
                if skip_depth > 0 {
                    skip_depth += 1;
                } else if !eval_truth(expr, &vars) {
                    skip_depth = 1;
                }
            }
            ActionKind::Else => {
                if skip_depth == 1 {
                    skip_depth = 0;
                } else if skip_depth == 0 {
                    skip_depth = 1;
                }
            }
            ActionKind::EndIf => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                }
            }
            ActionKind::EndFor => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                } else if let Some((start, var, next, to)) = for_stack.pop() {
                    if next <= to {
                        vars.insert(var.clone(), next.to_string());
                        for_stack.push((start, var, next + 1, to));
                        i = start + 1;
                        continue;
                    }
                }
            }
            ActionKind::EndWhile => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                } else if let Some(start) = while_stack.last().copied() {
                    if let ActionKind::While { expr } = &actions[start].kind {
                        if eval_truth(expr, &vars) {
                            i = start + 1;
                            continue;
                        }
                    }
                    while_stack.pop();
                }
            }
            ActionKind::For {
                var,
                from,
                to,
                step,
            } => {
                if skip_depth > 0 {
                    skip_depth += 1;
                } else {
                    let st = *step;
                    let nxt = *from + if st == 0 { 1 } else { st };
                    vars.insert(var.clone(), from.to_string());
                    if (*from <= *to && st >= 0) || (*from >= *to && st < 0) {
                        for_stack.push((i, var.clone(), nxt, *to));
                    } else {
                        skip_depth = 1;
                    }
                }
            }
            ActionKind::While { expr } => {
                if skip_depth > 0 {
                    skip_depth += 1;
                } else if !eval_truth(expr, &vars) {
                    skip_depth = 1;
                } else {
                    while_stack.push(i);
                }
            }
            _ => {
                if skip_depth > 0 {
                    i += 1;
                    continue;
                }
                if !exec_action(
                    &mut enigo, a, &mut vars, &labels, &mut i, speed, pause, stop, log, script_dir,
                ) {
                    return false;
                }
            }
        }

        if a.delay_ms > 0 && skip_depth == 0 && !sleep_scaled(a.delay_ms, speed, pause, stop) {
            return false;
        }
        i += 1;
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn exec_action(
    enigo: &mut Enigo,
    a: &crate::model::Action,
    vars: &mut HashMap<String, String>,
    labels: &HashMap<String, usize>,
    i: &mut usize,
    speed: f32,
    pause: &AtomicBool,
    stop: &AtomicBool,
    log: &Mutex<Vec<LogLine>>,
    script_dir: Option<&std::path::Path>,
) -> bool {
    let push = |t: String| {
        log.lock().unwrap().push(LogLine {
            time: chrono::Local::now().format("%H:%M:%S").to_string(),
            text: t,
        });
    };
    match &a.kind {
        ActionKind::FunctionEntry | ActionKind::EndFunction | ActionKind::Comment { .. } => {}
        ActionKind::Delay { ms } => {
            if !sleep_scaled(*ms, speed, pause, stop) {
                return false;
            }
        }
        ActionKind::MouseMove { x, y } => {
            let _ = enigo.move_mouse(*x, *y, enigo::Coordinate::Abs);
        }
        ActionKind::MouseClick {
            button,
            x,
            y,
            clicks,
        } => {
            let _ = enigo.move_mouse(*x, *y, enigo::Coordinate::Abs);
            let btn = map_btn(*button);
            for _ in 0..(*clicks).max(1) {
                let _ = enigo.button(btn, Direction::Click);
                thread::sleep(Duration::from_millis(40));
            }
        }
        ActionKind::MouseDrag {
            button,
            x1,
            y1,
            x2,
            y2,
        } => {
            let btn = map_btn(*button);
            let _ = enigo.move_mouse(*x1, *y1, enigo::Coordinate::Abs);
            let _ = enigo.button(btn, Direction::Press);
            thread::sleep(Duration::from_millis(30));
            let _ = enigo.move_mouse(*x2, *y2, enigo::Coordinate::Abs);
            thread::sleep(Duration::from_millis(30));
            let _ = enigo.button(btn, Direction::Release);
        }
        ActionKind::MouseWheel { delta } => {
            let _ = enigo.scroll(*delta, enigo::Axis::Vertical);
        }
        ActionKind::TypeText { text, interval_ms } => {
            for ch in text.chars() {
                if stop.load(Ordering::Relaxed) {
                    return false;
                }
                let _ = enigo.text(&ch.to_string());
                if *interval_ms > 0 {
                    thread::sleep(Duration::from_millis(*interval_ms));
                }
            }
        }
        ActionKind::KeyPress { key } => {
            send_combo(enigo, key);
        }
        ActionKind::MouseDown { button, x, y } => {
            let _ = enigo.move_mouse(*x, *y, enigo::Coordinate::Abs);
            let _ = enigo.button(map_btn(*button), Direction::Press);
        }
        ActionKind::MouseUp { button, x, y } => {
            let _ = enigo.move_mouse(*x, *y, enigo::Coordinate::Abs);
            let _ = enigo.button(map_btn(*button), Direction::Release);
        }
        ActionKind::SmartClick {
            x,
            y,
            image,
            timeout_ms,
            confidence,
            on_fail,
            ox,
            oy,
        } => {
            if image.is_empty() {
                push(format!("Smart Click ({x}, {y}) no image — skip"));
            } else if let Some(hit) = wait_match(
                image,
                Some((*x, *y)),
                *timeout_ms,
                *confidence,
                script_dir,
                stop,
                false,
            ) {
                let cx = ox.unwrap_or(hit.2 / 2) + hit.0;
                let cy = oy.unwrap_or(hit.3 / 2) + hit.1;
                let _ = enigo.move_mouse(cx, cy, enigo::Coordinate::Abs);
                let _ = enigo.button(Button::Left, Direction::Click);
                push(format!("Smart Click match @ {cx},{cy}"));
            } else {
                push(format!("Smart Click no match `{image}`"));
                if on_fail.eq_ignore_ascii_case("stop") {
                    return false;
                }
            }
        }
        ActionKind::SearchPicture {
            image,
            timeout_ms,
            save_x,
            save_y,
            confidence,
            on_fail,
        } => {
            if let Some(hit) = wait_match(
                image,
                None,
                *timeout_ms,
                *confidence,
                script_dir,
                stop,
                true,
            ) {
                vars.insert("found".into(), "true".into());
                vars.insert(save_x.clone(), hit.0.to_string());
                vars.insert(save_y.clone(), hit.1.to_string());
                vars.insert("found_x".into(), hit.0.to_string());
                vars.insert("found_y".into(), hit.1.to_string());
                push(format!("Search Picture found @ {},{}", hit.0, hit.1));
            } else {
                vars.insert("found".into(), "notfound".into());
                push(format!("Search Picture not found `{image}`"));
                if on_fail.eq_ignore_ascii_case("stop") {
                    return false;
                }
            }
        }
        ActionKind::WaitTime { hh, mm } => {
            use chrono::Timelike;
            let now = chrono::Local::now();
            let target = (*hh as i64) * 3600 + (*mm as i64) * 60;
            let cur = now.hour() as i64 * 3600 + now.minute() as i64 * 60 + now.second() as i64;
            if target <= cur {
                push("WaitTime missed".into());
            } else {
                let deadline = Instant::now() + Duration::from_secs((target - cur) as u64);
                while Instant::now() < deadline {
                    if stop.load(Ordering::Relaxed) {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(200));
                }
            }
        }
        ActionKind::RandomNumber { name, a, b } => {
            let lo = (*a).min(*b);
            let hi = (*a).max(*b);
            let span = (hi - lo + 1).max(1);
            let tick = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(1);
            let n = lo + (tick % span as u128) as i64;
            vars.insert(name.clone(), n.to_string());
            push(format!("Random {name} = {n}"));
        }
        ActionKind::Command { cmd } => {
            if !cmd.is_empty() {
                #[cfg(windows)]
                {
                    let _ = Command::new("cmd").args(["/C", cmd]).status();
                }
                #[cfg(not(windows))]
                {
                    let _ = Command::new("sh").args(["-c", cmd]).status();
                }
            }
        }
        ActionKind::ActivateWindow { title } => {
            let ok = window_command(title, false);
            push(format!(
                "Activate `{title}`: {}",
                if ok { "ok" } else { "sent" }
            ));
        }
        ActionKind::CloseWindow { title } => {
            let ok = window_command(title, true);
            push(format!(
                "Close `{title}`: {}",
                if ok { "ok" } else { "sent" }
            ));
        }
        ActionKind::OpenFile { path } | ActionKind::OpenFolder { path } => {
            open_path(path);
        }
        ActionKind::OpenUrl { url } => {
            open_path(url);
        }
        ActionKind::SetVar { name, value } => {
            vars.insert(name.clone(), value.clone());
        }
        ActionKind::Label { .. } => {}
        ActionKind::Goto { name } => {
            if let Some(idx) = labels.get(name) {
                *i = *idx;
            }
        }
        ActionKind::MessageBox { text } => {
            push(format!("MessageBox: {text}"));
        }
        ActionKind::CallFunction { name } => {
            push(format!("Call {name}"));
        }
        ActionKind::PlayScript { path } => {
            push(format!("Play script {path}"));
        }
        _ => {}
    }
    true
}

fn wait_match(
    image: &str,
    prefer: Option<(i32, i32)>,
    timeout_ms: u64,
    confidence: f32,
    script_dir: Option<&std::path::Path>,
    stop: &AtomicBool,
    full_search: bool,
) -> Option<(i32, i32, i32, i32)> {
    let path = vision::resolve_image(image, script_dir)?;
    let tmpl = vision::load_bmp24(&path)?;
    let once = vision::match_try_once(timeout_ms);
    let deadline = Instant::now() + Duration::from_millis(if once { 0 } else { timeout_ms });
    let mut pad = vision::smart_search_pad(tmpl.w, tmpl.h);
    loop {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        let t0 = Instant::now();
        if let Some((px, py)) = prefer {
            let x0 = (px - pad).max(0);
            let y0 = (py - pad).max(0);
            if let Some(local) = capture::grab_rect(x0, y0, tmpl.w + pad * 2, tmpl.h + pad * 2) {
                if let Some(hit) =
                    vision::find_template(&local, &tmpl, confidence, Some((px - x0, py - y0)))
                {
                    return Some((hit.0 + x0, hit.1 + y0, hit.2, hit.3));
                }
            }
        }
        if full_search {
            if let Some(screen) = capture::grab_screen() {
                if let Some(hit) = vision::find_template(&screen, &tmpl, confidence, prefer) {
                    return Some(hit);
                }
            }
        }
        if once || Instant::now() >= deadline {
            return None;
        }
        pad = (pad + pad / 2).min(320);
        let used = t0.elapsed();
        if used < Duration::from_millis(8) {
            thread::sleep(Duration::from_millis(8) - used);
        }
    }
}

fn map_btn(b: MouseBtn) -> Button {
    match b {
        MouseBtn::Left => Button::Left,
        MouseBtn::Right => Button::Right,
        MouseBtn::Middle => Button::Middle,
    }
}

fn send_combo(enigo: &mut Enigo, combo: &str) {
    let parts: Vec<&str> = combo
        .split('+')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return;
    }
    let mut held: Vec<Key> = Vec::new();
    for (idx, p) in parts.iter().enumerate() {
        let last = idx + 1 == parts.len();
        if let Some(k) = parse_key(p) {
            if last {
                let _ = enigo.key(k, Direction::Click);
            } else {
                let _ = enigo.key(k, Direction::Press);
                held.push(k);
            }
        } else if last && p.chars().count() == 1 {
            let _ = enigo.text(p);
        }
    }
    for k in held.into_iter().rev() {
        let _ = enigo.key(k, Direction::Release);
    }
}

fn parse_key(s: &str) -> Option<Key> {
    Some(match s.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Key::Control,
        "alt" => Key::Alt,
        "shift" => Key::Shift,
        "meta" | "win" | "cmd" => Key::Meta,
        "enter" | "return" => Key::Return,
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Escape,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        _ => return None,
    })
}

fn window_command(title: &str, close: bool) -> bool {
    if title.trim().is_empty() {
        return false;
    }
    let safe = title.replace('\'', "''").replace('"', "");
    #[cfg(target_os = "windows")]
    {
        let script = if close {
            format!(
                "$w = Get-Process | Where-Object {{ $_.MainWindowTitle -like '*{safe}*' }} | Select-Object -First 1; if ($w) {{ $w.CloseMainWindow() | Out-Null }}"
            )
        } else {
            format!("(New-Object -ComObject WScript.Shell).AppActivate('{safe}')")
        };
        Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .spawn()
            .is_ok()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (close, safe);
        false
    }
}

fn open_path(path: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/C", "start", "", path]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(path).spawn();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(path).spawn();
    }
}
