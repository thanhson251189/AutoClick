use crate::capture;
use crate::eval::{self, eval_truth};
use crate::model::{Action, ActionKind, MouseBtn, Script};
use crate::vision;
use enigo::{Button, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// Playback outlives individual mutex guards; a poisoned lock must degrade to
/// the guarded data instead of cascading panics across the UI and the engine.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunMode {
    Full,
    #[cfg_attr(not(test), allow(dead_code))]
    Logic,
}

#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct RunReport {
    pub ok: bool,
    pub vars: HashMap<String, String>,
    pub logs: Vec<LogLine>,
}

struct RunEnv<'a> {
    speed: f32,
    stop: &'a AtomicBool,
    pause: &'a AtomicBool,
    current: &'a AtomicUsize,
    log: &'a Mutex<Vec<LogLine>>,
    script_dir: Option<&'a Path>,
    mode: RunMode,
    depth: u32,
    state: Option<&'a Mutex<RunState>>,
    step_once: Option<&'a AtomicBool>,
    step_over: Option<&'a AtomicBool>,
    step_over_floor: Option<&'a AtomicUsize>,
    /// Child scripts parsed once per run (PlayScript inside loops re-reads).
    scripts: &'a RefCell<HashMap<PathBuf, Option<Arc<Script>>>>,
}

/// Keep the log dialog responsive on long Infinite runs.
const LOG_CAP: usize = 1000;

fn push_line(log: &Mutex<Vec<LogLine>>, text: impl Into<String>) {
    let mut log = lock(log);
    log.push(LogLine {
        time: chrono::Local::now().format("%H:%M:%S").to_string(),
        text: text.into(),
    });
    if log.len() > LOG_CAP + 200 {
        let excess = log.len() - LOG_CAP;
        log.drain(0..excess);
    }
}

pub struct Engine {
    pub state: Arc<Mutex<RunState>>,
    pub stop: Arc<AtomicBool>,
    pub pause: Arc<AtomicBool>,
    pub current: Arc<AtomicUsize>,
    pub log: Arc<Mutex<Vec<LogLine>>>,
    pub step_once: Arc<AtomicBool>,
    pub step_over: Arc<AtomicBool>,
    pub step_over_floor: Arc<AtomicUsize>,
    /// Outcome of the last run: false when it aborted (stop, failure, panic).
    last_ok: Arc<AtomicBool>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RunState::Idle)),
            stop: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            current: Arc::new(AtomicUsize::new(0)),
            log: Arc::new(Mutex::new(Vec::new())),
            step_once: Arc::new(AtomicBool::new(false)),
            step_over: Arc::new(AtomicBool::new(false)),
            step_over_floor: Arc::new(AtomicUsize::new(usize::MAX)),
            last_ok: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn snapshot_state(&self) -> RunState {
        *lock(&self.state)
    }

    pub fn current_index(&self) -> usize {
        self.current.load(Ordering::Relaxed)
    }

    pub fn logs(&self) -> Vec<LogLine> {
        lock(&self.log).clone()
    }

    /// Read-only borrow of the log, for rendering without cloning it.
    pub fn with_log<R>(&self, f: impl FnOnce(&[LogLine]) -> R) -> R {
        let g = lock(&self.log);
        f(&g)
    }

    pub fn clear_log(&self) {
        lock(&self.log).clear()
    }

    /// True when the last completed run finished all rounds without aborting.
    pub fn last_run_ok(&self) -> bool {
        self.last_ok.load(Ordering::Relaxed)
    }

    fn push_log(&self, text: impl Into<String>) {
        push_line(&self.log, text);
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.pause.store(false, Ordering::Relaxed);
    }

    pub fn toggle_pause(&self) {
        if self.snapshot_state() == RunState::Idle {
            return;
        }
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

    /// F7: run the current action, then pause (AMK Step Into).
    pub fn request_step_into(&self) {
        if self.snapshot_state() == RunState::Idle {
            return;
        }
        self.step_once.store(true, Ordering::Relaxed);
        self.pause.store(false, Ordering::Relaxed);
        if let Ok(mut s) = self.state.lock() {
            *s = RunState::Running;
        }
    }

    /// F8: run the current action; if it is Call Function, do not pause
    /// until that call returns (AMK Step Over).
    pub fn request_step_over(&self) {
        if self.snapshot_state() == RunState::Idle {
            return;
        }
        self.step_over.store(true, Ordering::Relaxed);
        self.step_over_floor.store(usize::MAX, Ordering::Relaxed);
        self.pause.store(false, Ordering::Relaxed);
        if let Ok(mut s) = self.state.lock() {
            *s = RunState::Running;
        }
    }

    pub fn play(
        &self,
        script: Script,
        speed: f32,
        times: u32,
        duration_secs: Option<u64>,
        script_dir: Option<std::path::PathBuf>,
        start_paused: bool,
    ) {
        if self.snapshot_state() != RunState::Idle {
            return;
        }
        self.stop.store(false, Ordering::Relaxed);
        self.step_once.store(false, Ordering::Relaxed);
        self.step_over.store(false, Ordering::Relaxed);
        self.step_over_floor.store(usize::MAX, Ordering::Relaxed);
        self.pause.store(start_paused, Ordering::Relaxed);
        self.current.store(0, Ordering::Relaxed);
        self.last_ok.store(true, Ordering::Relaxed);
        *lock(&self.state) = if start_paused {
            RunState::Paused
        } else {
            RunState::Running
        };
        self.clear_log();
        self.push_log(if start_paused {
            "Debug run — F7 step into, F8 step over, F12 stop"
        } else {
            "Play started"
        });

        let state = self.state.clone();
        let stop = self.stop.clone();
        let pause = self.pause.clone();
        let current = self.current.clone();
        let log = self.log.clone();
        let step_once = self.step_once.clone();
        let step_over = self.step_over.clone();
        let step_over_floor = self.step_over_floor.clone();
        let last_ok = self.last_ok.clone();

        thread::spawn(move || {
            let start = Instant::now();
            let scripts = RefCell::new(HashMap::new());
            // A panic on this thread would otherwise leave RunState::Running
            // forever (the only Idle reset lives here). Catch it, and report
            // the run as aborted either way.
            let completed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut round = 0u32;
                let mut completed = true;
                loop {
                    if stop.load(Ordering::Relaxed) {
                        completed = false;
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
                    push_line(&log, format!("Round {}", round));
                    let env = RunEnv {
                        speed,
                        stop: &stop,
                        pause: &pause,
                        current: &current,
                        log: &log,
                        script_dir: script_dir.as_deref(),
                        mode: RunMode::Full,
                        depth: 0,
                        state: Some(&state),
                        step_once: Some(&step_once),
                        step_over: Some(&step_over),
                        step_over_floor: Some(&step_over_floor),
                        scripts: &scripts,
                    };
                    let (ok, _) = run_once(&script, &env, &HashMap::new());
                    if !ok {
                        completed = false;
                        break;
                    }
                    if times == 1 && duration_secs.is_none() {
                        break;
                    }
                }
                completed
            }))
            .unwrap_or_else(|_| {
                push_line(&log, "Play aborted by an internal error");
                false
            });
            current.store(0, Ordering::Relaxed);
            last_ok.store(completed, Ordering::Relaxed);
            // Log first: --run drains the log as soon as the state hits Idle.
            push_line(&log, "Play finished");
            *lock(&state) = RunState::Idle;
        });
    }
}

