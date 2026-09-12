use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

fn default_confidence() -> f32 {
    0.90
}
fn default_on_fail() -> String {
    "skip".into()
}
fn default_search_fail() -> String {
    "stop".into()
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MouseBtn {
    Left,
    Right,
    Middle,
}

impl MouseBtn {
    pub fn as_str(self) -> &'static str {
        match self {
            MouseBtn::Left => "Left",
            MouseBtn::Right => "Right",
            MouseBtn::Middle => "Middle",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ActionKind {
    FunctionEntry,
    EndFunction,
    Comment {
        text: String,
    },
    Delay {
        ms: u64,
    },
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseClick {
        button: MouseBtn,
        x: i32,
        y: i32,
        clicks: u8,
    },
    MouseDrag {
        button: MouseBtn,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    },
    MouseWheel {
        delta: i32,
    },
    TypeText {
        text: String,
        interval_ms: u64,
    },
    KeyPress {
        key: String,
    },
    MouseDown {
        button: MouseBtn,
        x: i32,
        y: i32,
    },
    MouseUp {
        button: MouseBtn,
        x: i32,
        y: i32,
    },
    SmartClick {
        x: i32,
        y: i32,
        image: String,
        timeout_ms: u64,
        #[serde(default = "default_confidence")]
        confidence: f32,
        #[serde(default = "default_on_fail")]
        on_fail: String,
        #[serde(default)]
        ox: Option<i32>,
        #[serde(default)]
        oy: Option<i32>,
    },
    SearchPicture {
        image: String,
        timeout_ms: u64,
        save_x: String,
        save_y: String,
        #[serde(default = "default_confidence")]
        confidence: f32,
        #[serde(default = "default_search_fail")]
        on_fail: String,
    },
    WaitTime {
        hh: u32,
        mm: u32,
    },
    RandomNumber {
        name: String,
        a: i64,
        b: i64,
    },
    Command {
        cmd: String,
    },
    ActivateWindow {
        title: String,
    },
    CloseWindow {
        title: String,
    },
    OpenFile {
        path: String,
    },
    OpenUrl {
        url: String,
    },
    OpenFolder {
        path: String,
    },
    SetVar {
        name: String,
        value: String,
    },
    If {
        expr: String,
    },
    Else,
    EndIf,
    For {
        var: String,
        from: i64,
        to: i64,
        step: i64,
    },
    EndFor,
    While {
        expr: String,
    },
    EndWhile,
    Label {
        name: String,
    },
    Goto {
        name: String,
    },
    MessageBox {
        text: String,
    },
    CallFunction {
        name: String,
    },
    PlayScript {
        path: String,
    },
}

impl ActionKind {
    pub fn format_columns(&self) -> (String, String, String) {
        match self {
            ActionKind::FunctionEntry => ("Function".into(), "".into(), "Entry".into()),
            ActionKind::EndFunction => ("End".into(), "Function".into(), "".into()),
            ActionKind::Comment { text } => ("Comment".into(), "".into(), text.clone()),
            ActionKind::Delay { ms } => ("Delay".into(), format!("{} ms", ms), "".into()),
            ActionKind::MouseMove { x, y } => (
                "Mouse Move".into(),
                format!("X: {}, Y: {}", x, y),
                "".into(),
            ),
            ActionKind::MouseClick {
                button,
                x,
                y,
                clicks,
            } => (
                "Mouse Click".into(),
                format!("X: {}, Y: {}", x, y),
                format!("{} Button, {} click(s)", button.as_str(), clicks),
            ),
            ActionKind::MouseDrag {
                button,
                x1,
                y1,
                x2,
                y2,
            } => (
                "Mouse Drag".into(),
                format!("{},{} -> {},{}", x1, y1, x2, y2),
                format!("{} Button", button.as_str()),
            ),
            ActionKind::MouseWheel { delta } => {
                ("Mouse Wheel".into(), format!("Delta: {}", delta), "".into())
            }
            ActionKind::TypeText { text, interval_ms } => (
                "Type Text".into(),
                text.clone(),
                format!("Interval: {} ms", interval_ms),
            ),
            ActionKind::KeyPress { key } => ("Key Press".into(), key.clone(), "".into()),
            ActionKind::MouseDown { button, x, y } => (
                "Mouse Down".into(),
                format!("X: {}, Y: {}", x, y),
                format!("{} Button", button.as_str()),
            ),
            ActionKind::MouseUp { button, x, y } => (
                "Mouse Up".into(),
                format!("X: {}, Y: {}", x, y),
                format!("{} Button", button.as_str()),
            ),
            ActionKind::SmartClick {
                x,
                y,
                image,
                timeout_ms,
                ..
            } => (
                "Smart Click".into(),
                format!("X: {}, Y: {}", x, y),
                format!("Image: {}, Timeout: {} ms", image, timeout_ms),
            ),
            ActionKind::SearchPicture {
                image,
                timeout_ms,
                save_x,
                save_y,
                ..
            } => (
                "Search Picture".into(),
                image.clone(),
                format!("Save: {}, {}, Timeout: {} ms", save_x, save_y, timeout_ms),
            ),
            ActionKind::ActivateWindow { title } => {
                ("Activate Window".into(), title.clone(), "".into())
            }
            ActionKind::CloseWindow { title } => ("Close Window".into(), title.clone(), "".into()),
            ActionKind::OpenFile { path } => ("Open File".into(), path.clone(), "".into()),
            ActionKind::OpenUrl { url } => ("Open URL".into(), url.clone(), "".into()),
            ActionKind::OpenFolder { path } => ("Open Folder".into(), path.clone(), "".into()),
            ActionKind::SetVar { name, value } => {
                ("Set Variable".into(), name.clone(), value.clone())
            }
            ActionKind::If { expr } => ("If".into(), expr.clone(), "".into()),
            ActionKind::Else => ("Else".into(), "".into(), "".into()),
            ActionKind::EndIf => ("End If".into(), "".into(), "".into()),
            ActionKind::For {
                var,
                from,
                to,
                step,
            } => (
                "For".into(),
                var.clone(),
                format!("{} to {} step {}", from, to, step),
            ),
            ActionKind::EndFor => ("End For".into(), "".into(), "".into()),
            ActionKind::While { expr } => ("While".into(), expr.clone(), "".into()),
            ActionKind::EndWhile => ("End While".into(), "".into(), "".into()),
            ActionKind::Label { name } => ("Label".into(), name.clone(), "".into()),
            ActionKind::Goto { name } => ("Goto".into(), name.clone(), "".into()),
            ActionKind::MessageBox { text } => ("Message Box".into(), text.clone(), "".into()),
            ActionKind::CallFunction { name } => ("Call Function".into(), name.clone(), "".into()),
            ActionKind::PlayScript { path } => ("Play Script".into(), path.clone(), "".into()),
            ActionKind::WaitTime { .. } => ("Wait Time".into(), "".into(), "".into()),
            ActionKind::RandomNumber { .. } => ("Random Number".into(), "".into(), "".into()),
            ActionKind::Command { cmd, .. } => ("Run Command".into(), cmd.clone(), "".into()),
        }
    }

    pub fn default_name(&self) -> String {
        match self {
            ActionKind::FunctionEntry => "Function Entry".into(),
            ActionKind::EndFunction => "End Function".into(),
            ActionKind::Comment { text } => {
                if text.is_empty() {
                    "Comment".into()
                } else {
                    format!("/* {} */", text)
                }
            }
            ActionKind::Delay { ms } => format!("Delay {} ms", ms),
            ActionKind::MouseMove { x, y } => format!("Mouse Move  ({}, {})", x, y),
            ActionKind::MouseClick {
                button,
                x,
                y,
                clicks,
            } => {
                if *clicks >= 2 {
                    format!("Mouse Double Click {}  ({}, {})", button.as_str(), x, y)
                } else {
                    format!("Mouse Click {}  ({}, {})", button.as_str(), x, y)
                }
            }
            ActionKind::MouseDrag {
                button,
                x1,
                y1,
                x2,
                y2,
            } => format!(
                "Mouse Drag {}  ({},{}) → ({},{})",
                button.as_str(),
                x1,
                y1,
                x2,
                y2
            ),
            ActionKind::MouseWheel { delta } => format!("Mouse Wheel  {}", delta),
            ActionKind::MouseDown { button, x, y } => {
                format!("Mouse Down {}  ({}, {})", button.as_str(), x, y)
            }
            ActionKind::MouseUp { button, x, y } => {
                format!("Mouse Up {}  ({}, {})", button.as_str(), x, y)
            }
            ActionKind::TypeText { text, .. } => {
                let t = if text.chars().count() > 40 {
                    let cut: String = text.chars().take(40).collect();
                    format!("{}…", cut)
                } else {
                    text.clone()
                };
                format!("Type Text  \"{}\"", t)
            }
            ActionKind::KeyPress { key } => format!("Key Press  [{}]", key),
            ActionKind::SmartClick { x, y, image, .. } => {
                if image.is_empty() {
                    format!("Smart Click  ({}, {})", x, y)
                } else {
                    format!("Smart Click  image + ({}, {})", x, y)
                }
            }
            ActionKind::SearchPicture { image, .. } => format!("Search Picture  {}", image),
            ActionKind::ActivateWindow { title } => format!("Activate Window  \"{}\"", title),
            ActionKind::CloseWindow { title } => format!("Close Window  \"{}\"", title),
            ActionKind::OpenFile { path } => format!("Open File  {}", path),
            ActionKind::OpenUrl { url } => format!("Open URL  {}", url),
            ActionKind::OpenFolder { path } => format!("Open Folder  {}", path),
            ActionKind::SetVar { name, value } => format!("Set {} = {}", name, value),
            ActionKind::If { expr } => format!("If  ({})", expr),
            ActionKind::Else => "Else".into(),
            ActionKind::EndIf => "End If".into(),
            ActionKind::For {
                var,
                from,
                to,
                step,
            } => format!("For {} = {} to {} step {}", var, from, to, step),
            ActionKind::EndFor => "End For".into(),
            ActionKind::While { expr } => format!("While  ({})", expr),
            ActionKind::EndWhile => "End While".into(),
            ActionKind::Label { name } => format!("Label  {}", name),
            ActionKind::Goto { name } => format!("Goto  {}", name),
            ActionKind::MessageBox { text } => format!("MessageBox  \"{}\"", text),
            ActionKind::CallFunction { name } => format!("Call  {}", name),
            ActionKind::PlayScript { path } => format!("Play Script  {}", path),
            ActionKind::WaitTime { hh, mm } => format!("WaitTime  {:02}:{:02}", hh, mm),
            ActionKind::RandomNumber { name, a, b } => format!("Random {} = {}..{}", name, a, b),
            ActionKind::Command { cmd } => format!("Command  {}", cmd),
        }
    }

    pub fn smart_click(x: i32, y: i32, image: String, timeout_ms: u64) -> Self {
        ActionKind::SmartClick {
            x,
            y,
            image,
            timeout_ms,
            confidence: 0.90,
            on_fail: "skip".into(),
            ox: None,
            oy: None,
        }
    }

    pub fn search_picture(image: String, timeout_ms: u64) -> Self {
        ActionKind::SearchPicture {
            image,
            timeout_ms,
            save_x: "x".into(),
            save_y: "y".into(),
            confidence: 0.90,
            on_fail: "stop".into(),
        }
    }

    pub fn indent_delta(&self) -> i32 {
        match self {
            ActionKind::FunctionEntry
            | ActionKind::If { .. }
            | ActionKind::For { .. }
            | ActionKind::While { .. } => 1,
            ActionKind::EndFunction
            | ActionKind::EndIf
            | ActionKind::EndFor
            | ActionKind::EndWhile => -1,
            ActionKind::Else => 0,
            _ => 0,
        }
    }

    pub fn is_structure_end(&self) -> bool {
        matches!(
            self,
            ActionKind::EndFunction | ActionKind::EndIf | ActionKind::EndFor | ActionKind::EndWhile
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub delay_ms: u64,
    pub kind: ActionKind,
}

impl Action {
    pub fn new(kind: ActionKind) -> Self {
        let name = kind.default_name();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            enabled: true,
            delay_ms: 0,
            kind,
        }
    }

    pub fn with_delay(mut self, ms: u64) -> Self {
        self.delay_ms = ms;
        self
    }

    #[allow(dead_code)]
    pub fn refresh_name(&mut self) {
        if self.name.is_empty() {
            self.name = self.kind.default_name();
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Script {
    pub version: String,
    pub name: String,
    pub actions: Vec<Action>,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            version: "1.0".into(),
            name: "Untitled".into(),
            actions: vec![
                Action::new(ActionKind::FunctionEntry),
                Action::new(ActionKind::Comment {
                    text: "Add some commands here.".into(),
                }),
                Action::new(ActionKind::EndFunction),
            ],
        }
    }
}

impl Script {
    pub fn load_json(s: &str) -> Result<Self, String> {
        if let Ok(sc) = serde_json::from_str::<Script>(s) {
            if !sc.actions.is_empty() {
                return Ok(sc);
            }
        }
        load_flexible_amk(s)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn indent_of(&self, index: usize) -> i32 {
        let mut indent = 0i32;
        for (i, a) in self.actions.iter().enumerate() {
            if i == index {
                if a.kind.is_structure_end() || matches!(a.kind, ActionKind::Else) {
                    return (indent - 1).max(0);
                }
                return indent.max(0);
            }
            indent += a.kind.indent_delta();
            if matches!(a.kind, ActionKind::Else) {
                // else stays at parent indent visually handled above
            }
            indent = indent.max(0);
        }
        indent.max(0)
    }

    pub fn optimize_record(&mut self) {
        let mut out: Vec<Action> = Vec::new();
        let mut last_move: Option<(i32, i32)> = None;
        for a in self.actions.drain(..) {
            match a.kind {
                ActionKind::MouseMove { x, y } => {
                    if let Some((lx, ly)) = last_move {
                        if (x - lx).abs() < 3 && (y - ly).abs() < 3 {
                            continue;
                        }
                        if let Some(prev) = out.last_mut() {
                            if let ActionKind::MouseMove { .. } = prev.kind {
                                prev.kind = ActionKind::MouseMove { x, y };
                                prev.name = prev.kind.default_name();
                                last_move = Some((x, y));
                                continue;
                            }
                        }
                    }
                    last_move = Some((x, y));
                    out.push(a);
                }
                ActionKind::Delay { ms } => {
                    if let Some(prev) = out.last_mut() {
                        if let ActionKind::Delay { ms: p } = &mut prev.kind {
                            *p += ms;
                            prev.name = prev.kind.default_name();
                            continue;
                        }
                    }
                    last_move = None;
                    out.push(a);
                }
                _ => {
                    last_move = None;
                    out.push(a);
                }
            }
        }
        self.actions = out;
    }
}

#[derive(Clone, Debug)]
pub struct AppOptions {
    pub play_speed: f32,
    pub sample_ms: u64,
    pub ignore_px: i32,
    pub minimize_on_play: bool,
    pub hk_record: String,
    pub hk_play: String,
    pub hk_pause: String,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            play_speed: 1.0,
            sample_ms: 30,
            ignore_px: 2,
            minimize_on_play: false,
            hk_record: "Shift+F2".into(),
            hk_play: "F9".into(),
            hk_pause: "Ctrl+P".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub name: String,
    pub script: String,
    pub when: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMode {
    Once,
    Times,
    Duration,
    Infinite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationUnit {
    Seconds,
    Minutes,
    Hours,
}

fn btn_from(s: &str) -> MouseBtn {
    match s.to_ascii_lowercase().as_str() {
        "right" => MouseBtn::Right,
        "middle" => MouseBtn::Middle,
        _ => MouseBtn::Left,
    }
}

fn i32_of(v: &Value, key: &str, default: i32) -> i32 {
    v.get(key)
        .and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)))
        .unwrap_or(default as i64) as i32
}

fn u64_of(v: &Value, key: &str, default: u64) -> u64 {
    v.get(key)
        .and_then(|x| x.as_u64().or_else(|| x.as_i64().map(|i| i.max(0) as u64)))
        .unwrap_or(default)
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| {
            x.as_str()
                .map(|s| s.to_string())
                .or_else(|| x.as_i64().map(|i| i.to_string()))
        })
        .unwrap_or_default()
}

fn kind_from_python(kind: &str, p: &Value) -> ActionKind {
    match kind {
        "fn_entry" | "functionentry" => ActionKind::FunctionEntry,
        "fn_end" | "endfunction" => ActionKind::EndFunction,
        "comment" => ActionKind::Comment {
            text: str_of(p, "text"),
        },
        "delay" => ActionKind::Delay {
            ms: u64_of(p, "ms", 0),
        },
        "move" | "mousemove" => ActionKind::MouseMove {
            x: i32_of(p, "x", 0),
            y: i32_of(p, "y", 0),
        },
        "click" | "mouseclick" => ActionKind::MouseClick {
            button: btn_from(&str_of(p, "button")),
            x: i32_of(p, "x", 0),
            y: i32_of(p, "y", 0),
            clicks: i32_of(p, "clicks", 1).max(1) as u8,
        },
        "down" | "mousedown" => ActionKind::MouseDown {
            button: btn_from(&str_of(p, "button")),
            x: i32_of(p, "x", 0),
            y: i32_of(p, "y", 0),
        },
        "up" | "mouseup" => ActionKind::MouseUp {
            button: btn_from(&str_of(p, "button")),
            x: i32_of(p, "x", 0),
            y: i32_of(p, "y", 0),
        },
        "wheel" | "mousewheel" => ActionKind::MouseWheel {
            delta: i32_of(p, "delta", 0),
        },
        "type" | "typetext" => ActionKind::TypeText {
            text: str_of(p, "text"),
            interval_ms: u64_of(p, "interval", u64_of(p, "interval_ms", 20)),
        },
        "key" | "keypress" => ActionKind::KeyPress {
            key: str_of(p, "key"),
        },
        "smart" | "smartclick" => {
            let tgt = p
                .get("targets")
                .and_then(|t| t.as_array())
                .and_then(|a| a.first());
            let image = {
                let s = str_of(p, "image");
                if !s.is_empty() {
                    s
                } else if let Some(t) = tgt {
                    str_of(t, "image")
                } else {
                    String::new()
                }
            };
            let x = if p.get("x").is_some() {
                i32_of(p, "x", 0)
            } else if let Some(t) = tgt {
                i32_of(t, "x", 0)
            } else {
                0
            };
            let y = if p.get("y").is_some() {
                i32_of(p, "y", 0)
            } else if let Some(t) = tgt {
                i32_of(t, "y", 0)
            } else {
                0
            };
            let mut k = ActionKind::smart_click(
                x,
                y,
                image,
                u64_of(p, "timeout", u64_of(p, "timeout_ms", 2500)),
            );
            if let ActionKind::SmartClick {
                confidence,
                on_fail,
                ..
            } = &mut k
            {
                if let Some(c) = p.get("confidence").and_then(|x| x.as_f64()) {
                    *confidence = c as f32;
                }
                let f = str_of(p, "on_fail");
                if !f.is_empty() {
                    *on_fail = f;
                }
            }
            if let ActionKind::SmartClick { ox, oy, .. } = &mut k {
                if p.get("ox").is_some() {
                    *ox = Some(i32_of(p, "ox", 0));
                }
                if p.get("oy").is_some() {
                    *oy = Some(i32_of(p, "oy", 0));
                }
            }
            k
        }
        "search" | "searchpicture" => {
            let mut k = ActionKind::search_picture(
                str_of(p, "image"),
                u64_of(p, "timeout", u64_of(p, "timeout_ms", 10000)),
            );
            if let ActionKind::SearchPicture {
                on_fail,
                confidence,
                ..
            } = &mut k
            {
                let f = str_of(p, "on_fail");
                if !f.is_empty() {
                    *on_fail = f;
                }
                if let Some(c) = p.get("confidence").and_then(|x| x.as_f64()) {
                    *confidence = c as f32;
                }
            }
            k
        }
        "open" | "openfile" | "openfolder" => ActionKind::OpenFile {
            path: str_of(p, "path"),
        },
        "url" | "openurl" => ActionKind::OpenUrl {
            url: str_of(p, "url"),
        },
        "if" => ActionKind::If {
            expr: str_of(p, "expr"),
        },
        "else" => ActionKind::Else,
        "endif" => ActionKind::EndIf,
        "for" => ActionKind::For {
            var: {
                let v = str_of(p, "var");
                if v.is_empty() {
                    "i".into()
                } else {
                    v
                }
            },
            from: i32_of(p, "from", 1) as i64,
            to: i32_of(p, "to", 1) as i64,
            step: i32_of(p, "step", 1) as i64,
        },
        "endfor" => ActionKind::EndFor,
        "setvar" => ActionKind::SetVar {
            name: str_of(p, "name"),
            value: str_of(p, "value"),
        },
        "activate" | "activatewindow" => ActionKind::ActivateWindow {
            title: str_of(p, "title"),
        },
        "closewin" | "closewindow" => ActionKind::CloseWindow {
            title: str_of(p, "title"),
        },
        "waittime" => ActionKind::WaitTime {
            hh: str_of(p, "hh").parse().unwrap_or(0),
            mm: str_of(p, "mm").parse().unwrap_or(0),
        },
        "random" | "randomnumber" => ActionKind::RandomNumber {
            name: {
                let n = str_of(p, "name");
                if n.is_empty() {
                    "n".into()
                } else {
                    n
                }
            },
            a: i32_of(p, "a", 1) as i64,
            b: i32_of(p, "b", 10) as i64,
        },
        "command" => ActionKind::Command {
            cmd: str_of(p, "cmd"),
        },
        other => ActionKind::Comment {
            text: format!("unknown kind {other}"),
        },
    }
}

fn action_from_value(v: &Value) -> Option<Action> {
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let enabled = v.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true);
    let delay_ms = v.get("delay_ms").and_then(|x| x.as_u64()).unwrap_or(0);
    let id = v
        .get("id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let kind_v = v.get("kind")?;
    let kind = if let Some(s) = kind_v.as_str() {
        let params = v
            .get("params")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));
        kind_from_python(&s.to_ascii_lowercase(), &params)
    } else if kind_v.is_object() {
        if let Ok(k) = serde_json::from_value::<ActionKind>(kind_v.clone()) {
            k
        } else if let Some((name, val)) = kind_v.as_object().and_then(|o| {
            if o.len() == 1 {
                o.iter().next().map(|(k, v)| (k.clone(), v.clone()))
            } else {
                None
            }
        }) {
            kind_from_python(&name.to_ascii_lowercase(), &val)
        } else {
            return None;
        }
    } else {
        return None;
    };
    Some(Action {
        id,
        name,
        enabled,
        delay_ms,
        kind,
    })
}

fn load_flexible_amk(s: &str) -> Result<Script, String> {
    let v: Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("Untitled")
        .to_string();
    let version = v
        .get("version")
        .and_then(|x| x.as_str())
        .unwrap_or("1.0")
        .to_string();
    let list = if let Some(arr) = v.get("actions").and_then(|x| x.as_array()) {
        arr.clone()
    } else if let Some(arr) = v.as_array() {
        arr.clone()
    } else {
        return Err("no actions".into());
    };
    let actions: Vec<Action> = list.iter().filter_map(action_from_value).collect();
    if actions.is_empty() {
        return Err("no valid actions".into());
    }
    Ok(Script {
        version,
        name,
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_hello_sample_style() {
        let s = r#"{
          "version":"1.0","name":"hello",
          "actions":[
            {"id":"a1","name":"Function Entry","enabled":true,"delay_ms":0,"kind":"FunctionEntry"},
            {"id":"a2","name":"c","enabled":true,"delay_ms":0,"kind":{"Comment":{"text":"Demo"}}},
            {"id":"a3","name":"d","enabled":true,"delay_ms":0,"kind":{"Delay":{"ms":400}}},
            {"id":"a4","name":"t","enabled":true,"delay_ms":0,"kind":{"TypeText":{"text":"hi","interval_ms":25}}},
            {"id":"a5","name":"End Function","enabled":true,"delay_ms":0,"kind":"EndFunction"}
          ]
        }"#;
        let sc = Script::load_json(s).expect("load");
        assert_eq!(sc.actions.len(), 5);
        assert!(matches!(sc.actions[0].kind, ActionKind::FunctionEntry));
        assert!(matches!(sc.actions[2].kind, ActionKind::Delay { ms: 400 }));
    }

    #[test]
    fn loads_python_kind_params() {
        let s = r#"{
          "version":"1.0","name":"py",
          "actions":[
            {"kind":"fn_entry","name":"","enabled":true,"delay_ms":0,"params":{}},
            {"kind":"click","name":"","enabled":true,"delay_ms":10,"params":{"x":5,"y":6,"button":"left","clicks":1}},
            {"kind":"smart","name":"s","enabled":true,"delay_ms":0,"params":{"x":1,"y":2,"image":"a.bmp","confidence":0.9}},
            {"kind":"fn_end","name":"","enabled":true,"delay_ms":0,"params":{}}
          ]
        }"#;
        let sc = Script::load_json(s).expect("load");
        assert!(matches!(
            sc.actions[1].kind,
            ActionKind::MouseClick { x: 5, y: 6, .. }
        ));
        assert!(matches!(sc.actions[2].kind, ActionKind::SmartClick { .. }));
    }

    #[test]
    fn loads_python_smart_targets_and_confidence() {
        let s = r#"{
          "version":"1.0","name":"s",
          "actions":[
            {"kind":"smart","enabled":true,"delay_ms":0,"params":{
              "confidence":0.93,"on_fail":"stop",
              "targets":[{"x":11,"y":22,"image":"captures/a.bmp"}]
            }}
          ]
        }"#;
        let sc = Script::load_json(s).expect("load");
        match &sc.actions[0].kind {
            ActionKind::SmartClick {
                x,
                y,
                image,
                confidence,
                on_fail,
                ..
            } => {
                assert_eq!(*x, 11);
                assert_eq!(*y, 22);
                assert!(image.contains("a.bmp"));
                assert!((*confidence - 0.93).abs() < 0.001);
                assert_eq!(on_fail, "stop");
            }
            other => panic!("{:?}", other),
        }
    }
}
