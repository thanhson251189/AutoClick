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
fn default_rnd_x() -> String {
    "rnd_x".into()
}
fn default_rnd_y() -> String {
    "rnd_y".into()
}
fn default_clip_name() -> String {
    "clip".into()
}
fn default_wait_clip_timeout() -> u64 {
    30_000
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
        /// Move duration in ms: 0 = jump instantly, > 0 = glide like AMK.
        #[serde(default)]
        ms: u64,
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
        /// Drag duration in ms: 0 = instant, > 0 = glide like AMK.
        #[serde(default)]
        ms: u64,
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
    KeyDown {
        key: String,
    },
    KeyUp {
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
        /// Search-region size; `x,y` is top-left when set.
        #[serde(default)]
        rw: Option<i32>,
        #[serde(default)]
        rh: Option<i32>,
        /// Where the picture was when it was captured. Search starts here.
        #[serde(default)]
        px: Option<i32>,
        #[serde(default)]
        py: Option<i32>,
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
    /// Pick a random point in the rectangle and move the mouse there.
    RandomMouse {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        #[serde(default = "default_rnd_x")]
        save_x: String,
        #[serde(default = "default_rnd_y")]
        save_y: String,
    },
    /// Read a value from a JSON file by dot path (`a.b.0.c`) into a variable.
    ReadJson {
        file: String,
        query: String,
        name: String,
    },
    /// Wait until the clipboard text changes, then store it in a variable.
    WaitClipboard {
        #[serde(default)]
        exclude: String,
        #[serde(default = "default_clip_name")]
        save_name: String,
        #[serde(default = "default_wait_clip_timeout")]
        timeout_ms: u64,
        #[serde(default = "default_search_fail")]
        on_fail: String,
    },
    /// Play one random .amk script from a folder.
    PlayRandom {
        folder: String,
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
    WaitWindow {
        title: String,
        timeout_ms: u64,
        #[serde(default = "default_search_fail")]
        on_fail: String,
    },
    SetClipboard {
        text: String,
    },
    GetClipboard {
        name: String,
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
    Break,
    Continue,
    Switch {
        expr: String,
    },
    Case {
        value: String,
    },
    DefaultCase,
    EndSwitch,
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
    /// Write a value into a JSON file at a dot path (`a.b.0.c`), creating the
    /// file and intermediate objects/arrays as needed.
    WriteJson {
        file: String,
        query: String,
        value: String,
    },
    /// Read a registry string value into a variable. Path: `HKCU\Software\...`.
    ReadRegistry {
        path: String,
        value: String,
        name: String,
    },
    /// Write a registry string value. Path: `HKCU\Software\...`.
    WriteRegistry {
        path: String,
        value: String,
        data: String,
    },
    MinimizeWindow {
        title: String,
    },
    MaximizeWindow {
        title: String,
    },
    RestoreWindow {
        title: String,
    },
    /// Put HTML on the clipboard (Windows "HTML Format").
    SetClipboardHtml {
        html: String,
    },
    /// Read HTML from the clipboard into a variable.
    GetClipboardHtml {
        name: String,
    },
}

impl ActionKind {
    pub fn type_key(&self) -> &'static str {
        match self {
            ActionKind::FunctionEntry => "act_function",
            ActionKind::EndFunction => "act_end",
            ActionKind::Comment { .. } => "act_comment",
            ActionKind::Delay { .. } => "act_delay",
            ActionKind::MouseMove { .. } => "act_mouse_move",
            ActionKind::MouseClick { .. } => "act_mouse_click",
            ActionKind::MouseDrag { .. } => "act_mouse_drag",
            ActionKind::MouseWheel { .. } => "act_mouse_wheel",
            ActionKind::TypeText { .. } => "act_type",
            ActionKind::KeyPress { .. } => "act_key",
            ActionKind::MouseDown { .. } => "act_mouse_down",
            ActionKind::MouseUp { .. } => "act_mouse_up",
            ActionKind::KeyDown { .. } => "act_key_down",
            ActionKind::KeyUp { .. } => "act_key_up",
            ActionKind::SmartClick { .. } => "act_smart",
            ActionKind::SearchPicture { .. } => "act_search",
            ActionKind::WaitTime { .. } => "act_wait_time",
            ActionKind::RandomNumber { .. } => "act_random",
            ActionKind::Command { .. } => "act_cmd",
            ActionKind::ActivateWindow { .. } => "act_activate",
            ActionKind::CloseWindow { .. } => "act_close",
            ActionKind::WaitWindow { .. } => "act_wait_win",
            ActionKind::SetClipboard { .. } => "act_set_clip",
            ActionKind::GetClipboard { .. } => "act_get_clip",
            ActionKind::OpenFile { .. } => "act_open_file",
            ActionKind::OpenUrl { .. } => "act_open_url",
            ActionKind::OpenFolder { .. } => "act_open_folder",
            ActionKind::SetVar { .. } => "act_set_var",
            ActionKind::If { .. } => "act_if",
            ActionKind::Else => "act_else",
            ActionKind::EndIf => "act_endif",
            ActionKind::For { .. } => "act_for",
            ActionKind::EndFor => "act_endfor",
            ActionKind::While { .. } => "act_while",
            ActionKind::EndWhile => "act_endwhile",
            ActionKind::Break => "act_break",
            ActionKind::Continue => "act_continue",
            ActionKind::Switch { .. } => "act_switch",
            ActionKind::Case { .. } => "act_case",
            ActionKind::DefaultCase => "act_case_default",
            ActionKind::EndSwitch => "act_endswitch",
            ActionKind::RandomMouse { .. } => "act_random_mouse",
            ActionKind::ReadJson { .. } => "act_read_json",
            ActionKind::WaitClipboard { .. } => "act_wait_clip",
            ActionKind::PlayRandom { .. } => "act_play_random",
            ActionKind::Label { .. } => "act_label",
            ActionKind::Goto { .. } => "act_goto",
            ActionKind::MessageBox { .. } => "act_msg",
            ActionKind::CallFunction { .. } => "act_call",
            ActionKind::PlayScript { .. } => "act_play_script",
            ActionKind::WriteJson { .. } => "act_write_json",
            ActionKind::ReadRegistry { .. } => "act_read_reg",
            ActionKind::WriteRegistry { .. } => "act_write_reg",
            ActionKind::MinimizeWindow { .. } => "act_win_min",
            ActionKind::MaximizeWindow { .. } => "act_win_max",
            ActionKind::RestoreWindow { .. } => "act_win_restore",
            ActionKind::SetClipboardHtml { .. } => "act_set_clip_html",
            ActionKind::GetClipboardHtml { .. } => "act_get_clip_html",
        }
    }

    pub fn format_columns(&self) -> (String, String, String) {
        match self {
            ActionKind::FunctionEntry => ("Function".into(), "".into(), "Entry".into()),
            ActionKind::EndFunction => ("End".into(), "Function".into(), "".into()),
            ActionKind::Comment { text } => ("Comment".into(), "".into(), text.clone()),
            ActionKind::Delay { ms } => ("Delay".into(), format!("{} ms", ms), "".into()),
            ActionKind::MouseMove { x, y, ms } => (
                "Mouse Move".into(),
                format!("X: {}, Y: {}", x, y),
                if *ms > 0 {
                    format!("Duration: {} ms", ms)
                } else {
                    "".into()
                },
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
                ms,
            } => (
                "Mouse Drag".into(),
                format!("{},{} -> {},{}", x1, y1, x2, y2),
                if *ms > 0 {
                    format!("{} Button, {} ms", button.as_str(), ms)
                } else {
                    format!("{} Button", button.as_str())
                },
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
            ActionKind::KeyDown { key } => ("Key Down".into(), key.clone(), "".into()),
            ActionKind::KeyUp { key } => ("Key Up".into(), key.clone(), "".into()),
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
            ActionKind::WaitWindow {
                title, timeout_ms, ..
            } => (
                "Wait Window".into(),
                title.clone(),
                format!("Timeout: {} ms", timeout_ms),
            ),
            ActionKind::SetClipboard { text } => ("Set Clipboard".into(), text.clone(), "".into()),
            ActionKind::GetClipboard { name } => ("Get Clipboard".into(), name.clone(), "".into()),
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
            ActionKind::Break => ("Break".into(), "".into(), "".into()),
            ActionKind::Continue => ("Continue".into(), "".into(), "".into()),
            ActionKind::Switch { expr } => ("Switch".into(), expr.clone(), "".into()),
            ActionKind::Case { value } => ("Case".into(), value.clone(), "".into()),
            ActionKind::DefaultCase => ("Default".into(), "".into(), "".into()),
            ActionKind::EndSwitch => ("End Switch".into(), "".into(), "".into()),
            ActionKind::RandomMouse { x1, y1, x2, y2, .. } => (
                "Random Mouse".into(),
                format!("{},{} -> {},{}", x1, y1, x2, y2),
                "".into(),
            ),
            ActionKind::ReadJson { file, query, .. } => {
                ("Read JSON".into(), file.clone(), query.clone())
            }
            ActionKind::WaitClipboard {
                save_name,
                timeout_ms,
                ..
            } => (
                "Wait Clipboard".into(),
                save_name.clone(),
                format!("Timeout: {} ms", timeout_ms),
            ),
            ActionKind::PlayRandom { folder } => ("Play Random".into(), folder.clone(), "".into()),
            ActionKind::Label { name } => ("Label".into(), name.clone(), "".into()),
            ActionKind::Goto { name } => ("Goto".into(), name.clone(), "".into()),
            ActionKind::MessageBox { text } => ("Message Box".into(), text.clone(), "".into()),
            ActionKind::CallFunction { name } => ("Call Function".into(), name.clone(), "".into()),
            ActionKind::PlayScript { path } => ("Play Script".into(), path.clone(), "".into()),
            ActionKind::WaitTime { .. } => ("Wait Time".into(), "".into(), "".into()),
            ActionKind::RandomNumber { .. } => ("Random Number".into(), "".into(), "".into()),
            ActionKind::Command { cmd, .. } => ("Run Command".into(), cmd.clone(), "".into()),
            ActionKind::WriteJson { file, query, .. } => {
                ("Write JSON".into(), file.clone(), query.clone())
            }
            ActionKind::ReadRegistry { path, value, .. } => {
                ("Read Registry".into(), path.clone(), value.clone())
            }
            ActionKind::WriteRegistry { path, value, .. } => {
                ("Write Registry".into(), path.clone(), value.clone())
            }
            ActionKind::MinimizeWindow { title } => {
                ("Minimize Window".into(), title.clone(), "".into())
            }
            ActionKind::MaximizeWindow { title } => {
                ("Maximize Window".into(), title.clone(), "".into())
            }
            ActionKind::RestoreWindow { title } => {
                ("Restore Window".into(), title.clone(), "".into())
            }
            ActionKind::SetClipboardHtml { html } => {
                let shown: String = html.chars().take(40).collect();
                ("Set Clipboard HTML".into(), shown, "".into())
            }
            ActionKind::GetClipboardHtml { name } => {
                ("Get Clipboard HTML".into(), name.clone(), "".into())
            }
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
            ActionKind::MouseMove { x, y, .. } => format!("Mouse Move  ({}, {})", x, y),
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
                ..
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
            ActionKind::KeyDown { key } => format!("Key Down  [{}]", key),
            ActionKind::KeyUp { key } => format!("Key Up  [{}]", key),
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
            ActionKind::WaitWindow { title, .. } => format!("Wait Window  \"{}\"", title),
            ActionKind::SetClipboard { text } => format!("Set Clipboard  \"{}\"", text),
            ActionKind::GetClipboard { name } => format!("Get Clipboard  ->  {}", name),
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
            ActionKind::Break => "Break".into(),
            ActionKind::Continue => "Continue".into(),
            ActionKind::Switch { expr } => format!("Switch  ({})", expr),
            ActionKind::Case { value } => format!("Case  {}", value),
            ActionKind::DefaultCase => "Default".into(),
            ActionKind::EndSwitch => "End Switch".into(),
            ActionKind::RandomMouse { x1, y1, x2, y2, .. } => {
                format!("Random Mouse  ({},{}) -> ({},{})", x1, y1, x2, y2)
            }
            ActionKind::ReadJson { file, query, .. } => format!("Read JSON  {} :: {}", file, query),
            ActionKind::WaitClipboard { save_name, .. } => {
                format!("Wait Clipboard  ->  {}", save_name)
            }
            ActionKind::PlayRandom { folder } => format!("Play Random  {}", folder),
            ActionKind::Label { name } => format!("Label  {}", name),
            ActionKind::Goto { name } => format!("Goto  {}", name),
            ActionKind::MessageBox { text } => format!("MessageBox  \"{}\"", text),
            ActionKind::CallFunction { name } => format!("Call  {}", name),
            ActionKind::PlayScript { path } => format!("Play Script  {}", path),
            ActionKind::WaitTime { hh, mm } => format!("WaitTime  {:02}:{:02}", hh, mm),
            ActionKind::RandomNumber { name, a, b } => format!("Random {} = {}..{}", name, a, b),
            ActionKind::Command { cmd } => format!("Command  {}", cmd),
            ActionKind::WriteJson { file, query, .. } => {
                format!("Write JSON  {} :: {}", file, query)
            }
            ActionKind::ReadRegistry { path, value, .. } => {
                format!("Read Registry  {} :: {}", path, value)
            }
            ActionKind::WriteRegistry { path, value, .. } => {
                format!("Write Registry  {} :: {}", path, value)
            }
            ActionKind::MinimizeWindow { title } => format!("Minimize Window  {}", title),
            ActionKind::MaximizeWindow { title } => format!("Maximize Window  {}", title),
            ActionKind::RestoreWindow { title } => format!("Restore Window  {}", title),
            ActionKind::SetClipboardHtml { html } => {
                let shown: String = html.chars().take(40).collect();
                format!("Set Clipboard HTML  \"{}\"", shown)
            }
            ActionKind::GetClipboardHtml { name } => format!("Get Clipboard HTML  ->  {}", name),
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
            rw: None,
            rh: None,
            px: None,
            py: None,
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
            | ActionKind::While { .. }
            | ActionKind::Switch { .. } => 1,
            ActionKind::EndFunction
            | ActionKind::EndIf
            | ActionKind::EndFor
            | ActionKind::EndWhile
            | ActionKind::EndSwitch => -1,
            ActionKind::Else => 0,
            _ => 0,
        }
    }

    pub fn is_structure_end(&self) -> bool {
        matches!(
            self,
            ActionKind::EndFunction
                | ActionKind::EndIf
                | ActionKind::EndFor
                | ActionKind::EndWhile
                | ActionKind::EndSwitch
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

    pub fn optimize_record(&mut self) {
        let mut out: Vec<Action> = Vec::new();
        let mut last_move: Option<(i32, i32)> = None;
        for a in self.actions.drain(..) {
            match a.kind {
                ActionKind::MouseMove { x, y, .. } => {
                    if let Some((lx, ly)) = last_move {
                        if (x - lx).abs() < 3 && (y - ly).abs() < 3 {
                            // Keep the dropped move's gap on the previous step.
                            if let Some(prev) = out.last_mut() {
                                prev.delay_ms += a.delay_ms;
                            }
                            continue;
                        }
                        if let Some(prev) = out.last_mut() {
                            if let ActionKind::MouseMove { ms: pms, .. } = prev.kind {
                                // Merging two moves keeps the total timeline:
                                // the second gap must not vanish.
                                prev.delay_ms += a.delay_ms;
                                prev.kind = ActionKind::MouseMove { x, y, ms: pms };
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
                            prev.delay_ms += a.delay_ms;
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

    /// AMK "Rename a variable": renames every assignment, `{name}` reference,
    /// and whole-word use inside If/While/Switch expressions. Returns how many
    /// fields changed.
    pub fn rename_variable(&mut self, from: &str, to: &str) -> usize {
        if from.is_empty() || to.is_empty() || from == to {
            return 0;
        }
        let brace_from = format!("{{{from}}}");
        let brace_to = format!("{{{to}}}");
        let mut count = 0usize;
        for a in &mut self.actions {
            count += a.kind.rename_variable(from, to, &brace_from, &brace_to);
        }
        count
    }
}

fn rename_name(slot: &mut String, from: &str, to: &str, count: &mut usize) {
    if slot == from {
        *slot = to.to_string();
        *count += 1;
    }
}

fn rename_braces(slot: &mut String, brace_from: &str, brace_to: &str, count: &mut usize) {
    if slot.contains(brace_from) {
        *slot = slot.replace(brace_from, brace_to);
        *count += 1;
    }
}

/// Whole-word replacement for expressions: `n + 1` and `{n}` both match `n`,
/// but `xn` or a quoted `"n"` inside a longer token does not.
fn rename_expr(slot: &mut String, from: &str, to: &str, count: &mut usize) {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(slot.len());
    let mut rest = slot.as_str();
    let mut changed = false;
    while let Some(i) = rest.find(from) {
        let before_ok = rest[..i].chars().next_back().is_none_or(|c| !is_word(c));
        let after = &rest[i + from.len()..];
        let after_ok = after.chars().next().is_none_or(|c| !is_word(c));
        if before_ok && after_ok {
            out.push_str(&rest[..i]);
            out.push_str(to);
            rest = after;
            changed = true;
        } else {
            let skip = rest[i..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&rest[..i + skip]);
            rest = &rest[i + skip..];
        }
    }
    if changed {
        out.push_str(rest);
        *slot = out;
        *count += 1;
    }
}

impl ActionKind {
    fn rename_variable(&mut self, from: &str, to: &str, brace_from: &str, brace_to: &str) -> usize {
        let mut count = 0usize;
        match self {
            ActionKind::SetVar { name, value } => {
                rename_name(name, from, to, &mut count);
                rename_braces(value, brace_from, brace_to, &mut count);
            }
            ActionKind::For { var, .. } => rename_name(var, from, to, &mut count),
            ActionKind::RandomNumber { name, .. } => rename_name(name, from, to, &mut count),
            ActionKind::GetClipboard { name } => rename_name(name, from, to, &mut count),
            ActionKind::GetClipboardHtml { name } => rename_name(name, from, to, &mut count),
            ActionKind::ReadJson { file, query, name } => {
                rename_name(name, from, to, &mut count);
                rename_braces(file, brace_from, brace_to, &mut count);
                rename_braces(query, brace_from, brace_to, &mut count);
            }
            ActionKind::WriteJson { file, query, value } => {
                rename_braces(file, brace_from, brace_to, &mut count);
                rename_braces(query, brace_from, brace_to, &mut count);
                rename_braces(value, brace_from, brace_to, &mut count);
            }
            ActionKind::ReadRegistry { path, value, name } => {
                rename_name(name, from, to, &mut count);
                rename_braces(path, brace_from, brace_to, &mut count);
                rename_braces(value, brace_from, brace_to, &mut count);
            }
            ActionKind::WriteRegistry { path, value, data } => {
                rename_braces(path, brace_from, brace_to, &mut count);
                rename_braces(value, brace_from, brace_to, &mut count);
                rename_braces(data, brace_from, brace_to, &mut count);
            }
            ActionKind::WaitClipboard {
                exclude, save_name, ..
            } => {
                rename_name(save_name, from, to, &mut count);
                rename_braces(exclude, brace_from, brace_to, &mut count);
            }
            ActionKind::SearchPicture {
                image,
                save_x,
                save_y,
                ..
            } => {
                rename_braces(image, brace_from, brace_to, &mut count);
                rename_name(save_x, from, to, &mut count);
                rename_name(save_y, from, to, &mut count);
            }
            ActionKind::SmartClick { image, .. } => {
                rename_braces(image, brace_from, brace_to, &mut count)
            }
            ActionKind::RandomMouse { save_x, save_y, .. } => {
                rename_name(save_x, from, to, &mut count);
                rename_name(save_y, from, to, &mut count);
            }
            ActionKind::TypeText { text, .. } => {
                rename_braces(text, brace_from, brace_to, &mut count)
            }
            ActionKind::KeyPress { key }
            | ActionKind::KeyDown { key }
            | ActionKind::KeyUp { key } => rename_braces(key, brace_from, brace_to, &mut count),
            ActionKind::Command { cmd } => rename_braces(cmd, brace_from, brace_to, &mut count),
            ActionKind::ActivateWindow { title }
            | ActionKind::CloseWindow { title }
            | ActionKind::MinimizeWindow { title }
            | ActionKind::MaximizeWindow { title }
            | ActionKind::RestoreWindow { title }
            | ActionKind::WaitWindow { title, .. } => {
                rename_braces(title, brace_from, brace_to, &mut count)
            }
            ActionKind::SetClipboard { text } => {
                rename_braces(text, brace_from, brace_to, &mut count)
            }
            ActionKind::SetClipboardHtml { html } => {
                rename_braces(html, brace_from, brace_to, &mut count)
            }
            ActionKind::OpenFile { path }
            | ActionKind::OpenFolder { path }
            | ActionKind::PlayScript { path } => {
                rename_braces(path, brace_from, brace_to, &mut count)
            }
            ActionKind::OpenUrl { url } => rename_braces(url, brace_from, brace_to, &mut count),
            ActionKind::PlayRandom { folder } => {
                rename_braces(folder, brace_from, brace_to, &mut count)
            }
            ActionKind::If { expr } | ActionKind::While { expr } | ActionKind::Switch { expr } => {
                rename_expr(expr, from, to, &mut count)
            }
            ActionKind::Case { value } => rename_braces(value, brace_from, brace_to, &mut count),
            _ => {}
        }
        count
    }
}

// Field-level defaults: a session.json whose `options` object misses one key
// must fill in that key, not wipe the whole saved session.
fn d_play_speed() -> f32 {
    1.0
}
fn d_sample_ms() -> u64 {
    30
}
fn d_ignore_px() -> i32 {
    2
}
fn d_minimize_on_play() -> bool {
    false
}
fn d_hk_record() -> String {
    "F9".into()
}
fn d_hk_play() -> String {
    "F10".into()
}
fn d_hk_stop() -> String {
    "F12".into()
}
fn d_hk_pause() -> String {
    "Ctrl+P".into()
}
fn d_hk_step_into() -> String {
    "F7".into()
}
fn d_hk_step_over() -> String {
    "F8".into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppOptions {
    #[serde(default = "d_play_speed")]
    pub play_speed: f32,
    #[serde(default = "d_sample_ms")]
    pub sample_ms: u64,
    #[serde(default = "d_ignore_px")]
    pub ignore_px: i32,
    #[serde(default = "d_minimize_on_play")]
    pub minimize_on_play: bool,
    #[serde(default = "d_hk_record")]
    pub hk_record: String,
    #[serde(default = "d_hk_play")]
    pub hk_play: String,
    #[serde(default = "d_hk_stop")]
    pub hk_stop: String,
    #[serde(default = "d_hk_pause")]
    pub hk_pause: String,
    #[serde(default = "d_hk_step_into")]
    pub hk_step_into: String,
    #[serde(default = "d_hk_step_over")]
    pub hk_step_over: String,
}

impl Default for AppOptions {
    fn default() -> Self {
        Self {
            play_speed: 1.0,
            sample_ms: 30,
            ignore_px: 2,
            minimize_on_play: false,
            hk_record: "F9".into(),
            hk_play: "F10".into(),
            hk_stop: "F12".into(),
            hk_pause: "Ctrl+P".into(),
            hk_step_into: "F7".into(),
            hk_step_over: "F8".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScheduledTask {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub script: String,
    #[serde(default)]
    pub when: String,
}

/// Parse `HH:MM` (also `H:MM`). None if the string is not a clock time.
pub fn parse_hhmm(when: &str) -> Option<(u32, u32)> {
    let when = when.trim();
    let (h, m) = when.split_once(':')?;
    let h: u32 = h.trim().parse().ok()?;
    let m: u32 = m.trim().parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some((h, m))
}

/// True when `when` matches this clock minute and the task has not already
/// fired today. Used by the live scheduler tick while the app is open.
pub fn scheduled_task_due(when: &str, hour: u32, minute: u32, already_fired_today: bool) -> bool {
    match parse_hhmm(when) {
        Some((h, m)) => !already_fired_today && h == hour && m == minute,
        None => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatMode {
    Once,
    Times,
    Duration,
    Infinite,
}

impl RepeatMode {
    pub fn code(self) -> &'static str {
        match self {
            RepeatMode::Once => "once",
            RepeatMode::Times => "times",
            RepeatMode::Duration => "duration",
            RepeatMode::Infinite => "infinite",
        }
    }

    pub fn from_code(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "times" => RepeatMode::Times,
            "duration" => RepeatMode::Duration,
            "infinite" => RepeatMode::Infinite,
            _ => RepeatMode::Once,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationUnit {
    Seconds,
    Minutes,
    Hours,
}

impl DurationUnit {
    pub fn code(self) -> &'static str {
        match self {
            DurationUnit::Seconds => "seconds",
            DurationUnit::Minutes => "minutes",
            DurationUnit::Hours => "hours",
        }
    }

    pub fn from_code(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "seconds" => DurationUnit::Seconds,
            "hours" => DurationUnit::Hours,
            _ => DurationUnit::Minutes,
        }
    }
}

fn default_session_lang() -> String {
    "vi".into()
}
fn default_session_repeat() -> String {
    "once".into()
}
fn default_session_repeat_n() -> u32 {
    10
}
fn default_session_duration_n() -> u32 {
    1
}
fn default_session_duration_unit() -> String {
    "minutes".into()
}
fn default_session_speed_enabled() -> bool {
    true
}

/// Language, hotkeys, repeat, and scheduled tasks. Restored on the next launch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppSession {
    #[serde(default = "default_session_lang")]
    pub lang: String,
    #[serde(default)]
    pub options: AppOptions,
    #[serde(default)]
    pub tasks: Vec<ScheduledTask>,
    #[serde(default = "default_session_repeat")]
    pub repeat: String,
    #[serde(default = "default_session_repeat_n")]
    pub repeat_n: u32,
    #[serde(default = "default_session_duration_n")]
    pub duration_n: u32,
    #[serde(default = "default_session_duration_unit")]
    pub duration_unit: String,
    #[serde(default = "default_session_speed_enabled")]
    pub speed_enabled: bool,
}

impl Default for AppSession {
    fn default() -> Self {
        Self {
            lang: default_session_lang(),
            options: AppOptions::default(),
            tasks: Vec::new(),
            repeat: default_session_repeat(),
            repeat_n: default_session_repeat_n(),
            duration_n: default_session_duration_n(),
            duration_unit: default_session_duration_unit(),
            speed_enabled: default_session_speed_enabled(),
        }
    }
}

impl AppSession {
    pub fn repeat_mode(&self) -> RepeatMode {
        RepeatMode::from_code(&self.repeat)
    }

    pub fn duration_unit(&self) -> DurationUnit {
        DurationUnit::from_code(&self.duration_unit)
    }

    pub fn load(path: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self, path: &std::path::Path) -> bool {
        let Ok(text) = serde_json::to_string_pretty(self) else {
            return false;
        };
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() && std::fs::create_dir_all(dir).is_err() {
                return false;
            }
        }
        std::fs::write(path, text).is_ok()
    }
}

pub fn session_path() -> std::path::PathBuf {
    if let Some(base) = std::env::var_os("APPDATA") {
        return std::path::PathBuf::from(base)
            .join("AutomaticMouseKeyboard")
            .join("session.json");
    }
    std::path::PathBuf::from("amk-session.json")
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
        .unwrap_or(default as i64)
        .clamp(i32::MIN as i64, i32::MAX as i64) as i32
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
                .or_else(|| x.as_f64().map(|f| f.to_string()))
                .or_else(|| x.as_bool().map(|b| b.to_string()))
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
            ms: u64_of(p, "ms", 0),
        },
        "click" | "mouseclick" => ActionKind::MouseClick {
            button: btn_from(&str_of(p, "button")),
            x: i32_of(p, "x", 0),
            y: i32_of(p, "y", 0),
            clicks: i32_of(p, "clicks", 1).clamp(1, 255) as u8,
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
        "keydown" => ActionKind::KeyDown {
            key: str_of(p, "key"),
        },
        "keyup" => ActionKind::KeyUp {
            key: str_of(p, "key"),
        },
        "break" => ActionKind::Break,
        "continue" => ActionKind::Continue,
        "switch" => ActionKind::Switch {
            expr: str_of(p, "expr"),
        },
        "case" => ActionKind::Case {
            value: str_of(p, "value"),
        },
        "defaultcase" | "default" => ActionKind::DefaultCase,
        "endswitch" => ActionKind::EndSwitch,
        "randommouse" => ActionKind::RandomMouse {
            x1: i32_of(p, "x1", 0),
            y1: i32_of(p, "y1", 0),
            x2: i32_of(p, "x2", 1920),
            y2: i32_of(p, "y2", 1080),
            save_x: {
                let n = str_of(p, "save_x");
                if n.is_empty() {
                    default_rnd_x()
                } else {
                    n
                }
            },
            save_y: {
                let n = str_of(p, "save_y");
                if n.is_empty() {
                    default_rnd_y()
                } else {
                    n
                }
            },
        },
        "readjson" | "json" => ActionKind::ReadJson {
            file: str_of(p, "file"),
            query: str_of(p, "query"),
            name: {
                let n = str_of(p, "name");
                if n.is_empty() {
                    "json".into()
                } else {
                    n
                }
            },
        },
        "waitclipboard" | "waitclip" => {
            let mut k = ActionKind::WaitClipboard {
                exclude: str_of(p, "exclude"),
                save_name: {
                    let n = str_of(p, "save_name");
                    if n.is_empty() {
                        default_clip_name()
                    } else {
                        n
                    }
                },
                timeout_ms: u64_of(p, "timeout", u64_of(p, "timeout_ms", 30_000)),
                on_fail: "stop".into(),
            };
            if let ActionKind::WaitClipboard { on_fail, .. } = &mut k {
                let f = str_of(p, "on_fail");
                if !f.is_empty() {
                    *on_fail = f;
                }
            }
            k
        }
        "playrandom" => ActionKind::PlayRandom {
            folder: str_of(p, "folder"),
        },
        "setclip" | "setclipboard" => ActionKind::SetClipboard {
            text: str_of(p, "text"),
        },
        "getclip" | "getclipboard" => ActionKind::GetClipboard {
            name: {
                let n = str_of(p, "name");
                if n.is_empty() {
                    "clip".into()
                } else {
                    n
                }
            },
        },
        "waitwindow" => {
            let mut k = ActionKind::WaitWindow {
                title: str_of(p, "title"),
                timeout_ms: u64_of(p, "timeout", u64_of(p, "timeout_ms", 5000)),
                on_fail: "stop".into(),
            };
            if let ActionKind::WaitWindow { on_fail, .. } = &mut k {
                let f = str_of(p, "on_fail");
                if !f.is_empty() {
                    *on_fail = f;
                }
            }
            k
        }
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
    // Unparsable entries become visible comments instead of vanishing, but a
    // file with no real action at all stays an error.
    let mut actions: Vec<Action> = Vec::new();
    let mut parsed = 0usize;
    for (idx, v) in list.iter().enumerate() {
        match action_from_value(v) {
            Some(a) => {
                parsed += 1;
                actions.push(a);
            }
            None => actions.push(Action::new(ActionKind::Comment {
                text: format!("unparsable action #{}", idx + 1),
            })),
        }
    }
    if parsed == 0 {
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

    #[test]
    fn scheduled_task_due_matches_clock_once_per_day() {
        assert!(scheduled_task_due("09:00", 9, 0, false));
        assert!(scheduled_task_due("9:00", 9, 0, false));
        assert!(!scheduled_task_due("09:00", 9, 0, true));
        assert!(!scheduled_task_due("09:00", 9, 1, false));
        assert!(!scheduled_task_due("not-a-time", 9, 0, false));
        assert!(!scheduled_task_due("24:00", 0, 0, false));
    }

    #[test]
    fn session_roundtrip_keeps_hotkeys_tasks_and_repeat() {
        let path = std::env::temp_dir().join(format!("amk_session_{}.json", std::process::id()));
        let options = AppOptions {
            hk_record: "Ctrl+F9".into(),
            ..AppOptions::default()
        };
        let session = AppSession {
            lang: "en".into(),
            options,
            repeat: "times".into(),
            repeat_n: 4,
            tasks: vec![ScheduledTask {
                name: "Morning".into(),
                script: "a.amk".into(),
                when: "09:30".into(),
            }],
            ..AppSession::default()
        };
        assert!(session.save(&path));
        let back = AppSession::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(back, session);
        assert_eq!(back.repeat_mode(), RepeatMode::Times);
        assert_eq!(back.options.hk_record, "Ctrl+F9");
    }

    #[test]
    fn broken_session_file_falls_back_to_defaults() {
        let path =
            std::env::temp_dir().join(format!("amk_session_bad_{}.json", std::process::id()));
        std::fs::write(&path, "{not json").unwrap();
        let session = AppSession::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(session, AppSession::default());
        assert_eq!(session.options.hk_record, "F9");
        assert_eq!(session.repeat_mode(), RepeatMode::Once);
    }

    #[test]
    fn partial_session_fills_the_rest_from_defaults() {
        let path =
            std::env::temp_dir().join(format!("amk_session_part_{}.json", std::process::id()));
        std::fs::write(&path, r#"{"lang":"en"}"#).unwrap();
        let session = AppSession::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(session.lang, "en");
        assert_eq!(session.repeat, "once");
        assert_eq!(session.repeat_n, 10);
        assert!(session.speed_enabled);
        assert_eq!(session.options.hk_play, "F10");
    }

    #[test]
    fn partial_options_object_keeps_the_session_instead_of_wiping_it() {
        let path =
            std::env::temp_dir().join(format!("amk_session_popt_{}.json", std::process::id()));
        // `options` present but incomplete: the rest must fill from defaults
        // and the tasks/lang around it must survive.
        std::fs::write(
            &path,
            r#"{"lang":"en","options":{"play_speed":2.5},"tasks":[{"name":"T","script":"a.amk"}]}"#,
        )
        .unwrap();
        let session = AppSession::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(session.lang, "en");
        assert!((session.options.play_speed - 2.5).abs() < f32::EPSILON);
        assert_eq!(session.options.hk_play, "F10");
        assert_eq!(session.tasks.len(), 1);
        assert_eq!(session.tasks[0].script, "a.amk");
    }

    #[test]
    fn optimize_record_keeps_merged_gaps_on_the_timeline() {
        let mut sc = Script::default();
        let mk = |kind: ActionKind, delay: u64| Action::new(kind).with_delay(delay);
        sc.actions = vec![
            mk(
                ActionKind::MouseMove {
                    x: 10,
                    y: 10,
                    ms: 0,
                },
                30,
            ),
            mk(
                ActionKind::MouseMove {
                    x: 50,
                    y: 50,
                    ms: 0,
                },
                30,
            ),
            mk(
                ActionKind::MouseMove {
                    x: 51,
                    y: 51,
                    ms: 0,
                },
                30,
            ),
            mk(ActionKind::Delay { ms: 200 }, 30),
            mk(ActionKind::Delay { ms: 300 }, 30),
        ];
        sc.optimize_record();
        // Two moves merge into one carrying both gaps; the tiny third move's
        // 30 ms rides on it; the two delays merge carrying both 30 ms gaps.
        assert_eq!(sc.actions.len(), 2, "{:?}", sc.actions);
        assert_eq!(sc.actions[0].delay_ms, 60 + 30);
        assert!(matches!(
            sc.actions[0].kind,
            ActionKind::MouseMove {
                x: 50,
                y: 50,
                ms: _
            }
        ));
        assert_eq!(sc.actions[1].delay_ms, 30 + 30);
        assert!(matches!(sc.actions[1].kind, ActionKind::Delay { ms: 500 }));
    }

    #[test]
    fn python_params_survive_extreme_and_typed_values() {
        let big = serde_json::json!({"x": 5_000_000_000i64, "y": 0, "clicks": 300});
        assert_eq!(i32_of(&big, "x", 0), i32::MAX);
        let clicks = match kind_from_python("click", &big) {
            ActionKind::MouseClick { clicks, .. } => clicks,
            other => panic!("{other:?}"),
        };
        assert_eq!(clicks, 255, "clicks must clamp, not truncate 300 to 44");

        let typed = serde_json::json!({"text": true, "value": 1.5});
        assert_eq!(str_of(&typed, "text"), "true");
        assert_eq!(str_of(&typed, "value"), "1.5");
    }

    #[test]
    fn flexible_loader_marks_unparsable_actions_and_still_rejects_garbage() {
        let s = r#"{
          "version":"1.0","name":"mixed",
          "actions":[
            {"kind":"delay","name":"","enabled":true,"delay_ms":0,"params":{"ms":100}},
            {"name":"no kind here","enabled":true,"delay_ms":0},
            {"kind":123,"enabled":true,"delay_ms":0}
          ]
        }"#;
        let sc = Script::load_json(s).expect("one valid action keeps the script");
        assert_eq!(sc.actions.len(), 3);
        assert!(matches!(sc.actions[0].kind, ActionKind::Delay { ms: 100 }));
        assert!(matches!(sc.actions[1].kind, ActionKind::Comment { .. }));
        assert!(matches!(sc.actions[2].kind, ActionKind::Comment { .. }));

        let garbage = r#"{"actions":[1,2,3]}"#;
        assert!(Script::load_json(garbage).is_err());
    }

    #[test]
    fn new_parity_kinds_roundtrip_through_json() {
        let sc = Script {
            version: "1.0".into(),
            name: "parity".into(),
            actions: vec![
                Action::new(ActionKind::WriteJson {
                    file: "out.json".into(),
                    query: "a.b.0".into(),
                    value: "42".into(),
                }),
                Action::new(ActionKind::ReadRegistry {
                    path: r"HKCU\Software\AMK".into(),
                    value: "LastRun".into(),
                    name: "last".into(),
                }),
                Action::new(ActionKind::WriteRegistry {
                    path: r"HKCU\Software\AMK".into(),
                    value: "LastRun".into(),
                    data: "today".into(),
                }),
                Action::new(ActionKind::MinimizeWindow {
                    title: "Notepad".into(),
                }),
                Action::new(ActionKind::MaximizeWindow {
                    title: "Notepad".into(),
                }),
                Action::new(ActionKind::RestoreWindow {
                    title: "Notepad".into(),
                }),
                Action::new(ActionKind::SetClipboardHtml {
                    html: "<b>hi</b>".into(),
                }),
                Action::new(ActionKind::GetClipboardHtml {
                    name: "html".into(),
                }),
            ],
        };
        let json = sc.to_json().expect("serialize");
        let back = Script::load_json(&json).expect("deserialize");
        assert_eq!(back.actions.len(), 8);
        assert!(matches!(
            &back.actions[0].kind,
            ActionKind::WriteJson { .. }
        ));
        assert!(matches!(
            &back.actions[1].kind,
            ActionKind::ReadRegistry { .. }
        ));
        assert!(matches!(
            &back.actions[2].kind,
            ActionKind::WriteRegistry { .. }
        ));
        assert!(matches!(
            &back.actions[3].kind,
            ActionKind::MinimizeWindow { .. }
        ));
        assert!(matches!(
            &back.actions[4].kind,
            ActionKind::MaximizeWindow { .. }
        ));
        assert!(matches!(
            &back.actions[5].kind,
            ActionKind::RestoreWindow { .. }
        ));
        assert!(matches!(
            &back.actions[6].kind,
            ActionKind::SetClipboardHtml { .. }
        ));
        assert!(matches!(
            &back.actions[7].kind,
            ActionKind::GetClipboardHtml { .. }
        ));
        assert_eq!(back.actions[0].kind.type_key(), "act_write_json");
        assert_eq!(back.actions[3].kind.type_key(), "act_win_min");
        assert_eq!(back.actions[6].kind.type_key(), "act_set_clip_html");
    }

    #[test]
    fn rename_variable_renames_names_braces_and_expr_words() {
        let mut sc = Script {
            version: "1.0".into(),
            name: "ren".into(),
            actions: vec![
                Action::new(ActionKind::SetVar {
                    name: "n".into(),
                    value: "{n} + 1".into(),
                }),
                Action::new(ActionKind::If {
                    expr: "n > 2 and xn == n".into(),
                }),
                Action::new(ActionKind::For {
                    var: "n".into(),
                    from: 1,
                    to: 5,
                    step: 1,
                }),
                Action::new(ActionKind::GetClipboard { name: "n".into() }),
                Action::new(ActionKind::While {
                    expr: "{n} == 1".into(),
                }),
            ],
        };
        let changed = sc.rename_variable("n", "count");
        // SetVar name, SetVar value braces, If expr, For var,
        // GetClipboard name, While expr braces — one per field.
        assert_eq!(changed, 6);
        assert!(matches!(
            &sc.actions[0].kind,
            ActionKind::SetVar { name, value }
                if name == "count" && value == "{count} + 1"
        ));
        assert!(matches!(
            &sc.actions[1].kind,
            ActionKind::If { expr } if expr == "count > 2 and xn == count"
        ));
        assert!(matches!(
            &sc.actions[2].kind,
            ActionKind::For { var, .. } if var == "count"
        ));
        assert!(matches!(
            &sc.actions[3].kind,
            ActionKind::GetClipboard { name } if name == "count"
        ));
    }

    #[test]
    fn rename_variable_ignores_empty_and_same_names() {
        let mut sc = Script::default();
        assert_eq!(sc.rename_variable("", "x"), 0);
        assert_eq!(sc.rename_variable("n", "n"), 0);
    }
}