/// Shipped runner without OS mouse/keyboard/GDI. Same control-flow and
/// action dispatch as Play; delays are skipped so tests stay fast.
#[cfg_attr(not(test), allow(dead_code))]
pub fn run_logic(script: &Script) -> RunReport {
    run_logic_in(script, None)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn run_logic_in(script: &Script, script_dir: Option<&Path>) -> RunReport {
    let stop = AtomicBool::new(false);
    let pause = AtomicBool::new(false);
    let current = AtomicUsize::new(0);
    let log = Mutex::new(Vec::new());
    let scripts = RefCell::new(HashMap::new());
    let env = RunEnv {
        speed: 10.0,
        stop: &stop,
        pause: &pause,
        current: &current,
        log: &log,
        script_dir,
        mode: RunMode::Logic,
        depth: 0,
        state: None,
        step_once: None,
        step_over: None,
        step_over_floor: None,
        scripts: &scripts,
    };
    let (ok, vars) = run_once(script, &env, &HashMap::new());
    RunReport {
        ok,
        vars,
        logs: log.into_inner().unwrap_or_default(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StepDecision {
    pause: bool,
    new_floor: Option<usize>,
    clear_step_over: bool,
}

/// AMK F7/F8: after one action, either pause (step into) or stay in a call (step over).
fn step_decision(
    step_once: bool,
    step_over: bool,
    kind_is_call: bool,
    call_depth: usize,
    floor: usize,
) -> StepDecision {
    if step_once {
        return StepDecision {
            pause: true,
            new_floor: None,
            clear_step_over: true,
        };
    }
    if !step_over {
        return StepDecision {
            pause: false,
            new_floor: None,
            clear_step_over: false,
        };
    }
    if kind_is_call && floor == usize::MAX {
        return StepDecision {
            pause: false,
            new_floor: Some(call_depth.saturating_sub(1)),
            clear_step_over: false,
        };
    }
    if call_depth <= floor {
        return StepDecision {
            pause: true,
            new_floor: None,
            clear_step_over: true,
        };
    }
    StepDecision {
        pause: false,
        new_floor: None,
        clear_step_over: false,
    }
}

fn debug_after_action(env: &RunEnv<'_>, kind_is_call: bool, call_depth: usize) {
    if env.mode != RunMode::Full {
        return;
    }
    let step_once = env
        .step_once
        .map(|f| f.load(Ordering::Relaxed))
        .unwrap_or(false);
    let step_over = env
        .step_over
        .map(|f| f.load(Ordering::Relaxed))
        .unwrap_or(false);
    let floor = env
        .step_over_floor
        .map(|f| f.load(Ordering::Relaxed))
        .unwrap_or(usize::MAX);
    let d = step_decision(step_once, step_over, kind_is_call, call_depth, floor);
    if let Some(flag) = env.step_once {
        flag.store(false, Ordering::Relaxed);
    }
    if let Some(nf) = d.new_floor {
        if let Some(slot) = env.step_over_floor {
            slot.store(nf, Ordering::Relaxed);
        }
    }
    if d.clear_step_over {
        if let Some(over) = env.step_over {
            over.store(false, Ordering::Relaxed);
        }
    }
    if d.pause {
        env.pause.store(true, Ordering::Relaxed);
        if let Some(st) = env.state {
            if let Ok(mut s) = st.lock() {
                *s = RunState::Paused;
            }
        }
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

/// Move to `x,y`: `ms == 0` jumps, `ms > 0` glides there in steps
/// (stop/pause-aware). Returns false when the run should stop.
fn glide_or_move(e: &mut Enigo, x: i32, y: i32, ms: u64, env: &RunEnv<'_>) -> bool {
    let mut glided = false;
    if ms > 0 {
        if let Ok((cx, cy)) = e.location() {
            let steps = (ms / 15).clamp(1, 100);
            let step_ms = (ms / steps).max(1);
            for s in 1..=steps {
                let t = s as f32 / steps as f32;
                let nx = cx as f32 + (x - cx) as f32 * t;
                let ny = cy as f32 + (y - cy) as f32 * t;
                let _ = e.move_mouse(nx.round() as i32, ny.round() as i32, enigo::Coordinate::Abs);
                if s < steps && !sleep_scaled(step_ms, env.speed, env.pause, env.stop) {
                    return false;
                }
            }
            glided = true;
        }
    }
    if !glided {
        let _ = e.move_mouse(x, y, enigo::Coordinate::Abs);
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

fn find_fn(actions: &[Action], name: &str) -> Option<usize> {
    let want = name.trim();
    if want.is_empty() {
        return None;
    }
    actions
        .iter()
        .position(|a| matches!(a.kind, ActionKind::FunctionEntry) && a.name.trim() == want)
}

/// Nearest For/While loop around `from`: (opener index, matching End index).
/// If/Else nesting is ignored on purpose: only loops own an End we can jump to.
fn enclosing_loop(actions: &[Action], from: usize) -> Option<(usize, usize)> {
    let mut depth = 0i32;
    let mut opener = None;
    for j in (0..from).rev() {
        match &actions[j].kind {
            ActionKind::For { .. } | ActionKind::While { .. } => {
                if depth == 0 {
                    opener = Some(j);
                    break;
                }
                depth -= 1;
            }
            ActionKind::EndFor | ActionKind::EndWhile => depth += 1,
            _ => {}
        }
    }
    let opener = opener?;
    let mut depth = 0i32;
    for (j, a) in actions.iter().enumerate().skip(opener) {
        match &a.kind {
            ActionKind::For { .. } | ActionKind::While { .. } => depth += 1,
            ActionKind::EndFor | ActionKind::EndWhile => {
                depth -= 1;
                if depth == 0 {
                    return Some((opener, j));
                }
            }
            _ => {}
        }
    }
    None
}

/// Next switch boundary scanning forward from `from`: the matching EndSwitch,
/// or (when `cases`) the next Case/DefaultCase of the same switch.
fn find_switch_boundary(actions: &[Action], from: usize, cases: bool) -> Option<usize> {
    let mut depth = 0i32;
    for (j, a) in actions.iter().enumerate().skip(from) {
        match &a.kind {
            ActionKind::Switch { .. } => depth += 1,
            ActionKind::EndSwitch => {
                if depth == 0 {
                    return Some(j);
                }
                depth -= 1;
            }
            ActionKind::Case { .. } | ActionKind::DefaultCase if depth == 0 && cases => {
                return Some(j);
            }
            _ => {}
        }
    }
    None
}

fn run_once(
    script: &Script,
    env: &RunEnv<'_>,
    seed: &HashMap<String, String>,
) -> (bool, HashMap<String, String>) {
    let mut vars = seed.clone();
    let mut enigo = if env.mode == RunMode::Full {
        match Enigo::new(&Settings::default()) {
            Ok(e) => Some(e),
            Err(err) => {
                push_line(env.log, format!("Enigo init failed: {err}"));
                return (false, vars);
            }
        }
    } else {
        None
    };
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
    // start, var, next_value, to, step
    let mut for_stack: Vec<(usize, String, i64, i64, i64)> = Vec::new();
    // switch value, a case already matched
    let mut switch_stack: Vec<(String, bool)> = Vec::new();
    let mut while_stack: Vec<usize> = Vec::new();
    let mut call_stack: Vec<usize> = Vec::new();
    let mut steps = 0u32;

    while i < n {
        steps += 1;
        if steps > 1_000_000 {
            push_line(
                env.log,
                "Stopped: iteration limit (possible infinite Goto/While)",
            );
            return (false, vars);
        }
        if env.stop.load(Ordering::Relaxed) {
            return (false, vars);
        }
        if env.mode == RunMode::Full && !wait_pause(env.pause, env.stop) {
            return (false, vars);
        }
        env.current.store(i, Ordering::Relaxed);
        if env.mode == RunMode::Full {
            if let Some(e) = enigo.as_ref() {
                if fail_safe(e) {
                    push_line(env.log, "Fail-safe: top-left corner — stop");
                    return (false, vars);
                }
            }
        }
        let a = &actions[i];
        if !a.enabled {
            i += 1;
            continue;
        }

        // delay_ms is the gap before this step (filled by the recorder).
        if a.delay_ms > 0
            && skip_depth == 0
            && env.mode == RunMode::Full
            && !sleep_scaled(a.delay_ms, env.speed, env.pause, env.stop)
        {
            return (false, vars);
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
                } else if let Some((start, var, next, to, st)) = for_stack.pop() {
                    let in_range = if st >= 0 { next <= to } else { next >= to };
                    if in_range {
                        vars.insert(var.clone(), next.to_string());
                        push_line(env.log, format!("For {var}={next}"));
                        // An overflowed continuation is out of range anyway:
                        // dropping the frame ends the loop correctly.
                        if let Some(nxt) = next.checked_add(st) {
                            for_stack.push((start, var, nxt, to, st));
                        }
                        debug_after_action(env, false, call_stack.len());
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
                            debug_after_action(env, false, call_stack.len());
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
                    let st = if *step == 0 { 1 } else { *step };
                    let in_range = if st >= 0 { *from <= *to } else { *from >= *to };
                    if in_range {
                        vars.insert(var.clone(), from.to_string());
                        push_line(env.log, format!("For {var}={from}"));
                        if let Some(nxt) = (*from).checked_add(st) {
                            for_stack.push((i, var.clone(), nxt, *to, st));
                        }
                        // No frame on overflow: the loop body runs once and
                        // the EndFor below falls through.
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
            ActionKind::FunctionEntry => {}
            ActionKind::EndFunction => {
                if skip_depth == 0 {
                    if let Some(ret) = call_stack.pop() {
                        i = ret;
                    } else {
                        return (true, vars);
                    }
                }
            }
            ActionKind::CallFunction { name } => {
                if skip_depth == 0 {
                    // Same variable support as Goto.
                    let name = eval::expand_text(name, &vars);
                    if call_stack.len() >= 32 {
                        push_line(env.log, format!("Call {name}: depth limit"));
                        return (false, vars);
                    }
                    if let Some(idx) = find_fn(actions, &name) {
                        push_line(env.log, format!("Call {name}"));
                        call_stack.push(i);
                        i = idx;
                    } else {
                        push_line(env.log, format!("Call {name}: not found"));
                    }
                }
            }
            ActionKind::Switch { expr } => {
                if skip_depth > 0 {
                    skip_depth += 1;
                } else {
                    let v = eval::eval_value(expr, &vars);
                    switch_stack.push((v, false));
                }
            }
            ActionKind::Case { .. } | ActionKind::DefaultCase => {
                if skip_depth == 0 {
                    let is_default = matches!(a.kind, ActionKind::DefaultCase);
                    let empty = String::new();
                    let case_value = match &a.kind {
                        ActionKind::Case { value } => value,
                        _ => &empty,
                    };
                    match switch_stack.last_mut() {
                        Some((want, matched)) => {
                            if *matched {
                                // An earlier case ran: jump past the rest.
                                if let Some(end) = find_switch_boundary(actions, i + 1, false) {
                                    i = end;
                                    continue;
                                }
                                i = actions.len();
                                continue;
                            }
                            let hit = is_default || case_value.trim() == want.trim();
                            if hit {
                                *matched = true;
                            } else if let Some(next) = find_switch_boundary(actions, i + 1, true) {
                                i = next;
                                continue;
                            } else {
                                // Malformed switch without EndSwitch.
                                i = actions.len();
                                continue;
                            }
                        }
                        None => push_line(env.log, "Case: no Switch"),
                    }
                }
            }
            ActionKind::EndSwitch => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                } else {
                    switch_stack.pop();
                }
            }
            ActionKind::Break | ActionKind::Continue => {
                if skip_depth == 0 {
                    let is_break = matches!(a.kind, ActionKind::Break);
                    match enclosing_loop(actions, i) {
                        Some((opener, end)) => {
                            if is_break {
                                // Drop the loop frame so the End falls through
                                // instead of starting the next iteration.
                                if while_stack.last() == Some(&opener) {
                                    while_stack.pop();
                                } else if for_stack.last().map(|f| f.0) == Some(opener) {
                                    for_stack.pop();
                                }
                            }
                            // Land on the End itself (not past it): the End
                            // drives the loop for Continue, and for Break its
                            // empty stack falls through.
                            i = end;
                            continue;
                        }
                        None => push_line(
                            env.log,
                            if is_break {
                                "Break: no loop"
                            } else {
                                "Continue: no loop"
                            },
                        ),
                    }
                }
            }
            _ => {
                if skip_depth > 0 {
                    i += 1;
                    continue;
                }
                if !exec_action(&mut enigo, a, &mut vars, &labels, &mut i, env) {
                    return (false, vars);
                }
            }
        }

        if skip_depth == 0 {
            debug_after_action(
                env,
                matches!(a.kind, ActionKind::CallFunction { .. }),
                call_stack.len(),
            );
        }
        i += 1;
    }
    (true, vars)
}

fn exec_action(
    enigo: &mut Option<Enigo>,
    a: &Action,
    vars: &mut HashMap<String, String>,
    labels: &HashMap<String, usize>,
    i: &mut usize,
    env: &RunEnv<'_>,
) -> bool {
    let push = |t: String| push_line(env.log, t);
    let live = env.mode == RunMode::Full;
    match &a.kind {
        ActionKind::FunctionEntry | ActionKind::EndFunction | ActionKind::Comment { .. } => {}
        ActionKind::Delay { ms } => {
            if live && !sleep_scaled(*ms, env.speed, env.pause, env.stop) {
                return false;
            }
            let _ = ms;
        }
        ActionKind::MouseMove { x, y, ms } => {
            if let Some(e) = enigo.as_mut() {
                if !glide_or_move(e, *x, *y, *ms, env) {
                    return false;
                }
            }
        }
        ActionKind::MouseClick {
            button,
            x,
            y,
            clicks,
        } => {
            if let Some(e) = enigo.as_mut() {
                let _ = e.move_mouse(*x, *y, enigo::Coordinate::Abs);
                let btn = map_btn(*button);
                for _ in 0..(*clicks).max(1) {
                    let _ = e.button(btn, Direction::Click);
                    // Scaled, stop- and pause-aware: 255 clicks must not pin
                    // the playback thread for 10 blind seconds.
                    if !sleep_scaled(40, env.speed, env.pause, env.stop) {
                        return false;
                    }
                }
            }
        }
        ActionKind::MouseDrag {
            button,
            x1,
            y1,
            x2,
            y2,
            ms,
        } => {
            if let Some(e) = enigo.as_mut() {
                let btn = map_btn(*button);
                let _ = e.move_mouse(*x1, *y1, enigo::Coordinate::Abs);
                let _ = e.button(btn, Direction::Press);
                if !sleep_scaled(30, env.speed, env.pause, env.stop) {
                    return false;
                }
                // ms > 0 drags along a glide like AMK; 0 keeps the old jump.
                if !glide_or_move(e, *x2, *y2, *ms, env) {
                    return false;
                }
                if !sleep_scaled(30, env.speed, env.pause, env.stop) {
                    return false;
                }
                let _ = e.button(btn, Direction::Release);
            }
        }
        ActionKind::MouseWheel { delta } => {
            if let Some(e) = enigo.as_mut() {
                let _ = e.scroll(*delta, enigo::Axis::Vertical);
            }
        }
        ActionKind::TypeText { text, interval_ms } => {
            let text = eval::expand_text(text, vars);
            push(format!("TypeText {text}"));
            if let Some(e) = enigo.as_mut() {
                for ch in text.chars() {
                    if env.stop.load(Ordering::Relaxed) {
                        return false;
                    }
                    // Multiline text: a newline must press Enter — the
                    // Unicode path does not deliver 0x0A on Windows.
                    if ch == '\n' {
                        let _ = e.key(Key::Return, Direction::Click);
                    } else if ch != '\r' {
                        let _ = e.text(&ch.to_string());
                    }
                    if *interval_ms > 0
                        && !sleep_scaled(*interval_ms, env.speed, env.pause, env.stop)
                    {
                        return false;
                    }
                }
            }
        }
        ActionKind::KeyPress { key } => {
            let key = eval::expand_text(key, vars);
            push(format!("Key {key}"));
            if let Some(e) = enigo.as_mut() {
                send_combo(e, &key);
            }
        }
        ActionKind::KeyDown { key } | ActionKind::KeyUp { key } => {
            let down = matches!(a.kind, ActionKind::KeyDown { .. });
            let key = eval::expand_text(key, vars);
            push(format!("Key {} {key}", if down { "Down" } else { "Up" }));
            if let Some(e) = enigo.as_mut() {
                if let Some(k) = parse_key(&key) {
                    let _ = e.key(
                        k,
                        if down {
                            Direction::Press
                        } else {
                            Direction::Release
                        },
                    );
                }
            }
        }
        ActionKind::MouseDown { button, x, y } => {
            if let Some(e) = enigo.as_mut() {
                let _ = e.move_mouse(*x, *y, enigo::Coordinate::Abs);
                let _ = e.button(map_btn(*button), Direction::Press);
            }
        }
        ActionKind::MouseUp { button, x, y } => {
            if let Some(e) = enigo.as_mut() {
                let _ = e.move_mouse(*x, *y, enigo::Coordinate::Abs);
                let _ = e.button(map_btn(*button), Direction::Release);
            }
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
            rw,
            rh,
            px,
            py,
        } => {
            let image = eval::expand_text(image, vars);
            let hint = match (*px, *py) {
                (Some(px), Some(py)) => Some((px, py)),
                _ => Some((*x, *y)),
            };
            if !live {
                push(format!("Smart Click ({x}, {y}) logic-skip"));
            } else if image.is_empty() {
                push(format!("Smart Click ({x}, {y}) no image — skip"));
            } else if let Some((hit, tw, th)) = wait_match(
                &image,
                hint,
                *timeout_ms,
                *confidence,
                env.script_dir,
                env.stop,
                false,
                match (*rw, *rh) {
                    (Some(w), Some(h)) if w > 0 && h > 0 => Some((*x, *y, w, h)),
                    _ => None,
                },
            ) {
                let (cx, cy) = vision::click_on_match(hit, *ox, *oy, tw, th);
                if let Some(e) = enigo.as_mut() {
                    let _ = e.move_mouse(cx, cy, enigo::Coordinate::Abs);
                    let _ = e.button(Button::Left, Direction::Click);
                }
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
            let image = eval::expand_text(image, vars);
            let save_x = eval::expand_text(save_x, vars);
            let save_y = eval::expand_text(save_y, vars);
            if !live {
                vars.insert("found".into(), "notfound".into());
                push(format!("Search Picture logic-skip `{image}`"));
            } else if let Some((hit, _, _)) = wait_match(
                &image,
                None,
                *timeout_ms,
                *confidence,
                env.script_dir,
                env.stop,
                true,
                None,
            ) {
                vars.insert("found".into(), "true".into());
                vars.insert(save_x, hit.0.to_string());
                vars.insert(save_y, hit.1.to_string());
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
            if !live {
                push(format!("WaitTime {:02}:{:02} logic-skip", hh, mm));
            } else {
                use chrono::Timelike;
                let now = chrono::Local::now();
                match wait_seconds_until(now.hour(), now.minute(), now.second(), *hh, *mm) {
                    None => push("WaitTime missed".into()),
                    Some(0) => push(format!("WaitTime {:02}:{:02} now", hh, mm)),
                    Some(secs) => {
                        let deadline = Instant::now() + Duration::from_secs(secs);
                        while Instant::now() < deadline {
                            if env.stop.load(Ordering::Relaxed) {
                                return false;
                            }
                            // Pausing must freeze the countdown, not skip it.
                            if !wait_pause(env.pause, env.stop) {
                                return false;
                            }
                            thread::sleep(Duration::from_millis(200));
                        }
                        push(format!("WaitTime {:02}:{:02}", hh, mm));
                    }
                }
            }
        }
        ActionKind::RandomNumber { name, a, b } => {
            let n = random_in_range(*a, *b, next_draw());
            vars.insert(name.clone(), n.to_string());
            push(format!("Random {name} = {n}"));
        }
        ActionKind::RandomMouse {
            x1,
            y1,
            x2,
            y2,
            save_x,
            save_y,
        } => {
            let rx =
                random_in_range((*x1).min(*x2) as i64, (*x1).max(*x2) as i64, next_draw()) as i32;
            let ry =
                random_in_range((*y1).min(*y2) as i64, (*y1).max(*y2) as i64, next_draw()) as i32;
            vars.insert(save_x.clone(), rx.to_string());
            vars.insert(save_y.clone(), ry.to_string());
            if live {
                if let Some(e) = enigo.as_mut() {
                    let _ = e.move_mouse(rx, ry, enigo::Coordinate::Abs);
                }
                push(format!("Random mouse ({rx},{ry})"));
            } else {
                push(format!("Random mouse ({rx},{ry}) logic-skip"));
            }
        }
        ActionKind::ReadJson { file, query, name } => {
            // Pure data processing: runs in both modes.
            let file = eval::expand_text(file, vars);
            let query = eval::expand_text(query, vars);
            let resolved = resolve_script_path(&file, env.script_dir);
            match std::fs::read_to_string(&resolved)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            {
                Some(mut v) => {
                    let mut ok = true;
                    for seg in query.split('.').filter(|s| !s.is_empty()) {
                        let next = if let Some(arr) = v.as_array() {
                            seg.parse::<usize>().ok().and_then(|i| arr.get(i)).cloned()
                        } else {
                            v.get(seg).cloned()
                        };
                        match next {
                            Some(x) => v = x,
                            None => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        let text = match &v {
                            serde_json::Value::String(sv) => sv.clone(),
                            other => other.to_string(),
                        };
                        vars.insert(name.clone(), text);
                        push(format!("Read JSON {name}"));
                    } else {
                        push(format!("Read JSON `{query}` not found in {file}"));
                    }
                }
                None => push(format!("Read JSON read failed: {file}")),
            }
        }
        ActionKind::WaitClipboard {
            exclude,
            save_name,
            timeout_ms,
            on_fail,
        } => {
            let exclude = eval::expand_text(exclude, vars);
            if !live {
                push(format!("Wait Clipboard {save_name} logic-skip"));
            } else {
                let baseline = if exclude.trim().is_empty() {
                    crate::clipboard::get_text().unwrap_or_default()
                } else {
                    exclude
                };
                let deadline = Instant::now() + Duration::from_millis(*timeout_ms);
                loop {
                    if env.stop.load(Ordering::Relaxed) {
                        return false;
                    }
                    if !wait_pause(env.pause, env.stop) {
                        return false;
                    }
                    if let Some(text) = crate::clipboard::get_text() {
                        if !text.is_empty() && text != baseline {
                            vars.insert(save_name.clone(), text);
                            push(format!("Clipboard changed -> {save_name}"));
                            break;
                        }
                    }
                    if Instant::now() >= deadline {
                        push(format!("Wait Clipboard timeout ({save_name})"));
                        if on_fail.eq_ignore_ascii_case("stop") {
                            return false;
                        }
                        break;
                    }
                    thread::sleep(Duration::from_millis(200));
                }
            }
        }
        ActionKind::Command { cmd } => {
            let cmd = eval::expand_text(cmd, vars);
            push(format!("Command: {cmd}"));
            if live && !cmd.is_empty() {
                #[cfg(windows)]
                {
                    let _ = Command::new("cmd").args(["/C", &cmd]).status();
                }
                #[cfg(not(windows))]
                {
                    let _ = Command::new("sh").args(["-c", &cmd]).status();
                }
            }
        }
        ActionKind::ActivateWindow { title } => {
            let title = eval::expand_text(title, vars);
            if live {
                let ok = window_command(&title, false);
                push(format!(
                    "Activate `{title}`: {}",
                    if ok { "ok" } else { "not found" }
                ));
            }
        }
        ActionKind::CloseWindow { title } => {
            let title = eval::expand_text(title, vars);
            if live {
                let ok = window_command(&title, true);
                push(format!(
                    "Close `{title}`: {}",
                    if ok { "ok" } else { "not found" }
                ));
            }
        }
        ActionKind::WaitWindow {
            title,
            timeout_ms,
            on_fail,
        } => {
            let title = eval::expand_text(title, vars);
            if !live {
                push(format!("Wait Window \"{title}\" logic-skip"));
            } else {
                let deadline = Instant::now() + Duration::from_millis(*timeout_ms);
                loop {
                    if env.stop.load(Ordering::Relaxed) {
                        return false;
                    }
                    if !wait_pause(env.pause, env.stop) {
                        return false;
                    }
                    if window_exists(&title) {
                        push(format!("Wait Window \"{title}\" found"));
                        break;
                    }
                    if Instant::now() >= deadline {
                        push(format!("Wait Window \"{title}\" timeout"));
                        if on_fail.eq_ignore_ascii_case("stop") {
                            return false;
                        }
                        break;
                    }
                    thread::sleep(Duration::from_millis(200));
                }
            }
        }
        ActionKind::SetClipboard { text } => {
            let text = eval::expand_text(text, vars);
            let shown: String = text.chars().take(40).collect();
            push(format!("Set Clipboard \"{shown}\""));
            if live && !crate::clipboard::set_text(&text) {
                push("Set Clipboard failed".into());
            }
        }
        ActionKind::GetClipboard { name } => {
            if live {
                match crate::clipboard::get_text() {
                    Some(text) => {
                        vars.insert(name.clone(), text);
                        push(format!("Get Clipboard -> {name}"));
                    }
                    None => push(format!("Get Clipboard failed ({name})")),
                }
            } else {
                push(format!("Get Clipboard {name} logic-skip"));
            }
        }
        ActionKind::OpenFile { path } | ActionKind::OpenFolder { path } => {
            let path = eval::expand_text(path, vars);
            if live {
                open_path(&path);
            }
        }
        ActionKind::OpenUrl { url } => {
            let url = eval::expand_text(url, vars);
            if live {
                open_path(&url);
            }
        }
        ActionKind::SetVar { name, value } => {
            let resolved = eval::eval_value(value, vars);
            vars.insert(name.clone(), resolved.clone());
            push(format!("SetVar {name}={resolved}"));
        }
        ActionKind::Label { .. } => {}
        ActionKind::Goto { name } => {
            let name = eval::expand_text(name, vars);
            if let Some(idx) = labels.get(&name) {
                *i = *idx;
            } else {
                push(format!("Goto `{name}`: not found"));
            }
        }
        ActionKind::MessageBox { text } => {
            let text = eval::expand_text(text, vars);
            push(format!("MessageBox: {text}"));
            if live {
                show_message_box(&text);
            }
        }
        ActionKind::CallFunction { name } => {
            push(format!("Call {name}"));
        }
        ActionKind::WriteJson { file, query, value } => {
            // Pure data processing: runs in both modes.
            let file = eval::expand_text(file, vars);
            let query = eval::expand_text(query, vars);
            let value = eval::eval_value(value, vars);
            let resolved = resolve_script_path(&file, env.script_dir);
            let mut root: serde_json::Value = std::fs::read_to_string(&resolved)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Object(Default::default()));
            let typed: serde_json::Value = match value.parse::<i64>() {
                Ok(n) => serde_json::Value::from(n),
                Err(_) => match value.parse::<f64>() {
                    Ok(n) => serde_json::Value::from(n),
                    Err(_) => match value.to_ascii_lowercase().as_str() {
                        "true" => serde_json::Value::Bool(true),
                        "false" => serde_json::Value::Bool(false),
                        _ => serde_json::Value::from(value.clone()),
                    },
                },
            };
            let segs: Vec<&str> = query.split('.').filter(|s| !s.is_empty()).collect();
            if segs.is_empty() {
                push(format!("Write JSON empty path: {file}"));
            } else {
                json_set_at(&mut root, &segs, typed);
                match serde_json::to_string_pretty(&root)
                    .map_err(|e| e.to_string())
                    .and_then(|s| std::fs::write(&resolved, s).map_err(|e| e.to_string()))
                {
                    Ok(()) => push(format!("Write JSON {file} :: {query}")),
                    Err(e) => push(format!("Write JSON failed: {e}")),
                }
            }
        }
        ActionKind::ReadRegistry { path, value, name } => {
            let path = eval::expand_text(path, vars);
            let value = eval::expand_text(value, vars);
            if live {
                match reg_read_string(&path, &value) {
                    Some(data) => {
                        vars.insert(name.clone(), data.clone());
                        push(format!("Read Registry {name} <- {path}\\{value}"));
                    }
                    None => push(format!("Read Registry failed: {path}\\{value}")),
                }
            } else {
                push(format!(
                    "Read Registry {name} <- {path}\\{value} logic-skip"
                ));
            }
        }
        ActionKind::WriteRegistry { path, value, data } => {
            let path = eval::expand_text(path, vars);
            let value = eval::expand_text(value, vars);
            let data = eval::eval_value(data, vars);
            if live {
                let ok = reg_write_string(&path, &value, &data);
                push(format!(
                    "Write Registry {path}\\{value}: {}",
                    if ok { "ok" } else { "failed" }
                ));
            } else {
                push(format!("Write Registry {path}\\{value} logic-skip"));
            }
        }
        ActionKind::MinimizeWindow { title } => {
            let title = eval::expand_text(title, vars);
            if live {
                let ok = window_style_command(&title, 3); // action 3 = minimize
                push(format!(
                    "Minimize `{title}`: {}",
                    if ok { "ok" } else { "not found" }
                ));
            }
        }
        ActionKind::MaximizeWindow { title } => {
            let title = eval::expand_text(title, vars);
            if live {
                let ok = window_style_command(&title, 4); // action 4 = maximize
                push(format!(
                    "Maximize `{title}`: {}",
                    if ok { "ok" } else { "not found" }
                ));
            }
        }
        ActionKind::RestoreWindow { title } => {
            let title = eval::expand_text(title, vars);
            if live {
                let ok = window_style_command(&title, 5); // action 5 = restore
                push(format!(
                    "Restore `{title}`: {}",
                    if ok { "ok" } else { "not found" }
                ));
            }
        }
        ActionKind::SetClipboardHtml { html } => {
            let html = eval::expand_text(html, vars);
            let shown: String = html.chars().take(40).collect();
            push(format!("Set Clipboard HTML \"{shown}\""));
            if live && !crate::clipboard::set_html(&html) {
                push("Set Clipboard HTML failed".into());
            }
        }
        ActionKind::GetClipboardHtml { name } => {
            if live {
                match crate::clipboard::get_html() {
                    Some(html) => {
                        vars.insert(name.clone(), html);
                        push(format!("Get Clipboard HTML -> {name}"));
                    }
                    None => push(format!("Get Clipboard HTML failed ({name})")),
                }
            } else {
                push(format!("Get Clipboard HTML {name} logic-skip"));
            }
        }
        ActionKind::PlayScript { path } => {
            let path = eval::expand_text(path, vars);
            push(format!("Play script {path}"));
            if env.depth >= 8 {
                push("Play script depth limit".into());
                return false;
            }
            let resolved = resolve_script_path(&path, env.script_dir);
            if !run_child_script(resolved, env, vars, &path) {
                return false;
            }
        }
        ActionKind::PlayRandom { folder } => {
            let folder = eval::expand_text(folder, vars);
            push(format!("Play random from {folder}"));
            if env.depth >= 8 {
                push("Play script depth limit".into());
                return false;
            }
            let dir = {
                let p = Path::new(&folder);
                if p.is_dir() {
                    PathBuf::from(p)
                } else {
                    match env.script_dir {
                        Some(d) if d.join(&folder).is_dir() => d.join(&folder),
                        _ => PathBuf::from(&folder),
                    }
                }
            };
            let mut candidates: Vec<PathBuf> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().map(|e| e == "amk").unwrap_or(false))
                .collect();
            candidates.sort();
            if candidates.is_empty() {
                push(format!("Play random: no .amk in {folder}"));
                return false;
            }
            let idx = random_in_range(0, candidates.len() as i64 - 1, next_draw()) as usize;
            let picked = candidates[idx].clone();
            push(format!("Play random -> {}", picked.display()));
            if !run_child_script(picked, env, vars, &folder) {
                return false;
            }
        }
        _ => {}
    }
    true
}

/// Parse (once per run, cached) and run a child script, sharing variables.
fn run_child_script(
    resolved: PathBuf,
    env: &RunEnv<'_>,
    vars: &mut HashMap<String, String>,
    display_name: &str,
) -> bool {
    // Parse each child once per run: a PlayScript inside a While loop must
    // not re-read and re-parse the file on every iteration.
    let child = {
        let mut cache = env.scripts.borrow_mut();
        match cache.entry(resolved.clone()) {
            std::collections::hash_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let parsed = std::fs::read_to_string(&resolved)
                    .ok()
                    .and_then(|s| Script::load_json(&s).ok())
                    .map(Arc::new);
                e.insert(parsed).clone()
            }
        }
    };
    let Some(sc) = child else {
        push_line(env.log, format!("Play script load failed: {display_name}"));
        return false;
    };
    let nested_dir = resolved.parent().map(|p| p.to_path_buf());
    let nested = RunEnv {
        speed: env.speed,
        stop: env.stop,
        pause: env.pause,
        current: env.current,
        log: env.log,
        script_dir: nested_dir.as_deref().or(env.script_dir),
        mode: env.mode,
        depth: env.depth + 1,
        state: env.state,
        step_once: env.step_once,
        step_over: env.step_over,
        step_over_floor: env.step_over_floor,
        scripts: env.scripts,
    };
    let (ok, child_vars) = run_once(&sc, &nested, vars);
    *vars = child_vars;
    ok
}

fn resolve_script_path(path: &str, script_dir: Option<&Path>) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_file() {
        return p;
    }
    if let Some(dir) = script_dir {
        let c = dir.join(path);
        if c.is_file() {
            return c;
        }
    }
    p
}

type MatchAt = ((i32, i32, i32, i32), i32, i32);

#[allow(clippy::too_many_arguments)]
fn wait_match(
    image: &str,
    prefer: Option<(i32, i32)>,
    timeout_ms: u64,
    confidence: f32,
    script_dir: Option<&std::path::Path>,
    stop: &AtomicBool,
    full_search: bool,
    region: Option<(i32, i32, i32, i32)>,
) -> Option<MatchAt> {
    let path = vision::resolve_image(image, script_dir)?;
    let tmpl = vision::load_bmp24(&path)?;
    let template_w = tmpl.w;
    let template_h = tmpl.h;
    let once = vision::match_try_once(timeout_ms);
    let deadline = Instant::now() + Duration::from_millis(if once { 0 } else { timeout_ms });
    let mut pad = vision::smart_search_pad(tmpl.w, tmpl.h);
    loop {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        let t0 = Instant::now();
        if let Some((rx, ry, rw, rh)) = region {
            if let Some(local) = capture::grab_rect(rx, ry, rw, rh) {
                let pref = prefer.map(|(px, py)| (px - rx, py - ry));
                if let Some(hit) = vision::find_template(&local, &tmpl, confidence, pref) {
                    return Some((
                        (hit.0 + rx, hit.1 + ry, hit.2, hit.3),
                        template_w,
                        template_h,
                    ));
                }
            }
        } else {
            if let Some((px, py)) = prefer {
                let x0 = (px - pad).max(0);
                let y0 = (py - pad).max(0);
                if let Some(local) = capture::grab_rect(x0, y0, tmpl.w + pad * 2, tmpl.h + pad * 2)
                {
                    if let Some(hit) =
                        vision::find_template(&local, &tmpl, confidence, Some((px - x0, py - y0)))
                    {
                        return Some((
                            (hit.0 + x0, hit.1 + y0, hit.2, hit.3),
                            template_w,
                            template_h,
                        ));
                    }
                }
            }
            if full_search {
                if let Some((screen, sx, sy)) = capture::grab_screen() {
                    let pref = prefer.map(|(px, py)| (px - sx, py - sy));
                    if let Some(hit) = vision::find_template(&screen, &tmpl, confidence, pref) {
                        return Some((
                            (hit.0 + sx, hit.1 + sy, hit.2, hit.3),
                            template_w,
                            template_h,
                        ));
                    }
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
        other => {
            if let Some(rest) = other.strip_prefix("vk") {
                if let Ok(vk) = rest.parse::<u32>() {
                    return Some(Key::Other(vk));
                }
            }
            if other.chars().count() == 1 {
                let c = other.chars().next()?;
                #[cfg(windows)]
                {
                    return Some(match c.to_ascii_uppercase() {
                        'A' => Key::A,
                        'B' => Key::B,
                        'C' => Key::C,
                        'D' => Key::D,
                        'E' => Key::E,
                        'F' => Key::F,
                        'G' => Key::G,
                        'H' => Key::H,
                        'I' => Key::I,
                        'J' => Key::J,
                        'K' => Key::K,
                        'L' => Key::L,
                        'M' => Key::M,
                        'N' => Key::N,
                        'O' => Key::O,
                        'P' => Key::P,
                        'Q' => Key::Q,
                        'R' => Key::R,
                        'S' => Key::S,
                        'T' => Key::T,
                        'U' => Key::U,
                        'V' => Key::V,
                        'W' => Key::W,
                        'X' => Key::X,
                        'Y' => Key::Y,
                        'Z' => Key::Z,
                        '0' => Key::Num0,
                        '1' => Key::Num1,
                        '2' => Key::Num2,
                        '3' => Key::Num3,
                        '4' => Key::Num4,
                        '5' => Key::Num5,
                        '6' => Key::Num6,
                        '7' => Key::Num7,
                        '8' => Key::Num8,
                        '9' => Key::Num9,
                        _ => Key::Unicode(c),
                    });
                }
                #[cfg(not(windows))]
                {
                    return Some(Key::Unicode(c));
                }
            }
            return None;
        }
    })
}

fn show_message_box(text: &str) {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        let title: Vec<u16> = OsStr::new("Automatic Mouse and Keyboard")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let body: Vec<u16> = OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            MessageBoxW(std::ptr::null_mut(), body.as_ptr(), title.as_ptr(), 0);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("MessageBox: {text}");
    }
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(
        hwnd: *mut core::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        ty: u32,
    ) -> i32;
}

/// Seconds to wait until `hh:mm`. `Some(0)` when that minute is already the
/// current one. `None` when the minute has passed or the clock is invalid.
fn wait_seconds_until(hour: u32, minute: u32, second: u32, hh: u32, mm: u32) -> Option<u64> {
    if hh > 23 || mm > 59 || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let now = hour as u64 * 3600 + minute as u64 * 60 + second as u64;
    let target = hh as u64 * 3600 + mm as u64 * 60;
    if now < target {
        Some(target - now)
    } else if now < target + 60 {
        Some(0)
    } else {
        None
    }
}

/// Inclusive range. `draw` picks a slot; values are spread across the whole span.
pub fn random_in_range(lo: i64, hi: i64, draw: u64) -> i64 {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let span = (hi as i128) - (lo as i128) + 1;
    if span <= 1 {
        return lo;
    }
    let span = span as u128;
    let zone = u128::MAX - (u128::MAX % span);
    let mut x = draw as u128;
    for _ in 0..4 {
        if x < zone {
            return (lo as i128 + (x % span) as i128) as i64;
        }
        x = x
            .wrapping_add(0x9E3779B97F4A7C15)
            .wrapping_mul(0xBF58476D1CE4E5B9);
    }
    (lo as i128 + (draw as u128 % span) as i128) as i64
}

fn next_draw() -> u64 {
    use std::sync::atomic::AtomicU64;
    static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let mut z = STATE.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Inclusive random range for `RAND(lo, hi)` expressions and `RandomNumber`.
pub fn rand_range(lo: i64, hi: i64) -> i64 {
    random_in_range(lo, hi, next_draw())
}

fn title_matches(window: &str, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return false;
    }
    window.to_lowercase().contains(&query.to_lowercase())
}

fn window_command(title: &str, close: bool) -> bool {
    let title = title.trim();
    if title.is_empty() {
        return false;
    }
    #[cfg(windows)]
    {
        window_action(title, if close { 2 } else { 1 })
    }
    #[cfg(not(windows))]
    {
        let _ = close;
        false
    }
}

/// True when a visible window title contains `query`.
fn window_exists(query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return false;
    }
    #[cfg(windows)]
    {
        window_action(query, 0)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
struct WinHit {
    query: String,
    /// 0 = find only, 1 = activate, 2 = close, 3 = minimize, 4 = maximize,
    /// 5 = restore.
    action: u8,
    found: bool,
}

#[cfg(windows)]
fn window_action(query: &str, action: u8) -> bool {
    let mut hit = WinHit {
        query: query.to_string(),
        action,
        found: false,
    };
    unsafe {
        EnumWindows(enum_top_window, &mut hit as *mut WinHit as isize);
    }
    hit.found
}

/// Show-style window command, matching the `window_action` codes
/// (3 = minimize, 4 = maximize, 5 = restore).
fn window_style_command(query: &str, action: u8) -> bool {
    #[cfg(windows)]
    {
        window_action(query, action)
    }
    #[cfg(not(windows))]
    {
        let _ = (query, action);
        false
    }
}

#[cfg(windows)]
unsafe extern "system" fn enum_top_window(hwnd: *mut core::ffi::c_void, lp: isize) -> i32 {
    if lp == 0 {
        return 0;
    }
    let hit = unsafe { &mut *(lp as *mut WinHit) };
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n <= 0 {
            return 1;
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        if !title_matches(&title, &hit.query) {
            return 1;
        }
        hit.found = true;
        match hit.action {
            0 => return 0,
            1 => {
                bring_to_front(hwnd);
            }
            2 => {
                PostMessageW(hwnd, 0x0010, 0, 0);
            }
            3 => {
                ShowWindow(hwnd, 6); // SW_MINIMIZE
            }
            4 => {
                ShowWindow(hwnd, 3); // SW_MAXIMIZE
            }
            _ => {
                ShowWindow(hwnd, 9); // SW_RESTORE
            }
        }
    }
    0
}

#[cfg(windows)]
fn bring_to_front(hwnd: *mut core::ffi::c_void) {
    unsafe {
        let fg = GetForegroundWindow();
        let cur = GetCurrentThreadId();
        let fg_thread = GetWindowThreadProcessId(fg, std::ptr::null_mut());
        let dst_thread = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
        let attach_fg = fg_thread != 0 && fg_thread != cur;
        let attach_dst = dst_thread != 0 && dst_thread != cur && dst_thread != fg_thread;
        if attach_fg {
            AttachThreadInput(cur, fg_thread, 1);
        }
        if attach_dst {
            AttachThreadInput(cur, dst_thread, 1);
        }
        ShowWindow(hwnd, 9);
        SetForegroundWindow(hwnd);
        if attach_dst {
            AttachThreadInput(cur, dst_thread, 0);
        }
        if attach_fg {
            AttachThreadInput(cur, fg_thread, 0);
        }
    }
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn EnumWindows(
        cb: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
        lparam: isize,
    ) -> i32;
    fn GetWindowTextW(hwnd: *mut core::ffi::c_void, buf: *mut u16, max: i32) -> i32;
    fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
    fn SetForegroundWindow(hwnd: *mut core::ffi::c_void) -> i32;
    fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    fn PostMessageW(hwnd: *mut core::ffi::c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn GetForegroundWindow() -> *mut core::ffi::c_void;
    fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
    fn AttachThreadInput(from: u32, to: u32, attach: i32) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
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

/// Set `value` at a dot path (`a.b.0.c`), creating intermediate objects and
/// arrays. A numeric segment addresses an array index and reshapes the node.
fn json_set_at(root: &mut serde_json::Value, segs: &[&str], value: serde_json::Value) {
    if segs.is_empty() {
        *root = value;
        return;
    }
    let seg = segs[0];
    if let Ok(idx) = seg.parse::<usize>() {
        if !root.is_array() {
            *root = serde_json::Value::Array(Vec::new());
        }
        let arr = root.as_array_mut().expect("just made an array");
        while arr.len() <= idx {
            arr.push(serde_json::Value::Null);
        }
        json_set_at(&mut arr[idx], &segs[1..], value);
    } else {
        if !root.is_object() {
            *root = serde_json::Value::Object(Default::default());
        }
        let obj = root.as_object_mut().expect("just made an object");
        let entry = obj
            .entry(seg.to_string())
            .or_insert(serde_json::Value::Null);
        json_set_at(entry, &segs[1..], value);
    }
}

#[cfg(windows)]
mod registry {
    const HKEY_CURRENT_USER: usize = 0x8000_0001;
    const HKEY_LOCAL_MACHINE: usize = 0x8000_0002;
    const KEY_READ: u32 = 0x0002_0019;
    const KEY_WRITE: u32 = 0x0002_0006;
    const REG_SZ: u32 = 1;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            key: usize,
            sub: *const u16,
            options: u32,
            access: u32,
            result: *mut usize,
        ) -> i32;
        fn RegCreateKeyExW(
            key: usize,
            sub: *const u16,
            reserved: u32,
            class: *const u16,
            options: u32,
            access: u32,
            security: *mut core::ffi::c_void,
            result: *mut usize,
            disposition: *mut u32,
        ) -> i32;
        fn RegQueryValueExW(
            key: usize,
            name: *const u16,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            size: *mut u32,
        ) -> i32;
        fn RegSetValueExW(
            key: usize,
            name: *const u16,
            reserved: u32,
            kind: u32,
            data: *const u8,
            size: u32,
        ) -> i32;
        // Test cleanup only; the allow keeps the plain build warning-free.
        #[cfg(test)]
        #[allow(dead_code)]
        fn RegDeleteTreeW(key: usize, sub: *const u16) -> i32;
        fn RegCloseKey(key: usize) -> i32;
    }

    /// `HKCU\Software\...` (or HKLM) -> (root key handle, subkey text).
    pub fn parse_path(path: &str) -> Option<(usize, String)> {
        let is_sep = |c: char| c == '\\' || c == '/';
        let path = path.trim().trim_matches(is_sep);
        let (root, sub) = path.split_once(is_sep)?;
        let root = match root.to_ascii_uppercase().as_str() {
            "HKCU" | "HKEY_CURRENT_USER" => HKEY_CURRENT_USER,
            "HKLM" | "HKEY_LOCAL_MACHINE" => HKEY_LOCAL_MACHINE,
            _ => return None,
        };
        let sub = sub.trim_matches(is_sep);
        if sub.is_empty() {
            return None;
        }
        Some((root, sub.to_string()))
    }

    pub fn read(path: &str, value: &str) -> Option<String> {
        let (root, sub) = parse_path(path)?;
        let wide = to_wide(&sub);
        let name = to_wide(value);
        let mut key = 0usize;
        unsafe {
            if RegOpenKeyExW(root, wide.as_ptr(), 0, KEY_READ, &mut key) != 0 {
                return None;
            }
            let mut kind = 0u32;
            let mut size = 0u32;
            let status = RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            );
            if status != 0 || kind != REG_SZ || size == 0 || size > 1 << 16 {
                RegCloseKey(key);
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            let status = RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                buf.as_mut_ptr(),
                &mut size,
            );
            RegCloseKey(key);
            if status != 0 {
                return None;
            }
            // REG_SZ data is UTF-16LE.
            let mut wide = Vec::with_capacity(buf.len() / 2);
            for pair in buf.chunks_exact(2) {
                wide.push(u16::from_le_bytes([pair[0], pair[1]]));
            }
            while wide.last() == Some(&0) {
                wide.pop();
            }
            Some(String::from_utf16_lossy(&wide))
        }
    }

    pub fn write(path: &str, value: &str, data: &str) -> bool {
        let Some((root, sub)) = parse_path(path) else {
            return false;
        };
        let wide = to_wide(&sub);
        let name = to_wide(value);
        let mut payload: Vec<u8> = data.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        payload.extend_from_slice(&[0, 0]);
        let mut key = 0usize;
        let mut disposition = 0u32;
        unsafe {
            let ok = RegCreateKeyExW(
                root,
                wide.as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_WRITE,
                std::ptr::null_mut(),
                &mut key,
                &mut disposition,
            ) == 0
                && RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    payload.as_ptr(),
                    payload.len() as u32,
                ) == 0;
            RegCloseKey(key);
            ok
        }
    }

    /// Test cleanup only.
    #[cfg(test)]
    pub fn delete_tree(path: &str) -> bool {
        let Some((root, sub)) = parse_path(path) else {
            return false;
        };
        let wide = to_wide(&sub);
        unsafe { RegDeleteTreeW(root, wide.as_ptr()) == 0 }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

/// Read a registry string value (`REG_SZ`) like `HKCU\Software\Name`.
#[cfg(windows)]
fn reg_read_string(path: &str, value: &str) -> Option<String> {
    registry::read(path, value)
}

#[cfg(not(windows))]
fn reg_read_string(_path: &str, _value: &str) -> Option<String> {
    None
}

/// Write a registry string value (`REG_SZ`), creating the key when missing.
#[cfg(windows)]
fn reg_write_string(path: &str, value: &str, data: &str) -> bool {
    registry::write(path, value, data)
}

#[cfg(not(windows))]
fn reg_write_string(_path: &str, _value: &str, _data: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Action, ActionKind, Script};

    fn kinds(ks: Vec<ActionKind>) -> Script {
        Script {
            version: "1.0".into(),
            name: "t".into(),
            actions: ks.into_iter().map(Action::new).collect(),
        }
    }

    fn texts(r: &RunReport) -> Vec<String> {
        r.logs.iter().map(|l| l.text.clone()).collect()
    }

    #[test]
    fn for_step_two_visits_odd_values_only() {
        let sc = kinds(vec![
            ActionKind::For {
                var: "i".into(),
                from: 1,
                to: 5,
                step: 2,
            },
            ActionKind::Delay { ms: 0 },
            ActionKind::EndFor,
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        let t = texts(&r);
        assert!(t.iter().any(|s| s == "For i=1"), "{t:?}");
        assert!(t.iter().any(|s| s == "For i=3"), "{t:?}");
        assert!(t.iter().any(|s| s == "For i=5"), "{t:?}");
        assert!(!t.iter().any(|s| s == "For i=2"), "{t:?}");
        assert!(!t.iter().any(|s| s == "For i=4"), "{t:?}");
        assert_eq!(r.vars.get("i").map(String::as_str), Some("5"));
    }

    #[test]
    fn for_negative_step_counts_down() {
        let sc = kinds(vec![
            ActionKind::For {
                var: "i".into(),
                from: 5,
                to: 1,
                step: -1,
            },
            ActionKind::EndFor,
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        let t = texts(&r);
        assert_eq!(
            t.iter()
                .filter(|s| s.starts_with("For i="))
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                "For i=5".to_string(),
                "For i=4".to_string(),
                "For i=3".to_string(),
                "For i=2".to_string(),
                "For i=1".to_string(),
            ]
        );
        assert_eq!(r.vars.get("i").map(String::as_str), Some("1"));
    }

    #[test]
    fn for_with_unrepresentable_continuation_runs_once_and_terminates() {
        // from == to == i64::MAX with step 1: the body must run exactly once
        // (from + 1 overflows) instead of panicking or looping forever.
        let sc = kinds(vec![
            ActionKind::For {
                var: "i".into(),
                from: i64::MAX,
                to: i64::MAX,
                step: 1,
            },
            ActionKind::SetVar {
                name: "x".into(),
                value: "1".into(),
            },
            ActionKind::EndFor,
            ActionKind::SetVar {
                name: "after".into(),
                value: "yes".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        let max_s = i64::MAX.to_string();
        assert_eq!(r.vars.get("i").map(String::as_str), Some(max_s.as_str()));
        assert_eq!(r.vars.get("x").map(String::as_str), Some("1"));
        assert_eq!(r.vars.get("after").map(String::as_str), Some("yes"));
        assert_eq!(
            texts(&r).iter().filter(|s| s.starts_with("For i=")).count(),
            1
        );
    }

    #[test]
    fn break_exits_the_enclosing_loop_early() {
        let sc = kinds(vec![
            ActionKind::For {
                var: "i".into(),
                from: 1,
                to: 10,
                step: 1,
            },
            ActionKind::If {
                expr: "i == 3".into(),
            },
            ActionKind::Break,
            ActionKind::EndIf,
            ActionKind::EndFor,
            ActionKind::SetVar {
                name: "after".into(),
                value: "yes".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("i").map(String::as_str), Some("3"));
        assert_eq!(r.vars.get("after").map(String::as_str), Some("yes"));
        let t = texts(&r);
        assert!(t.iter().any(|s| s == "For i=3"), "{t:?}");
        assert!(!t.iter().any(|s| s == "For i=4"), "{t:?}");
    }

    #[test]
    fn continue_skips_only_the_current_iteration() {
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "n".into(),
                value: "0".into(),
            },
            ActionKind::For {
                var: "i".into(),
                from: 1,
                to: 4,
                step: 1,
            },
            ActionKind::If {
                expr: "i == 2".into(),
            },
            ActionKind::Continue,
            ActionKind::EndIf,
            ActionKind::SetVar {
                name: "n".into(),
                value: "n + 1".into(),
            },
            ActionKind::EndFor,
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("n").map(String::as_str), Some("3"));
        assert_eq!(r.vars.get("i").map(String::as_str), Some("4"));
    }

    #[test]
    fn break_in_inner_loop_leaves_the_outer_loop_running() {
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "rounds".into(),
                value: "0".into(),
            },
            ActionKind::While {
                expr: "true".into(),
            },
            ActionKind::For {
                var: "i".into(),
                from: 1,
                to: 5,
                step: 1,
            },
            ActionKind::If {
                expr: "i == 2".into(),
            },
            ActionKind::Break,
            ActionKind::EndIf,
            ActionKind::EndFor,
            ActionKind::SetVar {
                name: "rounds".into(),
                value: "rounds + 1".into(),
            },
            ActionKind::If {
                expr: "rounds == 2".into(),
            },
            ActionKind::Break,
            ActionKind::EndIf,
            ActionKind::EndWhile,
            ActionKind::SetVar {
                name: "done".into(),
                value: "1".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("rounds").map(String::as_str), Some("2"));
        assert_eq!(r.vars.get("done").map(String::as_str), Some("1"));
    }

    #[test]
    fn break_without_a_loop_is_logged_and_does_not_stop() {
        let sc = kinds(vec![
            ActionKind::Break,
            ActionKind::SetVar {
                name: "x".into(),
                value: "1".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("x").map(String::as_str), Some("1"));
        assert!(texts(&r).iter().any(|s| s.contains("no loop")));
    }

    #[test]
    fn new_amk_actions_log_in_logic_mode() {
        let sc = kinds(vec![
            ActionKind::KeyDown {
                key: "Shift".into(),
            },
            ActionKind::KeyUp {
                key: "Shift".into(),
            },
            ActionKind::SetClipboard {
                text: "hello".into(),
            },
            ActionKind::GetClipboard {
                name: "clip".into(),
            },
            ActionKind::WaitWindow {
                title: "Notepad".into(),
                timeout_ms: 0,
                on_fail: "stop".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        let t = texts(&r);
        assert!(t.iter().any(|s| s.contains("Key Down Shift")), "{t:?}");
        assert!(t.iter().any(|s| s.contains("Key Up Shift")), "{t:?}");
        assert!(t.iter().any(|s| s.contains("Set Clipboard")), "{t:?}");
        assert!(t.iter().any(|s| s.contains("Get Clipboard")), "{t:?}");
        assert!(t.iter().any(|s| s.contains("Wait Window")), "{t:?}");
    }

    #[test]
    fn mouse_move_json_without_ms_still_loads() {
        // Backward compatibility: scripts written before the ms field.
        let s = r#"{"kind":{"MouseMove":{"x":7,"y":9}}}"#;
        let v: serde_json::Value = serde_json::from_str(s).unwrap();
        let k: ActionKind = serde_json::from_value(v.get("kind").unwrap().clone()).unwrap();
        assert!(matches!(k, ActionKind::MouseMove { x: 7, y: 9, ms: 0 }));
    }

    #[test]
    fn switch_runs_matching_case_and_default_when_no_match() {
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "n".into(),
                value: "2".into(),
            },
            ActionKind::Switch { expr: "n".into() },
            ActionKind::Case { value: "1".into() },
            ActionKind::SetVar {
                name: "r".into(),
                value: "one".into(),
            },
            ActionKind::Case { value: "2".into() },
            ActionKind::SetVar {
                name: "r".into(),
                value: "two".into(),
            },
            ActionKind::DefaultCase,
            ActionKind::SetVar {
                name: "r".into(),
                value: "other".into(),
            },
            ActionKind::EndSwitch,
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("r").map(String::as_str), Some("two"));
    }

    #[test]
    fn switch_default_runs_when_no_case_matches() {
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "n".into(),
                value: "9".into(),
            },
            ActionKind::Switch { expr: "n".into() },
            ActionKind::Case { value: "1".into() },
            ActionKind::SetVar {
                name: "r".into(),
                value: "one".into(),
            },
            ActionKind::DefaultCase,
            ActionKind::SetVar {
                name: "r".into(),
                value: "other".into(),
            },
            ActionKind::EndSwitch,
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("r").map(String::as_str), Some("other"));
    }

    #[test]
    fn random_mouse_picks_inside_the_rect_and_saves_vars() {
        let sc = kinds(vec![ActionKind::RandomMouse {
            x1: 10,
            y1: 20,
            x2: 30,
            y2: 40,
            save_x: "rx".into(),
            save_y: "ry".into(),
        }]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        let rx: i32 = r.vars.get("rx").unwrap().parse().unwrap();
        let ry: i32 = r.vars.get("ry").unwrap().parse().unwrap();
        assert!((10..=30).contains(&rx), "{rx}");
        assert!((20..=40).contains(&ry), "{ry}");
    }

    #[test]
    fn write_json_creates_and_updates_values_at_dot_paths() {
        let path = std::env::temp_dir().join(format!("amk_write_json_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let sc = kinds(vec![
            ActionKind::WriteJson {
                file: path.to_string_lossy().into_owned(),
                query: "user.name".into(),
                value: "Ada".into(),
            },
            ActionKind::WriteJson {
                file: path.to_string_lossy().into_owned(),
                query: "user.scores.1".into(),
                value: "42".into(),
            },
            ActionKind::WriteJson {
                file: path.to_string_lossy().into_owned(),
                query: "user.scores.0".into(),
                value: "7".into(),
            },
            ActionKind::WriteJson {
                file: path.to_string_lossy().into_owned(),
                query: "user.active".into(),
                value: "true".into(),
            },
            // A numeric segment reshapes an object into an array.
            ActionKind::WriteJson {
                file: path.to_string_lossy().into_owned(),
                query: "user.name.0".into(),
                value: "first".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        let raw = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["user"]["scores"], serde_json::json!([7, 42]));
        assert_eq!(v["user"]["active"], serde_json::json!(true));
        assert_eq!(v["user"]["name"], serde_json::json!(["first"]));
    }

    #[test]
    fn read_json_picks_up_what_write_json_saved() {
        let path = std::env::temp_dir().join(format!("amk_rw_json_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let sc = kinds(vec![
            ActionKind::WriteJson {
                file: path.to_string_lossy().into_owned(),
                query: "token".into(),
                value: "{secret}9".into(),
            },
            ActionKind::ReadJson {
                file: path.to_string_lossy().into_owned(),
                query: "token".into(),
                name: "back".into(),
            },
        ]);
        let r = run_logic(&sc);
        let _ = std::fs::remove_file(&path);
        assert!(r.ok, "{:?}", r.logs);
        // "{secret}" is not a known variable, so it stays literal in the file.
        assert_eq!(r.vars.get("back").map(String::as_str), Some("{secret}9"));
    }

    #[test]
    #[cfg(windows)]
    fn registry_write_then_read_roundtrips() {
        use crate::engine::registry;
        let base = format!(r"HKCU\Software\AMKTest\{}", std::process::id());
        assert!(registry::write(&base, "Greeting", "hello world"));
        let got = registry::read(&base, "Greeting");
        assert!(registry::delete_tree(&base), "cleanup failed");
        assert_eq!(got.as_deref(), Some("hello world"));
        assert_eq!(registry::read(&base, "Greeting"), None);
    }

    #[test]
    #[cfg(windows)]
    fn registry_parse_path_accepts_short_and_long_roots() {
        use crate::engine::registry;
        let (root, sub) = registry::parse_path(r"HKCU\Software\AMK").unwrap();
        assert_eq!(sub, r"Software\AMK");
        let _ = root;
        assert!(registry::parse_path(r"HKCU\").is_none());
        assert!(registry::parse_path(r"HKCR\Software\X").is_none());
    }

    #[test]
    fn read_json_stores_the_value_at_the_dot_path() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("amk_json_{}.json", std::process::id()));
        std::fs::write(&path, r#"{"a":{"b":[10,20,{"c":"hi"}]}}"#).unwrap();
        let sc = kinds(vec![
            ActionKind::ReadJson {
                file: path.to_string_lossy().into_owned(),
                query: "a.b.1".into(),
                name: "v".into(),
            },
            ActionKind::ReadJson {
                file: path.to_string_lossy().into_owned(),
                query: "a.b.2.c".into(),
                name: "w".into(),
            },
        ]);
        let r = run_logic(&sc);
        let _ = std::fs::remove_file(&path);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("v").map(String::as_str), Some("20"));
        assert_eq!(r.vars.get("w").map(String::as_str), Some("hi"));
    }

    #[test]
    fn play_random_runs_a_script_from_the_folder() {
        let dir = std::env::temp_dir().join(format!("amk_rand_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("only.amk"),
            r#"{"version":"1.0","name":"c","actions":[{"id":"c1","name":"set","enabled":true,"delay_ms":0,"kind":{"SetVar":{"name":"nested","value":"yes"}}}]}"#,
        )
        .unwrap();
        let sc = kinds(vec![ActionKind::PlayRandom {
            folder: dir.to_string_lossy().into_owned(),
        }]);
        let r = run_logic(&sc);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("nested").map(String::as_str), Some("yes"));
    }

    #[test]
    fn play_random_without_scripts_fails_the_run() {
        let dir = std::env::temp_dir().join(format!("amk_empty_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sc = kinds(vec![ActionKind::PlayRandom {
            folder: dir.to_string_lossy().into_owned(),
        }]);
        let r = run_logic(&sc);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!r.ok);
    }

    #[test]
    fn mouse_drag_json_without_ms_still_loads() {
        // Backward compatibility: scripts written before the drag ms field.
        let s = r#"{"kind":{"MouseDrag":{"button":"Left","x1":1,"y1":2,"x2":3,"y2":4}}}"#;
        let v: serde_json::Value = serde_json::from_str(s).unwrap();
        let k: ActionKind = serde_json::from_value(v.get("kind").unwrap().clone()).unwrap();
        assert!(matches!(
            k,
            ActionKind::MouseDrag {
                x1: 1,
                y1: 2,
                x2: 3,
                y2: 4,
                ms: 0,
                ..
            }
        ));
    }

    #[test]
    fn if_false_skips_body_else_runs() {
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "x".into(),
                value: "0".into(),
            },
            ActionKind::If {
                expr: "false".into(),
            },
            ActionKind::SetVar {
                name: "x".into(),
                value: "1".into(),
            },
            ActionKind::Else,
            ActionKind::SetVar {
                name: "x".into(),
                value: "2".into(),
            },
            ActionKind::EndIf,
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("x").map(String::as_str), Some("2"));
        let t = texts(&r);
        assert!(t.iter().any(|s| s == "SetVar x=2"), "{t:?}");
        assert!(!t.iter().any(|s| s == "SetVar x=1"), "{t:?}");
    }

    #[test]
    fn if_true_runs_body_skips_else() {
        let sc = kinds(vec![
            ActionKind::If {
                expr: "true".into(),
            },
            ActionKind::SetVar {
                name: "x".into(),
                value: "yes".into(),
            },
            ActionKind::Else,
            ActionKind::SetVar {
                name: "x".into(),
                value: "no".into(),
            },
            ActionKind::EndIf,
        ]);
        let r = run_logic(&sc);
        assert!(r.ok);
        assert_eq!(r.vars.get("x").map(String::as_str), Some("yes"));
        assert!(!texts(&r).iter().any(|s| s == "SetVar x=no"));
    }

    #[test]
    fn while_stops_when_expression_is_false() {
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "n".into(),
                value: "1".into(),
            },
            ActionKind::While {
                expr: "n == 1".into(),
            },
            ActionKind::SetVar {
                name: "n".into(),
                value: "0".into(),
            },
            ActionKind::SetVar {
                name: "hit".into(),
                value: "1".into(),
            },
            ActionKind::EndWhile,
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("n").map(String::as_str), Some("0"));
        assert_eq!(r.vars.get("hit").map(String::as_str), Some("1"));
        let hits = texts(&r).iter().filter(|s| *s == "SetVar hit=1").count();
        assert_eq!(hits, 1, "while must run the body once, not hang");
    }

    #[test]
    fn goto_jumps_to_named_label() {
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "a".into(),
                value: "1".into(),
            },
            ActionKind::Goto {
                name: "skip".into(),
            },
            ActionKind::SetVar {
                name: "a".into(),
                value: "2".into(),
            },
            ActionKind::Label {
                name: "skip".into(),
            },
            ActionKind::SetVar {
                name: "b".into(),
                value: "1".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("a").map(String::as_str), Some("1"));
        assert_eq!(r.vars.get("b").map(String::as_str), Some("1"));
        assert!(!texts(&r).iter().any(|s| s == "SetVar a=2"));
    }

    #[test]
    fn call_function_jumps_to_named_function_entry_and_returns() {
        let mut actions = vec![
            Action::new(ActionKind::FunctionEntry),
            Action::new(ActionKind::CallFunction { name: "foo".into() }),
            Action::new(ActionKind::SetVar {
                name: "after".into(),
                value: "1".into(),
            }),
            Action::new(ActionKind::EndFunction),
        ];
        let mut foo = Action::new(ActionKind::FunctionEntry);
        foo.name = "foo".into();
        actions.push(foo);
        actions.push(Action::new(ActionKind::SetVar {
            name: "called".into(),
            value: "1".into(),
        }));
        actions.push(Action::new(ActionKind::EndFunction));
        let sc = Script {
            version: "1.0".into(),
            name: "t".into(),
            actions,
        };
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("called").map(String::as_str), Some("1"));
        assert_eq!(r.vars.get("after").map(String::as_str), Some("1"));
        assert!(texts(&r).iter().any(|s| s == "Call foo"), "{:?}", r.logs);
    }

    #[test]
    fn play_script_loads_and_runs_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("amk_play_{}.amk", std::process::id()));
        let child = r#"{
          "version":"1.0","name":"child",
          "actions":[
            {"id":"c1","name":"set","enabled":true,"delay_ms":0,
             "kind":{"SetVar":{"name":"nested","value":"yes"}}}
          ]
        }"#;
        std::fs::write(&path, child).unwrap();
        let sc = kinds(vec![ActionKind::PlayScript {
            path: path.to_string_lossy().into_owned(),
        }]);
        let r = run_logic(&sc);
        let _ = std::fs::remove_file(&path);
        assert!(r.ok, "{:?}", r.logs);
        let t = texts(&r);
        assert!(t.iter().any(|s| s.starts_with("Play script ")), "{t:?}");
        assert!(t.iter().any(|s| s == "SetVar nested=yes"), "{t:?}");
    }

    #[test]
    fn message_box_logs_text_in_logic_mode() {
        let sc = kinds(vec![ActionKind::MessageBox {
            text: "hello-user".into(),
        }]);
        let r = run_logic(&sc);
        assert!(r.ok);
        assert!(texts(&r).iter().any(|s| s == "MessageBox: hello-user"));
    }

    #[test]
    fn type_text_is_logged_without_moving_the_mouse() {
        let sc = kinds(vec![ActionKind::TypeText {
            text: "Xin chao tu AMK".into(),
            interval_ms: 0,
        }]);
        let r = run_logic(&sc);
        assert!(r.ok);
        assert!(texts(&r).iter().any(|s| s.contains("Xin chao tu AMK")));
    }

    #[test]
    fn parse_key_maps_letters_as_real_keys() {
        assert!(parse_key("A").is_some());
        assert!(parse_key("c").is_some());
        assert!(parse_key("7").is_some());
        assert!(parse_key("Enter").is_some());
        assert!(parse_key("vk65").is_some());
    }

    #[test]
    fn f7_step_into_pauses_after_the_action() {
        let d = step_decision(true, false, false, 0, usize::MAX);
        assert!(d.pause);
        assert!(d.clear_step_over);
    }

    #[test]
    fn f8_step_over_call_waits_until_return() {
        let enter = step_decision(false, true, true, 1, usize::MAX);
        assert!(!enter.pause);
        assert_eq!(enter.new_floor, Some(0));
        let inside = step_decision(false, true, false, 1, 0);
        assert!(!inside.pause);
        let ret = step_decision(false, true, false, 0, 0);
        assert!(ret.pause);
        assert!(ret.clear_step_over);
    }

    #[test]
    fn f8_on_plain_action_pauses_immediately() {
        let d = step_decision(false, true, false, 0, usize::MAX);
        assert!(d.pause);
    }

    #[test]
    fn nested_if_false_runs_else_not_inner_body() {
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "x".into(),
                value: "0".into(),
            },
            ActionKind::If {
                expr: "false".into(),
            },
            ActionKind::If {
                expr: "true".into(),
            },
            ActionKind::SetVar {
                name: "x".into(),
                value: "1".into(),
            },
            ActionKind::EndIf,
            ActionKind::Else,
            ActionKind::SetVar {
                name: "x".into(),
                value: "2".into(),
            },
            ActionKind::EndIf,
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("x").map(String::as_str), Some("2"));
        assert!(!texts(&r).iter().any(|s| s == "SetVar x=1"));
    }

    #[test]
    fn disabled_action_is_skipped() {
        let mut sc = kinds(vec![
            ActionKind::SetVar {
                name: "x".into(),
                value: "0".into(),
            },
            ActionKind::SetVar {
                name: "x".into(),
                value: "1".into(),
            },
            ActionKind::SetVar {
                name: "x".into(),
                value: "2".into(),
            },
        ]);
        sc.actions[1].enabled = false;
        let r = run_logic(&sc);
        assert!(r.ok);
        assert_eq!(r.vars.get("x").map(String::as_str), Some("2"));
        assert!(!texts(&r).iter().any(|s| s == "SetVar x=1"));
    }

    #[test]
    fn for_inside_false_if_does_not_run() {
        let sc = kinds(vec![
            ActionKind::If {
                expr: "false".into(),
            },
            ActionKind::For {
                var: "i".into(),
                from: 1,
                to: 3,
                step: 1,
            },
            ActionKind::SetVar {
                name: "x".into(),
                value: "1".into(),
            },
            ActionKind::EndFor,
            ActionKind::EndIf,
            ActionKind::SetVar {
                name: "x".into(),
                value: "ok".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("x").map(String::as_str), Some("ok"));
        assert!(!texts(&r).iter().any(|s| s.starts_with("For i=")));
    }

    #[test]
    fn set_var_keeps_text_and_while_counts_with_addition() {
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "file".into(),
                value: "hello-world".into(),
            },
            ActionKind::SetVar {
                name: "n".into(),
                value: "0".into(),
            },
            ActionKind::While {
                expr: "n < 3".into(),
            },
            ActionKind::SetVar {
                name: "n".into(),
                value: "n + 1".into(),
            },
            ActionKind::EndWhile,
            ActionKind::MessageBox {
                text: "n={n} file={file}".into(),
            },
            ActionKind::TypeText {
                text: "Hi {file}".into(),
                interval_ms: 0,
            },
            ActionKind::Command {
                cmd: "echo {n}".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("n").map(String::as_str), Some("3"));
        assert_eq!(r.vars.get("file").map(String::as_str), Some("hello-world"));
        let t = texts(&r);
        assert!(
            t.iter().any(|s| s == "MessageBox: n=3 file=hello-world"),
            "{t:?}"
        );
        assert!(t.iter().any(|s| s == "TypeText Hi hello-world"), "{t:?}");
        assert!(t.iter().any(|s| s == "Command: echo 3"), "{t:?}");
    }

    #[test]
    fn play_script_shares_variables_with_the_caller() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("amk_share_{}.amk", std::process::id()));
        let child = r#"{
          "version":"1.0","name":"child",
          "actions":[
            {"id":"c1","name":"inc","enabled":true,"delay_ms":0,
             "kind":{"SetVar":{"name":"n","value":"n + 1"}}},
            {"id":"c2","name":"mark","enabled":true,"delay_ms":0,
             "kind":{"SetVar":{"name":"extra","value":"child"}}}
          ]
        }"#;
        std::fs::write(&path, child).unwrap();
        let sc = kinds(vec![
            ActionKind::SetVar {
                name: "keep".into(),
                value: "yes".into(),
            },
            ActionKind::SetVar {
                name: "n".into(),
                value: "1".into(),
            },
            ActionKind::PlayScript {
                path: path.to_string_lossy().into_owned(),
            },
        ]);
        let r = run_logic(&sc);
        let _ = std::fs::remove_file(&path);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("keep").map(String::as_str), Some("yes"));
        assert_eq!(r.vars.get("n").map(String::as_str), Some("2"));
        assert_eq!(r.vars.get("extra").map(String::as_str), Some("child"));
    }

    #[test]
    fn goto_missing_label_is_logged_and_does_not_stop() {
        let sc = kinds(vec![
            ActionKind::Goto {
                name: "missing".into(),
            },
            ActionKind::SetVar {
                name: "x".into(),
                value: "1".into(),
            },
        ]);
        let r = run_logic(&sc);
        assert!(r.ok, "{:?}", r.logs);
        assert_eq!(r.vars.get("x").map(String::as_str), Some("1"));
        assert!(texts(&r).iter().any(|s| s.contains("not found")));
    }

    #[test]
    fn wait_time_treats_the_current_minute_as_already_due() {
        assert_eq!(wait_seconds_until(9, 0, 30, 9, 0), Some(0));
        assert_eq!(wait_seconds_until(8, 59, 58, 9, 0), Some(2));
        assert_eq!(wait_seconds_until(9, 1, 0, 9, 0), None);
        assert_eq!(wait_seconds_until(10, 0, 0, 25, 0), None);
        assert_eq!(wait_seconds_until(10, 0, 0, 9, 60), None);
    }

    #[test]
    fn random_stays_inside_inclusive_bounds() {
        assert_eq!(random_in_range(4, 4, 99), 4);
        assert_eq!(random_in_range(5, 1, 0), 1);
        let mut seen = std::collections::HashSet::new();
        for draw in 0..40 {
            let n = random_in_range(3, 1, draw);
            assert!((1..=3).contains(&n), "{n}");
            seen.insert(n);
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn window_title_match_is_a_case_insensitive_part() {
        assert!(title_matches("Untitled - Notepad", "notepad"));
        assert!(title_matches("Tài liệu - Word", "TÀI"));
        assert!(!title_matches("Notepad", "chrome"));
        assert!(!title_matches("Notepad", "  "));
    }
}
