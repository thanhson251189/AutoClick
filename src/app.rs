use crate::engine::{Engine, RunState};
use crate::i18n::{t, Lang};
use crate::model::*;
use crate::record::Recorder;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialog {
    None,
    Mouse,
    Keyboard,
    Delay,
    Smart,
    Search,
    Window,
    File,
    Variable,
    If,
    For,
    While,
    Label,
    Goto,
    Message,
    Comment,
    CallFn,
    PlayScript,
    Options,
    Hotkeys,
    About,
    Help,
    Log,
    Schedule,
    Clicker,
    Presser,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolboxId {
    Mouse,
    Keyboard,
    Image,
    Window,
    File,
    Flow,
    Var,
    Fn,
    More,
    Schedule,
}

pub struct AmkApp {
    lang: Lang,
    script: Script,
    path: Option<PathBuf>,
    dirty: bool,
    selected: Option<usize>,
    options: AppOptions,
    engine: Engine,
    recorder: Recorder,
    last_drain: Instant,
    dialog: Dialog,
    edit_index: Option<usize>,
    draft: ActionKind,
    draft_name: String,
    draft_delay: u64,
    status: String,
    clipboard: Vec<Action>,
    undo: Vec<Script>,
    redo: Vec<Script>,
    repeat: RepeatMode,
    repeat_n: u32,
    duration_n: u32,
    duration_unit: DurationUnit,
    shutdown: bool,
    no_activate: bool,
    show_recent: bool,
    show_toolbox: bool,
    show_play_opts: bool,
    show_status: bool,
    hover_tb: Option<u8>,
    tasks: Vec<ScheduledTask>,
    clicker_x: i32,
    clicker_y: i32,
    clicker_interval: u64,
    clicker_count: u32,
    presser_key: String,
    presser_interval: u64,
    presser_count: u32,
    help_query: String,
    path_input: String,
    show_open: bool,
    show_save: bool,
    speed_enabled: bool,
    icons: std::collections::HashMap<String, eframe::egui::TextureHandle>,
}

impl AmkApp {
    pub fn new() -> Self {
        let lang = Lang::Vi;
        Self {
            icons: std::collections::HashMap::new(),
            status: t(lang, "status_ready").to_string(),
            lang,
            script: Script::default(),
            path: None,
            dirty: false,
            selected: Some(1),
            options: AppOptions::default(),
            engine: Engine::new(),
            recorder: Recorder::new(),
            last_drain: Instant::now(),
            dialog: Dialog::None,
            edit_index: None,
            draft: ActionKind::Delay { ms: 500 },
            draft_name: String::new(),
            draft_delay: 0,
            clipboard: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            repeat: RepeatMode::Once,
            repeat_n: 10,
            duration_n: 1,
            duration_unit: DurationUnit::Minutes,
            shutdown: false,
            no_activate: false,
            show_recent: true,
            show_toolbox: true,
            show_play_opts: true,
            show_status: true,
            hover_tb: None,
            tasks: Vec::new(),
            clicker_x: 0,
            clicker_y: 0,
            clicker_interval: 100,
            clicker_count: 10,
            presser_key: "Space".into(),
            presser_interval: 200,
            presser_count: 10,
            help_query: String::new(),
            path_input: "script.amk".into(),
            show_open: false,
            show_save: false,
            speed_enabled: true,
        }
    }

    fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or(t(self.lang, "untitled"));
        let star = if self.dirty { "*" } else { "" };
        format!("{}{} — {}", name, star, t(self.lang, "app_title"))
    }

    fn snapshot(&mut self) {
        self.undo.push(self.script.clone());
        if self.undo.len() > 80 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(self.script.clone());
            self.script = prev;
            self.dirty = true;
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.script.clone());
            self.script = next;
            self.dirty = true;
        }
    }

    fn insert_kind(&mut self, kind: ActionKind) {
        self.snapshot();
        let mut a = Action::new(kind);
        a.name = a.kind.default_name();
        let idx = self
            .selected
            .map(|i| (i + 1).min(self.script.actions.len()))
            .unwrap_or(self.script.actions.len().saturating_sub(1));
        let idx = if idx == 0 { 1 } else { idx };
        let cap = self.script.actions.len().saturating_sub(0);
        let idx = idx.min(cap);
        self.script.actions.insert(idx, a);
        self.selected = Some(idx);
        self.dirty = true;
    }

    fn open_edit(&mut self, kind: ActionKind, existing: Option<usize>) {
        self.edit_index = existing;
        if let Some(i) = existing {
            if let Some(a) = self.script.actions.get(i) {
                self.draft = a.kind.clone();
                self.draft_name = a.name.clone();
                self.draft_delay = a.delay_ms;
            }
        } else {
            self.draft = kind.clone();
            self.draft_name = kind.default_name();
            self.draft_delay = 0;
        }
        self.dialog = match &self.draft {
            ActionKind::MouseMove { .. }
            | ActionKind::MouseClick { .. }
            | ActionKind::MouseDrag { .. }
            | ActionKind::MouseWheel { .. } => Dialog::Mouse,
            ActionKind::TypeText { .. } | ActionKind::KeyPress { .. } => Dialog::Keyboard,
            ActionKind::Delay { .. } => Dialog::Delay,
            ActionKind::SmartClick { .. } => Dialog::Smart,
            ActionKind::SearchPicture { .. } => Dialog::Search,
            ActionKind::ActivateWindow { .. } | ActionKind::CloseWindow { .. } => Dialog::Window,
            ActionKind::OpenFile { .. }
            | ActionKind::OpenUrl { .. }
            | ActionKind::OpenFolder { .. } => Dialog::File,
            ActionKind::SetVar { .. } => Dialog::Variable,
            ActionKind::If { .. } => Dialog::If,
            ActionKind::For { .. } => Dialog::For,
            ActionKind::While { .. } => Dialog::While,
            ActionKind::Label { .. } => Dialog::Label,
            ActionKind::Goto { .. } => Dialog::Goto,
            ActionKind::MessageBox { .. } => Dialog::Message,
            ActionKind::Comment { .. } => Dialog::Comment,
            ActionKind::CallFunction { .. } => Dialog::CallFn,
            ActionKind::PlayScript { .. } => Dialog::PlayScript,
            _ => Dialog::None,
        };
    }

    fn commit_draft(&mut self) {
        let mut a = Action::new(self.draft.clone());
        if !self.draft_name.trim().is_empty() {
            a.name = self.draft_name.clone();
        }
        a.delay_ms = self.draft_delay;
        if let Some(i) = self.edit_index {
            self.snapshot();
            if let Some(slot) = self.script.actions.get_mut(i) {
                *slot = a;
            }
        } else {
            self.insert_kind(self.draft.clone());
            if let Some(i) = self.selected {
                if let Some(slot) = self.script.actions.get_mut(i) {
                    slot.name = if self.draft_name.trim().is_empty() {
                        slot.kind.default_name()
                    } else {
                        self.draft_name.clone()
                    };
                    slot.delay_ms = self.draft_delay;
                    slot.kind = self.draft.clone();
                }
            }
        }
        self.dialog = Dialog::None;
        self.edit_index = None;
    }

    fn delete_selected(&mut self) {
        if let Some(i) = self.selected {
            if i < self.script.actions.len() {
                match self.script.actions[i].kind {
                    ActionKind::FunctionEntry | ActionKind::EndFunction => return,
                    _ => {}
                }
                self.snapshot();
                self.script.actions.remove(i);
                if self.script.actions.is_empty() {
                    self.selected = None;
                } else {
                    self.selected = Some(i.min(self.script.actions.len() - 1));
                }
            }
        }
    }

    fn move_sel(&mut self, dir: i32) {
        if let Some(i) = self.selected {
            let j = i as i32 + dir;
            if j >= 0 && (j as usize) < self.script.actions.len() {
                self.snapshot();
                self.script.actions.swap(i, j as usize);
                self.selected = Some(j as usize);
            }
        }
    }

    fn new_script(&mut self) {
        self.snapshot();
        self.script = Script::default();
        self.path = None;
        self.selected = Some(1);
        self.dirty = false;
    }

    fn save_to(&mut self, path: PathBuf) {
        match self.script.to_json() {
            Ok(s) => {
                if std::fs::write(&path, s).is_ok() {
                    self.path = Some(path);
                    self.dirty = false;
                    self.status = "Saved.".into();
                } else {
                    self.status = "Save failed.".into();
                }
            }
            Err(e) => self.status = e,
        }
    }

    fn save(&mut self) {
        if let Some(p) = self.path.clone() {
            self.save_to(p);
        } else {
            self.save_as();
        }
    }

    fn save_as(&mut self) {
        self.path_input = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "script.amk".into());
        self.show_save = true;
    }

    fn open_from_path(&mut self, f: PathBuf) {
        match std::fs::read_to_string(&f) {
            Ok(s) => match Script::load_json(&s) {
                Ok(sc) => {
                    self.snapshot();
                    self.script = sc;
                    self.path = Some(f);
                    self.dirty = false;
                    self.selected = if self.script.actions.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                }
                Err(e) => self.status = e,
            },
            Err(e) => self.status = e.to_string(),
        }
    }

    fn open_file(&mut self) {
        self.path_input = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "script.amk".into());
        self.show_open = true;
    }

    fn start_record(&mut self) {
        if self.engine.snapshot_state() != RunState::Idle {
            return;
        }
        self.snapshot();
        self.recorder
            .start(self.options.sample_ms, self.options.ignore_px);
        self.status = t(self.lang, "status_recording").to_string();
    }

    fn stop_record(&mut self) {
        self.recorder.stop();
        let mut rec = self.recorder.drain();
        if rec.is_empty() {
            self.status = t(self.lang, "status_ready").to_string();
            return;
        }
        let idx = self
            .selected
            .map(|i| (i + 1).min(self.script.actions.len()))
            .unwrap_or(self.script.actions.len().saturating_sub(1))
            .max(1);
        for (n, a) in rec.drain(..).enumerate() {
            self.script.actions.insert(idx + n, a);
        }
        self.selected = Some(idx);
        self.status = t(self.lang, "status_ready").to_string();
    }

    fn play(&mut self) {
        if self.recorder.is_running() {
            return;
        }
        let times = match self.repeat {
            RepeatMode::Once => 1,
            RepeatMode::Times => self.repeat_n.max(1),
            RepeatMode::Duration => 0,
            RepeatMode::Infinite => 0, // 0 = run forever until stopped
        };
        let dur = match self.repeat {
            RepeatMode::Duration => {
                let mul = match self.duration_unit {
                    DurationUnit::Seconds => 1,
                    DurationUnit::Minutes => 60,
                    DurationUnit::Hours => 3600,
                };
                Some(self.duration_n as u64 * mul)
            }
            _ => None,
        };
        self.status = t(self.lang, "status_playing").to_string();
        let dir = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());
        self.engine.play(
            self.script.clone(),
            self.options.play_speed,
            times,
            dur,
            dir,
        );
    }

    fn load_sample(&mut self, which: u8) {
        self.snapshot();
        let mut s = Script::default();
        s.actions.clear();
        s.actions.push(Action::new(ActionKind::FunctionEntry));
        match which {
            0 => {
                s.name = "Sample_Click".into();
                s.actions.push(Action::new(ActionKind::Comment {
                    text: "Move and click the center-ish of a 1920x1080 screen".into(),
                }));
                s.actions
                    .push(Action::new(ActionKind::MouseMove { x: 960, y: 540 }));
                s.actions.push(Action::new(ActionKind::Delay { ms: 300 }));
                s.actions.push(Action::new(ActionKind::MouseClick {
                    button: MouseBtn::Left,
                    x: 960,
                    y: 540,
                    clicks: 1,
                }));
            }
            1 => {
                s.name = "Sample_Type".into();
                s.actions.push(Action::new(ActionKind::TypeText {
                    text: "Hello from Automatic Mouse and Keyboard".into(),
                    interval_ms: 20,
                }));
                s.actions.push(Action::new(ActionKind::KeyPress {
                    key: "Enter".into(),
                }));
            }
            2 => {
                s.name = "Sample_Loop".into();
                s.actions.push(Action::new(ActionKind::For {
                    var: "i".into(),
                    from: 1,
                    to: 5,
                    step: 1,
                }));
                s.actions.push(Action::new(ActionKind::Delay { ms: 400 }));
                s.actions.push(Action::new(ActionKind::Comment {
                    text: "loop body".into(),
                }));
                s.actions.push(Action::new(ActionKind::EndFor));
            }
            _ => {
                s.name = "Sample_SmartClick".into();
                s.actions.push(Action::new(ActionKind::smart_click(
                    100,
                    100,
                    String::new(),
                    5000,
                )));
            }
        }
        s.actions.push(Action::new(ActionKind::EndFunction));
        self.script = s;
        self.selected = Some(1);
        self.dirty = true;
    }

    fn compile_launcher(&mut self) {
        let path = match self.path.clone() {
            Some(p) => p,
            None => {
                self.save_as();
                return;
            }
        };
        self.save_to(path.clone());
        let bat = path.with_extension("cmd");
        let body = format!(
            "@echo off\r\nrem AMK compiled launcher\r\nautomatic-mouse-keyboard --run \"{}\"\r\n",
            path.display()
        );
        let _ = std::fs::write(&bat, body);
        self.status = format!("Launcher written: {}", bat.display());
    }
}

impl eframe::App for AmkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));

        if self.recorder.is_running() && self.last_drain.elapsed() > Duration::from_millis(200) {
            let extra = self.recorder.drain();
            if !extra.is_empty() {
                let idx = self
                    .selected
                    .map(|i| (i + 1).min(self.script.actions.len()))
                    .unwrap_or(self.script.actions.len())
                    .max(1);
                let n = extra.len();
                for (k, a) in extra.into_iter().enumerate() {
                    let at = (idx + k).min(self.script.actions.len());
                    self.script.actions.insert(at, a);
                }
                self.selected = Some(idx + n - 1);
                self.dirty = true;
            }
            self.last_drain = Instant::now();
        }

        match self.engine.snapshot_state() {
            RunState::Running => {
                self.status = t(self.lang, "status_playing").to_string();
                ctx.request_repaint();
            }
            RunState::Paused => {
                self.status = t(self.lang, "status_paused").to_string();
                ctx.request_repaint();
            }
            RunState::Idle => {
                if !self.recorder.is_running()
                    && (self.status.contains("Playing")
                        || self.status.contains("chạy")
                        || self.status.contains("Paused")
                        || self.status.contains("tạm"))
                {
                    self.status = t(self.lang, "status_ready").to_string();
                }
            }
        }

        self.hotkeys(ctx);
        self.menu_bar(ctx);
        self.toolbar(ctx);

        if self.show_status {
            egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (color, text) = if self.status.contains("Ready") || self.status.contains("Sẵn") {
                        (Color32::from_rgb(0, 120, 215), &self.status) // Blue dot for ready
                    } else if self.status.contains("Play") || self.status.contains("Chạy") {
                        (Color32::from_rgb(40, 180, 80), &self.status) // Green dot for play
                    } else if self.status.contains("Pause") || self.status.contains("Tạm") {
                        (Color32::from_rgb(200, 150, 0), &self.status) // Yellow dot for pause
                    } else if self.recorder.is_running() {
                        (Color32::from_rgb(220, 50, 50), &self.status) // Red dot for record
                    } else {
                        (Color32::GRAY, &self.status)
                    };

                    ui.label(RichText::new("●").color(color));
                    ui.label(RichText::new(format!("{} | Active Window: Desktop (1920x1080)", text)).size(12.0));
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        let total = self.script.actions.len();
                        let current = self.selected.unwrap_or(0);
                        ui.label(RichText::new(format!("Line: {} of {}", current, total)).size(11.0).color(Color32::from_rgb(60,60,60)));
                        ui.separator();
                        
                        ui.label(RichText::new("Ins: ON").size(11.0).color(Color32::from_rgb(60,60,60)));
                        ui.separator();
                        ui.label(RichText::new("Num: ON").size(11.0).color(Color32::from_rgb(60,60,60)));
                        ui.separator();
                        ui.label(RichText::new("Caps: OFF").size(11.0).color(Color32::from_rgb(60,60,60)));
                        ui.separator();
                        
                        ui.label(RichText::new(format!("Cursor: X: {}, Y: {} (Color: #...)", self.clicker_x, self.clicker_y)).size(11.0).color(Color32::from_rgb(60,60,60)));
                        ui.add_space(8.0);
                    });
                });
            });
        }

        if self.show_play_opts {
            egui::SidePanel::right("play_opts_panel")
                .resizable(true)
                .min_width(230.0)
                .default_width(250.0)
                .show(ctx, |ui| {
                    self.play_options(ui);
                });
        }

        if self.show_toolbox {
            egui::SidePanel::left("toolbox_panel")
                .resizable(false)
                .exact_width(180.0)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(250, 252, 255))
                        .inner_margin(0.0),
                )
                .show_separator_line(true)
                .show(ctx, |ui| {
                    self.toolbox(ui);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.action_table(ui);
        });

        self.draw_dialogs(ctx);
        self.file_path_windows(ctx);
    }
}

impl AmkApp {
    fn hotkeys(&mut self, ctx: &egui::Context) {
        let input = ctx.input(|i| {
            (
                i.modifiers.ctrl,
                i.modifiers.shift,
                i.key_pressed(egui::Key::Z),
                i.key_pressed(egui::Key::Y),
                i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::O),
                i.key_pressed(egui::Key::N),
                i.key_pressed(egui::Key::P),
                i.key_pressed(egui::Key::F2),
                i.key_pressed(egui::Key::F9),
                i.key_pressed(egui::Key::Delete),
                i.key_pressed(egui::Key::C),
                i.key_pressed(egui::Key::X),
                i.key_pressed(egui::Key::V),
            )
        });
        let (ctrl, shift, z, y, s, o, n, p, f2, f9, del, c, x, v) = input;
        if ctrl && z {
            self.undo();
        }
        if ctrl && y {
            self.redo();
        }
        if ctrl && s {
            self.save();
        }
        if ctrl && o {
            self.open_file();
        }
        if ctrl && n {
            self.new_script();
        }
        if ctrl && p && self.engine.snapshot_state() != RunState::Idle {
            self.engine.toggle_pause();
        }
        if shift && f2 {
            if self.recorder.is_running() {
                self.stop_record();
            } else {
                self.start_record();
            }
        }
        if f9 {
            match self.engine.snapshot_state() {
                RunState::Idle => self.play(),
                _ => self.engine.request_stop(),
            }
        }
        if del && self.dialog == Dialog::None {
            self.delete_selected();
        }
        if ctrl && c {
            if let Some(i) = self.selected {
                if let Some(a) = self.script.actions.get(i) {
                    self.clipboard = vec![a.clone()];
                }
            }
        }
        if ctrl && x {
            if let Some(i) = self.selected {
                if let Some(a) = self.script.actions.get(i) {
                    self.clipboard = vec![a.clone()];
                }
                self.delete_selected();
            }
        }
        if ctrl && v {
            if let Some(a) = self.clipboard.first().cloned() {
                let mut b = a;
                b.id = uuid::Uuid::new_v4().to_string();
                self.snapshot();
                let idx = self
                    .selected
                    .map(|i| i + 1)
                    .unwrap_or(self.script.actions.len());
                self.script
                    .actions
                    .insert(idx.min(self.script.actions.len()), b);
                self.selected = Some(idx.min(self.script.actions.len() - 1));
            }
        }
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(t(self.lang, "menu_file"), |ui| {
                    if ui.button(t(self.lang, "new")).clicked() {
                        self.new_script();
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "open")).clicked() {
                        self.open_file();
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "save")).clicked() {
                        self.save();
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "save_as")).clicked() {
                        self.save_as();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t(self.lang, "import")).clicked() {
                        self.open_file();
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "export")).clicked() {
                        self.save_as();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t(self.lang, "compile_exe")).clicked() {
                        self.compile_launcher();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t(self.lang, "exit")).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button(t(self.lang, "menu_edit"), |ui| {
                    if ui.button(t(self.lang, "undo")).clicked() {
                        self.undo();
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "redo")).clicked() {
                        self.redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t(self.lang, "cut")).clicked() {
                        if let Some(i) = self.selected {
                            if let Some(a) = self.script.actions.get(i) {
                                self.clipboard = vec![a.clone()];
                            }
                            self.delete_selected();
                        }
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "copy")).clicked() {
                        if let Some(i) = self.selected {
                            if let Some(a) = self.script.actions.get(i) {
                                self.clipboard = vec![a.clone()];
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "paste")).clicked() {
                        if let Some(a) = self.clipboard.first().cloned() {
                            let mut b = a;
                            b.id = uuid::Uuid::new_v4().to_string();
                            self.insert_kind(b.kind);
                        }
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "delete")).clicked() {
                        self.delete_selected();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t(self.lang, "enable")).clicked() {
                        if let Some(i) = self.selected {
                            if let Some(a) = self.script.actions.get_mut(i) {
                                a.enabled = true;
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "disable")).clicked() {
                        if let Some(i) = self.selected {
                            if let Some(a) = self.script.actions.get_mut(i) {
                                a.enabled = false;
                            }
                        }
                        ui.close_menu();
                    }
                });
                ui.menu_button(t(self.lang, "menu_insert"), |ui| {
                    if ui.button(t(self.lang, "insert_mouse")).clicked() {
                        self.open_edit(
                            ActionKind::MouseClick {
                                button: MouseBtn::Left,
                                x: 0,
                                y: 0,
                                clicks: 1,
                            },
                            None,
                        );
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_key")).clicked() {
                        self.open_edit(
                            ActionKind::TypeText {
                                text: String::new(),
                                interval_ms: 20,
                            },
                            None,
                        );
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_delay")).clicked() {
                        self.open_edit(ActionKind::Delay { ms: 500 }, None);
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_search")).clicked() {
                        self.open_edit(ActionKind::search_picture(String::new(), 5000), None);
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "smart_click")).clicked() {
                        self.open_edit(ActionKind::smart_click(0, 0, String::new(), 5000), None);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t(self.lang, "insert_window")).clicked() {
                        self.open_edit(
                            ActionKind::ActivateWindow {
                                title: String::new(),
                            },
                            None,
                        );
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_file")).clicked() {
                        self.open_edit(
                            ActionKind::OpenFile {
                                path: String::new(),
                            },
                            None,
                        );
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t(self.lang, "insert_if")).clicked() {
                        self.snapshot();
                        self.insert_kind(ActionKind::If {
                            expr: "true".into(),
                        });
                        self.insert_kind(ActionKind::EndIf);
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_for")).clicked() {
                        self.snapshot();
                        self.insert_kind(ActionKind::For {
                            var: "i".into(),
                            from: 1,
                            to: 10,
                            step: 1,
                        });
                        self.insert_kind(ActionKind::EndFor);
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_while")).clicked() {
                        self.snapshot();
                        self.insert_kind(ActionKind::While {
                            expr: "true".into(),
                        });
                        self.insert_kind(ActionKind::EndWhile);
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_label")).clicked() {
                        self.open_edit(ActionKind::Label { name: "L1".into() }, None);
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_var")).clicked() {
                        self.open_edit(
                            ActionKind::SetVar {
                                name: "v".into(),
                                value: "0".into(),
                            },
                            None,
                        );
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_msg")).clicked() {
                        self.open_edit(
                            ActionKind::MessageBox {
                                text: String::new(),
                            },
                            None,
                        );
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_comment")).clicked() {
                        self.open_edit(
                            ActionKind::Comment {
                                text: String::new(),
                            },
                            None,
                        );
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "insert_fn")).clicked() {
                        self.open_edit(
                            ActionKind::CallFunction {
                                name: "Delay".into(),
                            },
                            None,
                        );
                        ui.close_menu();
                    }
                });
                ui.menu_button(t(self.lang, "menu_tools"), |ui| {
                    if ui.button(t(self.lang, "options")).clicked() {
                        self.dialog = Dialog::Options;
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "hotkeys")).clicked() {
                        self.dialog = Dialog::Hotkeys;
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "debug_run")).clicked() {
                        self.options.play_speed = 0.4;
                        self.play();
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "optimize")).clicked() {
                        self.snapshot();
                        self.script.optimize_record();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t(self.lang, "mouse_clicker")).clicked() {
                        self.dialog = Dialog::Clicker;
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "key_presser")).clicked() {
                        self.dialog = Dialog::Presser;
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "scheduler")).clicked() {
                        self.dialog = Dialog::Schedule;
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "exec_log")).clicked() {
                        self.dialog = Dialog::Log;
                        ui.close_menu();
                    }
                });
                ui.menu_button(t(self.lang, "menu_view"), |ui| {
                    ui.checkbox(&mut self.show_toolbox, t(self.lang, "toolbox"));
                    ui.checkbox(&mut self.show_play_opts, t(self.lang, "play_options"));
                    ui.checkbox(&mut self.show_status, t(self.lang, "status_bar"));
                    ui.separator();
                    ui.label(t(self.lang, "language"));
                    if ui
                        .selectable_label(self.lang == Lang::En, "English")
                        .clicked()
                    {
                        self.lang = Lang::En;
                        ui.close_menu();
                    }
                    if ui
                        .selectable_label(self.lang == Lang::Vi, "Tiếng Việt")
                        .clicked()
                    {
                        self.lang = Lang::Vi;
                        ui.close_menu();
                    }
                });
                ui.menu_button(t(self.lang, "menu_help"), |ui| {
                    if ui.button(t(self.lang, "help_contents")).clicked() {
                        self.dialog = Dialog::Help;
                        ui.close_menu();
                    }
                    if ui.button(t(self.lang, "search_help")).clicked() {
                        self.dialog = Dialog::Help;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Sample: Click").clicked() {
                        self.load_sample(0);
                        ui.close_menu();
                    }
                    if ui.button("Sample: Type").clicked() {
                        self.load_sample(1);
                        ui.close_menu();
                    }
                    if ui.button("Sample: For loop").clicked() {
                        self.load_sample(2);
                        ui.close_menu();
                    }
                    if ui.button("Sample: Smart Click").clicked() {
                        self.load_sample(3);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t(self.lang, "about")).clicked() {
                        self.dialog = Dialog::About;
                        ui.close_menu();
                    }
                });
            });
        });
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(250, 250, 250))
                    .inner_margin(egui::Margin::symmetric(8.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let default_color = Color32::from_rgb(40, 50, 60);

                    let tex_folder = self.get_icon(ctx, "folder");
                    if big_tool(
                        ui,
                        &tex_folder,
                        t(self.lang, "open"),
                        default_color,
                        None,
                        None,
                    )
                    .clicked()
                    {
                        self.open_file();
                    }

                    let tex_save = self.get_icon(ctx, "save");
                    if big_tool(
                        ui,
                        &tex_save,
                        t(self.lang, "save"),
                        default_color,
                        None,
                        None,
                    )
                    .clicked()
                    {
                        self.save();
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    let rec = self.recorder.is_running();
                    let rec_label = if rec {
                        t(self.lang, "stop")
                    } else {
                        t(self.lang, "record")
                    };
                    let rec_color = if rec {
                        Color32::from_rgb(180, 30, 30)
                    } else {
                        Color32::from_rgb(200, 40, 40)
                    };
                    let tex_record = self.get_icon(ctx, "record");
                    if big_tool(ui, &tex_record, rec_label, rec_color, None, None).clicked() {
                        if rec {
                            self.stop_record();
                        } else {
                            self.start_record();
                        }
                    }

                    let tex_smart = self.get_icon(ctx, "tb_eye");
                    if big_tool(
                        ui,
                        &tex_smart,
                        t(self.lang, "smart_click"),
                        Color32::from_rgb(30, 90, 160),
                        None,
                        None,
                    )
                    .clicked()
                    {
                        self.open_edit(ActionKind::smart_click(0, 0, String::new(), 5000), None);
                    }

                    let playing = self.engine.snapshot_state() != RunState::Idle;
                    let play_label = if playing {
                        t(self.lang, "stop")
                    } else {
                        t(self.lang, "play")
                    };
                    let play_color = if playing {
                        Color32::from_rgb(160, 80, 0)
                    } else {
                        Color32::from_rgb(30, 150, 50)
                    };
                    let tex_play = self.get_icon(ctx, "play");
                    if big_tool(
                        ui,
                        &tex_play,
                        play_label,
                        play_color,
                        Some(Color32::from_rgb(229, 241, 251)),
                        Some(Color32::from_rgb(100, 160, 220)),
                    )
                    .clicked()
                    {
                        if playing {
                            self.engine.request_stop();
                        } else {
                            self.play();
                        }
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Bug/Debug
                    let tex_bug = self.get_icon(ctx, "bug");
                    if big_tool(
                        ui,
                        &tex_bug,
                        t(self.lang, "debug_run"),
                        default_color,
                        None,
                        None,
                    )
                    .clicked()
                    {
                        self.options.play_speed = 0.4;
                        self.play();
                    }

                    let tex_copy = self.get_icon(ctx, "copy");
                    if big_tool(
                        ui,
                        &tex_copy,
                        t(self.lang, "copy"),
                        default_color,
                        None,
                        None,
                    )
                    .clicked()
                    {
                        if let Some(i) = self.selected {
                            if let Some(a) = self.script.actions.get(i) {
                                self.clipboard = vec![a.clone()];
                            }
                        }
                    }

                    let tex_opt = self.get_icon(ctx, "tb_sch");
                    if big_tool(
                        ui,
                        &tex_opt,
                        t(self.lang, "optimize"),
                        default_color,
                        None,
                        None,
                    )
                    .clicked()
                    {
                        self.snapshot();
                        self.script.optimize_record();
                    }

                    let tex_up = self.get_icon(ctx, "up");
                    if big_tool(ui, &tex_up, "Up", default_color, None, None).clicked() {
                        self.move_sel(-1);
                    }

                    let tex_down = self.get_icon(ctx, "down");
                    if big_tool(ui, &tex_down, "Down", default_color, None, None).clicked() {
                        self.move_sel(1);
                    }

                    let tex_del = self.get_icon(ctx, "delete");
                    if big_tool(
                        ui,
                        &tex_del,
                        t(self.lang, "delete"),
                        Color32::from_rgb(180, 50, 50),
                        None,
                        None,
                    )
                    .clicked()
                    {
                        self.delete_selected();
                    }

                    let tex_help = self.get_icon(ctx, "help");
                    if big_tool(
                        ui,
                        &tex_help,
                        t(self.lang, "help_contents"),
                        default_color,
                        None,
                        None,
                    )
                    .clicked()
                    {
                        self.dialog = Dialog::Help;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let lab = format!("🌐 {}", self.lang.label());
                        if ui
                            .add(
                                egui::Button::new(RichText::new(lab).strong())
                                    .min_size(Vec2::new(52.0, 28.0)),
                            )
                            .clicked()
                        {
                            self.lang = self.lang.toggle();
                            if !self.recorder.is_running()
                                && self.engine.snapshot_state() == RunState::Idle
                            {
                                self.status = t(self.lang, "status_ready").to_string();
                            }
                        }
                    });
                });
                ui.add_space(4.0);
                let stroke = Stroke::new(1.0_f32, Color32::from_rgb(220, 225, 230));
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter()
                    .line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
            });
    }

    fn toolbox(&mut self, ui: &mut egui::Ui) {
        let frame = egui::Frame::none()
            .fill(Color32::from_rgb(250, 252, 255))
            .inner_margin(0.0);

        frame.show(ui, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(180.0, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        let tex_wrench = self.get_icon(ui.ctx(), "tb_more");
                        let img_rect = egui::Rect::from_center_size(
                            egui::pos2(ui.cursor().min.x + 8.0, ui.cursor().min.y + 8.0),
                            Vec2::new(16.0, 16.0),
                        );
                        egui::Image::new(&tex_wrench).tint(Color32::from_rgb(60, 80, 100)).paint_at(ui, img_rect);
                        ui.add_space(20.0);
                        ui.label(
                            RichText::new("TOOLBOX")
                                .strong()
                                .color(Color32::from_rgb(60, 80, 100))
                                .size(13.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(16.0);
                            ui.label(RichText::new("📌").size(10.0).color(Color32::GRAY));
                        });
                    });
                    ui.add_space(12.0);

                    let stroke = Stroke::new(1.0_f32, Color32::from_rgb(220, 225, 230));
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(180.0, 1.0), Sense::hover());
                    ui.painter()
                        .line_segment([rect.left_top(), rect.right_top()], stroke);
                    ui.add_space(12.0);

                    let groups = [
                        (
                            "Input Actions",
                            [
                                ("tb_mouse", 0, ToolboxId::Mouse, "tb_mouse"),
                                ("tb_keyboard", 1, ToolboxId::Keyboard, "tb_key"),
                                ("tb_image", 2, ToolboxId::Image, "tb_eye"),
                            ]
                            .as_slice(),
                        ),
                        (
                            "Flow & Logic",
                            [
                                ("tb_flow", 5, ToolboxId::Flow, "tb_if"),
                                ("tb_var", 6, ToolboxId::Var, "tb_var"),
                                ("tb_fn", 7, ToolboxId::Fn, "tb_fn"),
                            ]
                            .as_slice(),
                        ),
                        (
                            "System & Tasks",
                            [
                                ("tb_window", 3, ToolboxId::Window, "tb_win"),
                                ("tb_file", 4, ToolboxId::File, "tb_folder"),
                                ("tb_schedule", 9, ToolboxId::Schedule, "tb_sch"),
                                ("tb_more", 8, ToolboxId::More, "tb_more"),
                            ]
                            .as_slice(),
                        ),
                    ];

                    for (i, (group_title, items)) in groups.into_iter().enumerate() {
                        if i > 0 {
                            ui.add_space(16.0);
                        }

                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            ui.label(
                                RichText::new(group_title.to_uppercase())
                                    .size(11.0)
                                    .strong()
                                    .color(Color32::from_rgb(130, 140, 150)),
                            );
                        });
                        ui.add_space(6.0);

                        for &(key, id, kind, icon_name) in items {
                            let selected = self.hover_tb == Some(id);
                            let tip = t(self.lang, key);
                            let caption = tip.split(['/', '&', '(']).next().unwrap_or(tip).trim();

                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::new(180.0, 36.0), Sense::click());

                            if selected {
                                let item_rect = rect.shrink2(Vec2::new(4.0, 1.0));
                                ui.painter().rect(
                                    item_rect,
                                    3.0,
                                    Color32::from_rgb(230, 240, 250),
                                    Stroke::new(1.0_f32, Color32::from_rgb(153, 204, 255)),
                                );
                            } else if resp.hovered() {
                                let item_rect = rect.shrink2(Vec2::new(4.0, 1.0));
                                ui.painter().rect_filled(
                                    item_rect,
                                    3.0,
                                    Color32::from_rgb(245, 248, 252),
                                );
                            }

                            if resp.hovered() {
                                self.hover_tb = Some(id);
                            }

                            let tex = self.get_icon(ui.ctx(), icon_name);
                            let img_rect = egui::Rect::from_min_size(
                                rect.min + Vec2::new(16.0, 6.0),
                                Vec2::new(24.0, 24.0),
                            );
                            let img_color = if selected { Color32::from_rgb(0, 80, 180) } else { Color32::from_rgb(60, 70, 80) };
                            egui::Image::new(&tex).tint(img_color).paint_at(ui, img_rect);

                            let text_color = if selected {
                                Color32::from_rgb(0, 80, 180)
                            } else {
                                Color32::from_rgb(60, 70, 80)
                            };

                            ui.painter().text(
                                rect.min + Vec2::new(42.0, 18.0),
                                egui::Align2::LEFT_CENTER,
                                caption,
                                egui::FontId::proportional(13.0),
                                text_color,
                            );

                            let r = resp.on_hover_text(tip);
                            if r.clicked() {
                                match kind {
                                    ToolboxId::Mouse => self.open_edit(
                                        ActionKind::MouseClick {
                                            button: MouseBtn::Left,
                                            x: 0,
                                            y: 0,
                                            clicks: 1,
                                        },
                                        None,
                                    ),
                                    ToolboxId::Keyboard => self.open_edit(
                                        ActionKind::TypeText {
                                            text: String::new(),
                                            interval_ms: 20,
                                        },
                                        None,
                                    ),
                                    ToolboxId::Image => self.open_edit(
                                        ActionKind::smart_click(0, 0, String::new(), 5000),
                                        None,
                                    ),
                                    ToolboxId::Window => self.open_edit(
                                        ActionKind::ActivateWindow {
                                            title: String::new(),
                                        },
                                        None,
                                    ),
                                    ToolboxId::File => self.open_edit(
                                        ActionKind::OpenFile {
                                            path: String::new(),
                                        },
                                        None,
                                    ),
                                    ToolboxId::Flow => {
                                        self.snapshot();
                                        self.insert_kind(ActionKind::If {
                                            expr: "true".into(),
                                        });
                                        self.insert_kind(ActionKind::EndIf);
                                    }
                                    ToolboxId::Var => self.open_edit(
                                        ActionKind::SetVar {
                                            name: "v".into(),
                                            value: "0".into(),
                                        },
                                        None,
                                    ),
                                    ToolboxId::Fn => self.open_edit(
                                        ActionKind::CallFunction {
                                            name: "Delay".into(),
                                        },
                                        None,
                                    ),
                                    ToolboxId::More => self.dialog = Dialog::Options,
                                    ToolboxId::Schedule => self.dialog = Dialog::Schedule,
                                }
                            }
                        }
                    }
                },
            );
        });
    }

    fn action_table(&mut self, ui: &mut egui::Ui) {
        let playing_idx = if self.engine.snapshot_state() != RunState::Idle {
            Some(self.engine.current_index())
        } else {
            None
        };

        let header_h = 24.0;
        let row_h = 24.0; // slightly taller

        let w_step = 40.0;
        let w_action = 140.0;
        let w_target = 200.0;
        let w_delay = 80.0;
        let w_details =
            (ui.available_width() - w_step - w_action - w_target - w_delay - 20.0).max(100.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            header_cell(ui, "ID", w_step, header_h);
            header_cell(ui, "Action", w_action, header_h);
            header_cell(ui, "Target / Position", w_target, header_h);
            header_cell(ui, "Details & Parameters", w_details, header_h);
            header_cell(ui, "Delay / Wait", w_delay, header_h);
        });

        let mut edit_i = None;
        let mut sel_i = None;
        let mut toggle_i = None;

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 4.0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for i in 0..self.script.actions.len() {
                    let indent = self.script.indent_of(i);
                    let selected = self.selected == Some(i);
                    let playing = playing_idx == Some(i);

                    let icon_name = match self.script.actions[i].kind {
                        ActionKind::MouseClick { .. } | ActionKind::MouseMove { .. } => "tb_mouse",
                        ActionKind::TypeText { .. } | ActionKind::KeyPress { .. } => "tb_key",
                        ActionKind::Delay { .. } => "tb_sch",
                        ActionKind::SmartClick { .. } | ActionKind::SearchPicture { .. } => {
                            "tb_eye"
                        }
                        ActionKind::If { .. }
                        | ActionKind::For { .. }
                        | ActionKind::While { .. } => "tb_if",
                        ActionKind::SetVar { .. } => "tb_var",
                        ActionKind::FunctionEntry | ActionKind::CallFunction { .. } => "tb_fn",
                        ActionKind::ActivateWindow { .. } => "tb_win",
                        ActionKind::OpenFile { .. } => "tb_folder",
                        _ => "tb_more",
                    };
                    let tex = self.get_icon(ui.ctx(), icon_name);

                    let a = &self.script.actions[i];

                    let bg = if selected {
                        Color32::from_rgb(51, 153, 255) // #3399FF
                    } else if playing {
                        Color32::from_rgb(255, 243, 180)
                    } else if i % 2 == 0 {
                        Color32::from_rgb(255, 255, 255)
                    } else {
                        Color32::from_rgb(250, 250, 250)
                    };

                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(w_step + w_action + w_target + w_details + w_delay, row_h),
                        Sense::click(),
                    );
                    ui.painter().rect_filled(rect, 0.0, bg);

                    let (_text_color, name_color, delay_color, step_color) = if selected {
                        (
                            Color32::WHITE,
                            Color32::WHITE,
                            Color32::WHITE,
                            Color32::WHITE,
                        )
                    } else if !a.enabled {
                        (Color32::GRAY, Color32::GRAY, Color32::GRAY, Color32::GRAY)
                    } else {
                        let nc = match &a.kind {
                            ActionKind::FunctionEntry | ActionKind::EndFunction => {
                                Color32::from_rgb(0, 128, 0)
                            }
                            ActionKind::Comment { .. } => Color32::from_rgb(0, 128, 0),
                            ActionKind::If { .. }
                            | ActionKind::Else
                            | ActionKind::EndIf
                            | ActionKind::For { .. }
                            | ActionKind::EndFor
                            | ActionKind::While { .. }
                            | ActionKind::EndWhile
                            | ActionKind::Goto { .. }
                            | ActionKind::Label { .. } => Color32::from_rgb(128, 0, 128),
                            _ => Color32::from_rgb(20, 20, 20),
                        };
                        (
                            Color32::from_rgb(20, 20, 20),
                            nc,
                            Color32::GRAY,
                            Color32::DARK_GRAY,
                        )
                    };

                    let (act, tgt, det) = a.kind.format_columns();
                    let act_str = if a.name.is_empty() || a.name == a.kind.default_name() {
                        act
                    } else {
                        a.name.clone()
                    };
                    let pad = 8.0 + indent as f32 * 16.0;

                    let mut x = rect.min.x;
                    let y = rect.center().y;
                    let stroke = Stroke::new(1.0_f32, Color32::from_rgb(230, 230, 230));

                    // Step
                    ui.painter().text(
                        egui::pos2(x + 8.0, y),
                        egui::Align2::LEFT_CENTER,
                        format!("{:03}", i + 1),
                        egui::FontId::proportional(12.0),
                        step_color,
                    );
                    x += w_step;
                    ui.painter().line_segment(
                        [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                        stroke,
                    );

                    // Action
                    let img_rect = egui::Rect::from_center_size(
                        egui::pos2(x + pad + 8.0, y),
                        Vec2::new(16.0, 16.0),
                    );
                    egui::Image::new(&tex).tint(name_color).paint_at(ui, img_rect);

                    ui.painter().text(
                        egui::pos2(x + pad + 20.0, y),
                        egui::Align2::LEFT_CENTER,
                        act_str,
                        egui::FontId::proportional(13.0),
                        name_color,
                    );
                    x += w_action;
                    ui.painter().line_segment(
                        [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                        stroke,
                    );

                    // Target
                    ui.painter().text(
                        egui::pos2(x + 8.0, y),
                        egui::Align2::LEFT_CENTER,
                        tgt,
                        egui::FontId::proportional(12.0),
                        name_color,
                    );
                    x += w_target;
                    ui.painter().line_segment(
                        [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                        stroke,
                    );

                    // Details
                    ui.painter().text(
                        egui::pos2(x + 8.0, y),
                        egui::Align2::LEFT_CENTER,
                        det,
                        egui::FontId::proportional(12.0),
                        name_color,
                    );
                    x += w_details;
                    ui.painter().line_segment(
                        [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                        stroke,
                    );

                    // Delay
                    if a.delay_ms > 0 {
                        ui.painter().text(
                            egui::pos2(x + 8.0, y),
                            egui::Align2::LEFT_CENTER,
                            format!("{} ms", a.delay_ms),
                            egui::FontId::proportional(12.0),
                            delay_color,
                        );
                    } else if selected {
                        ui.painter().text(
                            egui::pos2(x + 8.0, y),
                            egui::Align2::LEFT_CENTER,
                            "0 ms".to_string(),
                            egui::FontId::proportional(12.0),
                            delay_color,
                        );
                    }

                    ui.painter()
                        .line_segment([rect.left_bottom(), rect.right_bottom()], stroke);

                    if resp.clicked() {
                        sel_i = Some(i);
                    }
                    if resp.double_clicked() {
                        edit_i = Some(i);
                    }
                    resp.context_menu(|ui| {
                        if ui.button(t(self.lang, "enable")).clicked() {
                            toggle_i = Some((i, true));
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "disable")).clicked() {
                            toggle_i = Some((i, false));
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "delete")).clicked() {
                            sel_i = Some(i);
                            ui.close_menu();
                        }
                    });
                }
            });

        if let Some(i) = sel_i {
            self.selected = Some(i);
        }
        if let Some(i) = edit_i {
            let kind = self.script.actions[i].kind.clone();
            self.open_edit(kind, Some(i));
        }
        if let Some((i, en)) = toggle_i {
            if let Some(a) = self.script.actions.get_mut(i) {
                a.enabled = en;
            }
        }
    }

    fn play_options(&mut self, ui: &mut egui::Ui) {
        // Header
        egui::Frame::none()
            .fill(Color32::from_rgb(248, 249, 250))
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("⚙")
                            .size(14.0)
                            .color(Color32::from_rgb(80, 100, 120)),
                    );
                    ui.label(
                        RichText::new("Play Options")
                            .strong()
                            .size(13.0)
                            .color(Color32::from_rgb(40, 55, 70)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("⚙").size(11.0).color(Color32::GRAY));
                    });
                });
            });

        let sep_stroke = Stroke::new(1.0_f32, Color32::from_rgb(220, 224, 229));
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter()
            .line_segment([r.left_top(), r.right_top()], sep_stroke);

        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(8.0, 0.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;

                // ── Play Repetition ───────────────────────────────────────
                ui.add_space(10.0);
                ui.label(
                    RichText::new("Play Repetition")
                        .strong()
                        .size(12.0)
                        .color(Color32::from_rgb(50, 65, 80)),
                );
                ui.add_space(4.0);

                ui.radio_value(&mut self.repeat, RepeatMode::Once, "Play script once");

                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.repeat, RepeatMode::Times, "Play script");
                    ui.add_enabled(
                        self.repeat == RepeatMode::Times,
                        egui::DragValue::new(&mut self.repeat_n)
                            .clamp_range(1..=1_000_000)
                            .speed(1),
                    );
                    ui.label("times");
                });

                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.repeat, RepeatMode::Duration, "Play script for");
                    ui.add_enabled(
                        self.repeat == RepeatMode::Duration,
                        egui::DragValue::new(&mut self.duration_n)
                            .clamp_range(1..=10_000)
                            .speed(1),
                    );
                    egui::ComboBox::from_id_source("dur_unit")
                        .selected_text(match self.duration_unit {
                            DurationUnit::Seconds => "secs",
                            DurationUnit::Minutes => "mins",
                            DurationUnit::Hours => "hrs",
                        })
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.duration_unit,
                                DurationUnit::Seconds,
                                "secs",
                            );
                            ui.selectable_value(
                                &mut self.duration_unit,
                                DurationUnit::Minutes,
                                "mins",
                            );
                            ui.selectable_value(
                                &mut self.duration_unit,
                                DurationUnit::Hours,
                                "hrs",
                            );
                        });
                });

                ui.radio_value(
                    &mut self.repeat,
                    RepeatMode::Infinite,
                    "Repeat until Stop key",
                );

                // separator
                ui.add_space(4.0);
                let (r, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter()
                    .line_segment([r.left_top(), r.right_top()], sep_stroke);

                // ── Execution Options ─────────────────────────────────────
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Execution Options")
                        .strong()
                        .size(12.0)
                        .color(Color32::from_rgb(50, 65, 80)),
                );
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.speed_enabled, "Speed");
                    ui.add_enabled(
                        self.speed_enabled,
                        egui::DragValue::new(&mut self.options.play_speed)
                            .clamp_range(0.1..=10.0)
                            .speed(0.1)
                            .fixed_decimals(1)
                            .suffix("x"),
                    );
                });
                ui.checkbox(&mut self.show_recent, "Show cursor movement trail");
                ui.checkbox(&mut self.no_activate, "Lock keyboard and mouse");

                let mut min_start = false;
                ui.checkbox(&mut min_start, "Minimize window on start");
                ui.checkbox(&mut self.shutdown, "Shutdown PC on finish");

                // separator
                ui.add_space(4.0);
                let (r, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter()
                    .line_segment([r.left_top(), r.right_top()], sep_stroke);

                // ── Hotkeys ───────────────────────────────────────────────
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Hotkeys")
                        .strong()
                        .size(12.0)
                        .color(Color32::from_rgb(50, 65, 80)),
                );
                ui.add_space(4.0);

                // Draw a key badge (F9 / F12 in a styled box)
                let draw_key_badge = |ui: &mut egui::Ui, key: &str, is_red: bool| {
                    let font_id = egui::FontId::proportional(11.0);
                    let fg = if is_red {
                        Color32::from_rgb(200, 50, 50)
                    } else {
                        Color32::from_rgb(40, 80, 160)
                    };
                    let border = if is_red {
                        Color32::from_rgb(200, 50, 50)
                    } else {
                        Color32::from_rgb(100, 140, 210)
                    };
                    let galley = ui.painter().layout_no_wrap(key.to_string(), font_id, fg);
                    let pad = Vec2::new(8.0, 3.0);
                    let size = galley.size() + pad * 2.0;
                    let (badge_rect, _) = ui.allocate_exact_size(size, Sense::hover());
                    ui.painter()
                        .rect_filled(badge_rect, 3.0, Color32::from_rgb(245, 247, 250));
                    ui.painter()
                        .rect_stroke(badge_rect, 3.0, Stroke::new(1.0_f32, border));
                    ui.painter().galley(
                        egui::pos2(badge_rect.min.x + pad.x, badge_rect.min.y + pad.y),
                        galley,
                        fg,
                    );
                };

                ui.horizontal(|ui| {
                    ui.label("Start / Pause:");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_key_badge(ui, "F9", false);
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Stop Execution:");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_key_badge(ui, "F12", true);
                    });
                });
            });

        // ── START PLAYBACK button (always at bottom) ─────────────────────
        let avail = ui.available_rect_before_wrap();
        let btn_h = 40.0;
        // push to bottom
        if avail.height() > btn_h {
            ui.add_space(avail.height() - btn_h);
        }

        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, btn_h), Sense::click());
        let bg = if resp.hovered() {
            Color32::from_rgb(30, 110, 220)
        } else {
            Color32::from_rgb(22, 120, 255)
        };
        ui.painter().rect_filled(rect, 0.0, bg);

        // Play triangle icon (▶)
        let icon_center = egui::pos2(rect.min.x + 22.0, rect.center().y);
        let r = 7.0_f32;
        let pts = [
            egui::pos2(icon_center.x - r * 0.6, icon_center.y - r),
            egui::pos2(icon_center.x - r * 0.6, icon_center.y + r),
            egui::pos2(icon_center.x + r, icon_center.y),
        ];
        let circle_rect = egui::Rect::from_center_size(icon_center, Vec2::splat(r * 2.2));
        ui.painter().circle_stroke(
            circle_rect.center(),
            r * 1.1,
            Stroke::new(1.5_f32, Color32::WHITE),
        );
        ui.painter().add(egui::Shape::convex_polygon(
            pts.to_vec(),
            Color32::WHITE,
            Stroke::NONE,
        ));

        ui.painter().text(
            egui::pos2(rect.center().x + 10.0, rect.center().y),
            egui::Align2::CENTER_CENTER,
            "START PLAYBACK (F9)",
            egui::FontId::proportional(13.0),
            Color32::WHITE,
        );

        if resp.clicked() {
            self.play();
        }
    }

    fn file_path_windows(&mut self, ctx: &egui::Context) {
        if self.show_open {
            let mut open = true;
            egui::Window::new(t(self.lang, "open"))
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(t(self.lang, "path"));
                    ui.text_edit_singleline(&mut self.path_input);
                    ui.horizontal(|ui| {
                        if ui.button(t(self.lang, "ok")).clicked() {
                            let p = PathBuf::from(self.path_input.trim());
                            self.open_from_path(p);
                            self.show_open = false;
                        }
                        if ui.button(t(self.lang, "cancel")).clicked() {
                            self.show_open = false;
                        }
                    });
                });
            if !open {
                self.show_open = false;
            }
        }
        if self.show_save {
            let mut open = true;
            egui::Window::new(t(self.lang, "save_as"))
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(t(self.lang, "path"));
                    ui.text_edit_singleline(&mut self.path_input);
                    ui.horizontal(|ui| {
                        if ui.button(t(self.lang, "ok")).clicked() {
                            let p = PathBuf::from(self.path_input.trim());
                            self.save_to(p);
                            self.show_save = false;
                        }
                        if ui.button(t(self.lang, "cancel")).clicked() {
                            self.show_save = false;
                        }
                    });
                });
            if !open {
                self.show_save = false;
            }
        }
    }

    fn draw_dialogs(&mut self, ctx: &egui::Context) {
        if self.dialog == Dialog::None {
            return;
        }
        let mut open = true;
        let title = match self.dialog {
            Dialog::Mouse => t(self.lang, "dlg_mouse"),
            Dialog::Keyboard => t(self.lang, "dlg_key"),
            Dialog::Delay => t(self.lang, "insert_delay"),
            Dialog::Smart => t(self.lang, "dlg_smart"),
            Dialog::Search => t(self.lang, "dlg_search"),
            Dialog::Window => t(self.lang, "dlg_window"),
            Dialog::File => t(self.lang, "dlg_file"),
            Dialog::Variable => t(self.lang, "insert_var"),
            Dialog::If => t(self.lang, "insert_if"),
            Dialog::For => t(self.lang, "insert_for"),
            Dialog::While => t(self.lang, "insert_while"),
            Dialog::Label => t(self.lang, "label"),
            Dialog::Goto => "Goto",
            Dialog::Message => t(self.lang, "insert_msg"),
            Dialog::Comment => t(self.lang, "insert_comment"),
            Dialog::CallFn => t(self.lang, "insert_fn"),
            Dialog::PlayScript => t(self.lang, "script"),
            Dialog::Options => t(self.lang, "dlg_options"),
            Dialog::Hotkeys => t(self.lang, "dlg_hotkeys"),
            Dialog::About => t(self.lang, "dlg_about"),
            Dialog::Help => t(self.lang, "dlg_help"),
            Dialog::Log => t(self.lang, "dlg_log"),
            Dialog::Schedule => t(self.lang, "dlg_schedule"),
            Dialog::Clicker => t(self.lang, "dlg_clicker"),
            Dialog::Presser => t(self.lang, "dlg_presser"),
            Dialog::None => "",
        }
        .to_string();

        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t(self.lang, "step_name"));
                    ui.text_edit_singleline(&mut self.draft_name);
                });
                if !matches!(
                    self.dialog,
                    Dialog::About
                        | Dialog::Help
                        | Dialog::Log
                        | Dialog::Options
                        | Dialog::Hotkeys
                        | Dialog::Schedule
                        | Dialog::Clicker
                        | Dialog::Presser
                ) {
                    ui.horizontal(|ui| {
                        ui.label(t(self.lang, "delay_ms"));
                        ui.add(
                            egui::DragValue::new(&mut self.draft_delay).clamp_range(0..=600_000),
                        );
                    });
                    ui.separator();
                }

                match self.dialog {
                    Dialog::Mouse => self.ui_mouse(ui),
                    Dialog::Keyboard => self.ui_key(ui),
                    Dialog::Delay => {
                        if let ActionKind::Delay { ms } = &mut self.draft {
                            ui.horizontal(|ui| {
                                ui.label(t(self.lang, "delay_ms"));
                                ui.add(egui::DragValue::new(ms).clamp_range(0..=3_600_000));
                            });
                        }
                    }
                    Dialog::Smart => self.ui_smart(ui),
                    Dialog::Search => self.ui_search(ui),
                    Dialog::Window => self.ui_window(ui),
                    Dialog::File => self.ui_file(ui),
                    Dialog::Variable => {
                        if let ActionKind::SetVar { name, value } = &mut self.draft {
                            ui.horizontal(|ui| {
                                ui.label(t(self.lang, "var_name"));
                                ui.text_edit_singleline(name);
                            });
                            ui.horizontal(|ui| {
                                ui.label(t(self.lang, "value"));
                                ui.text_edit_singleline(value);
                            });
                        }
                    }
                    Dialog::If => {
                        if let ActionKind::If { expr } = &mut self.draft {
                            ui.horizontal(|ui| {
                                ui.label(t(self.lang, "expr"));
                                ui.text_edit_singleline(expr);
                            });
                        }
                    }
                    Dialog::For => {
                        if let ActionKind::For {
                            var,
                            from,
                            to,
                            step,
                        } = &mut self.draft
                        {
                            ui.horizontal(|ui| {
                                ui.label(t(self.lang, "var_name"));
                                ui.text_edit_singleline(var);
                            });
                            ui.horizontal(|ui| {
                                ui.label(t(self.lang, "from"));
                                ui.add(egui::DragValue::new(from));
                                ui.label(t(self.lang, "to"));
                                ui.add(egui::DragValue::new(to));
                                ui.label(t(self.lang, "step"));
                                ui.add(egui::DragValue::new(step));
                            });
                        }
                    }
                    Dialog::While => {
                        if let ActionKind::While { expr } = &mut self.draft {
                            ui.horizontal(|ui| {
                                ui.label(t(self.lang, "expr"));
                                ui.text_edit_singleline(expr);
                            });
                        }
                    }
                    Dialog::Label => {
                        if let ActionKind::Label { name } = &mut self.draft {
                            ui.text_edit_singleline(name);
                        }
                    }
                    Dialog::Goto => {
                        if let ActionKind::Goto { name } = &mut self.draft {
                            ui.text_edit_singleline(name);
                        }
                    }
                    Dialog::Message => {
                        if let ActionKind::MessageBox { text } = &mut self.draft {
                            ui.text_edit_multiline(text);
                        }
                    }
                    Dialog::Comment => {
                        if let ActionKind::Comment { text } = &mut self.draft {
                            ui.text_edit_multiline(text);
                        }
                    }
                    Dialog::CallFn => {
                        if let ActionKind::CallFunction { name } = &mut self.draft {
                            ui.horizontal(|ui| {
                                ui.label(t(self.lang, "function"));
                                ui.text_edit_singleline(name);
                            });
                            ui.label(
                                "Delay, RandomNumber, GetMousePositionX, TypeText, FindWindow…",
                            );
                        }
                    }
                    Dialog::PlayScript => {
                        if let ActionKind::PlayScript { path } = &mut self.draft {
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(path);
                            });
                        }
                    }
                    Dialog::Options => {
                        ui.checkbox(
                            &mut self.options.minimize_on_play,
                            t(self.lang, "opt_minimize"),
                        );
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "opt_sample"));
                            ui.add(
                                egui::DragValue::new(&mut self.options.sample_ms)
                                    .clamp_range(5..=500),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "opt_ignore_move"));
                            ui.add(
                                egui::DragValue::new(&mut self.options.ignore_px)
                                    .clamp_range(0..=50),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "speed"));
                            ui.add(egui::Slider::new(&mut self.options.play_speed, 0.1..=8.0));
                        });
                    }
                    Dialog::Hotkeys => {
                        ui.label(format!(
                            "{} : {}",
                            t(self.lang, "hk_record"),
                            self.options.hk_record
                        ));
                        ui.label(format!(
                            "{} : {}",
                            t(self.lang, "hk_play"),
                            self.options.hk_play
                        ));
                        ui.label(format!(
                            "{} : {}",
                            t(self.lang, "hk_pause"),
                            self.options.hk_pause
                        ));
                        ui.label("F7 / F8 — debug step (play at reduced speed)");
                    }
                    Dialog::About => {
                        ui.label(t(self.lang, "about_body"));
                    }
                    Dialog::Help => {
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "search_help"));
                            ui.text_edit_singleline(&mut self.help_query);
                        });
                        ui.separator();
                        ui.label(t(self.lang, "help_body"));
                    }
                    Dialog::Log => {
                        if ui.button("Clear").clicked() {
                            self.engine.clear_log();
                        }
                        egui::ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                for line in self.engine.logs() {
                                    ui.monospace(format!("{}  {}", line.time, line.text));
                                }
                            });
                    }
                    Dialog::Schedule => {
                        if ui.button(t(self.lang, "add_task")).clicked() {
                            self.tasks.push(ScheduledTask {
                                name: "Task".into(),
                                script: self
                                    .path
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_default(),
                                when: "09:00".into(),
                            });
                        }
                        let mut rm = None;
                        for (i, task) in self.tasks.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(&mut task.name);
                                ui.text_edit_singleline(&mut task.when);
                                ui.text_edit_singleline(&mut task.script);
                                if ui.button(t(self.lang, "remove_task")).clicked() {
                                    rm = Some(i);
                                }
                            });
                        }
                        if let Some(i) = rm {
                            self.tasks.remove(i);
                        }
                    }
                    Dialog::Clicker => {
                        ui.horizontal(|ui| {
                            ui.label("X");
                            ui.add(egui::DragValue::new(&mut self.clicker_x));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut self.clicker_y));
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "interval_ms"));
                            ui.add(egui::DragValue::new(&mut self.clicker_interval));
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "repeat_count"));
                            ui.add(egui::DragValue::new(&mut self.clicker_count));
                        });
                        if ui.button(t(self.lang, "play")).clicked() {
                            self.snapshot();
                            let mut s = Script::default();
                            s.actions.clear();
                            s.actions.push(Action::new(ActionKind::FunctionEntry));
                            s.actions.push(Action::new(ActionKind::For {
                                var: "c".into(),
                                from: 1,
                                to: self.clicker_count as i64,
                                step: 1,
                            }));
                            s.actions.push(Action::new(ActionKind::MouseClick {
                                button: MouseBtn::Left,
                                x: self.clicker_x,
                                y: self.clicker_y,
                                clicks: 1,
                            }));
                            s.actions.push(Action::new(ActionKind::Delay {
                                ms: self.clicker_interval,
                            }));
                            s.actions.push(Action::new(ActionKind::EndFor));
                            s.actions.push(Action::new(ActionKind::EndFunction));
                            self.script = s;
                            self.play();
                        }
                    }
                    Dialog::Presser => {
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "key"));
                            ui.text_edit_singleline(&mut self.presser_key);
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "interval_ms"));
                            ui.add(egui::DragValue::new(&mut self.presser_interval));
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "repeat_count"));
                            ui.add(egui::DragValue::new(&mut self.presser_count));
                        });
                        if ui.button(t(self.lang, "play")).clicked() {
                            self.snapshot();
                            let mut s = Script::default();
                            s.actions.clear();
                            s.actions.push(Action::new(ActionKind::FunctionEntry));
                            s.actions.push(Action::new(ActionKind::For {
                                var: "k".into(),
                                from: 1,
                                to: self.presser_count as i64,
                                step: 1,
                            }));
                            s.actions.push(Action::new(ActionKind::KeyPress {
                                key: self.presser_key.clone(),
                            }));
                            s.actions.push(Action::new(ActionKind::Delay {
                                ms: self.presser_interval,
                            }));
                            s.actions.push(Action::new(ActionKind::EndFor));
                            s.actions.push(Action::new(ActionKind::EndFunction));
                            self.script = s;
                            self.play();
                        }
                    }
                    Dialog::None => {}
                }

                if !matches!(
                    self.dialog,
                    Dialog::About
                        | Dialog::Help
                        | Dialog::Log
                        | Dialog::Options
                        | Dialog::Hotkeys
                        | Dialog::Schedule
                        | Dialog::Clicker
                        | Dialog::Presser
                ) {
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button(t(self.lang, "ok")).clicked() {
                            self.commit_draft();
                        }
                        if ui.button(t(self.lang, "cancel")).clicked() {
                            self.dialog = Dialog::None;
                        }
                    });
                }
            });

        if !open {
            self.dialog = Dialog::None;
        }
    }

    fn ui_mouse(&mut self, ui: &mut egui::Ui) {
        let mut mode = match &self.draft {
            ActionKind::MouseMove { .. } => 0,
            ActionKind::MouseClick { clicks, .. } if *clicks >= 2 => 2,
            ActionKind::MouseClick { .. } => 1,
            ActionKind::MouseDrag { .. } => 3,
            ActionKind::MouseWheel { .. } => 4,
            _ => 1,
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut mode, 0, t(self.lang, "mouse_move"));
            ui.selectable_value(&mut mode, 1, t(self.lang, "mouse_click"));
            ui.selectable_value(&mut mode, 2, t(self.lang, "mouse_dbl"));
            ui.selectable_value(&mut mode, 3, t(self.lang, "mouse_drag"));
            ui.selectable_value(&mut mode, 4, t(self.lang, "mouse_wheel"));
        });
        let (mut x, mut y, mut x2, mut y2, mut btn, mut delta) = match &self.draft {
            ActionKind::MouseMove { x, y } => (*x, *y, *x + 10, *y + 10, MouseBtn::Left, 0),
            ActionKind::MouseClick { x, y, button, .. } => (*x, *y, *x, *y, *button, 0),
            ActionKind::MouseDrag {
                x1,
                y1,
                x2,
                y2,
                button,
            } => (*x1, *y1, *x2, *y2, *button, 0),
            ActionKind::MouseWheel { delta } => (0, 0, 0, 0, MouseBtn::Left, *delta),
            _ => (0, 0, 0, 0, MouseBtn::Left, 0),
        };
        if mode != 4 {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut y));
            });
        }
        if mode == 3 {
            ui.horizontal(|ui| {
                ui.label("X2");
                ui.add(egui::DragValue::new(&mut x2));
                ui.label("Y2");
                ui.add(egui::DragValue::new(&mut y2));
            });
        }
        if mode == 1 || mode == 2 || mode == 3 {
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "button"));
                ui.selectable_value(&mut btn, MouseBtn::Left, t(self.lang, "left"));
                ui.selectable_value(&mut btn, MouseBtn::Right, t(self.lang, "right"));
                ui.selectable_value(&mut btn, MouseBtn::Middle, t(self.lang, "middle"));
            });
        }
        if mode == 4 {
            ui.horizontal(|ui| {
                ui.label("Delta");
                ui.add(egui::DragValue::new(&mut delta));
            });
        }
        self.draft = match mode {
            0 => ActionKind::MouseMove { x, y },
            2 => ActionKind::MouseClick {
                button: btn,
                x,
                y,
                clicks: 2,
            },
            3 => ActionKind::MouseDrag {
                button: btn,
                x1: x,
                y1: y,
                x2,
                y2,
            },
            4 => ActionKind::MouseWheel { delta },
            _ => ActionKind::MouseClick {
                button: btn,
                x,
                y,
                clicks: 1,
            },
        };
    }

    fn ui_key(&mut self, ui: &mut egui::Ui) {
        let mut mode = if matches!(self.draft, ActionKind::KeyPress { .. }) {
            1
        } else {
            0
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut mode, 0, t(self.lang, "type_text"));
            ui.selectable_value(&mut mode, 1, t(self.lang, "key_press"));
        });
        if mode == 0 {
            let (mut text, mut iv) = match &self.draft {
                ActionKind::TypeText { text, interval_ms } => (text.clone(), *interval_ms),
                _ => (String::new(), 20u64),
            };
            ui.label(t(self.lang, "text"));
            ui.text_edit_multiline(&mut text);
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "interval_ms"));
                ui.add(egui::DragValue::new(&mut iv).clamp_range(0..=2000));
            });
            self.draft = ActionKind::TypeText {
                text,
                interval_ms: iv,
            };
        } else {
            let mut key = match &self.draft {
                ActionKind::KeyPress { key } => key.clone(),
                _ => "Enter".into(),
            };
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "key"));
                ui.text_edit_singleline(&mut key);
            });
            ui.label("Ctrl+C, Alt+F4, Enter, Tab, F5…");
            self.draft = ActionKind::KeyPress { key };
        }
    }

    fn ui_smart(&mut self, ui: &mut egui::Ui) {
        if let ActionKind::SmartClick {
            x,
            y,
            image,
            timeout_ms,
            confidence,
            on_fail,
            ox,
            oy,
        } = &mut self.draft
        {
            ui.label("Image path required. No match = no click.");
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(x));
                ui.label("Y");
                ui.add(egui::DragValue::new(y));
            });
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "image_file"));
                ui.text_edit_singleline(image);
            });
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "timeout"));
                ui.add(egui::DragValue::new(timeout_ms));
                ui.label("conf");
                ui.add(egui::Slider::new(confidence, 0.75..=0.99));
            });
            ui.horizontal(|ui| {
                ui.label("on_fail");
                ui.text_edit_singleline(on_fail);
            });
            ui.horizontal(|ui| {
                ui.label("ox");
                let mut v = ox.unwrap_or(0);
                if ui.add(egui::DragValue::new(&mut v)).changed() {
                    *ox = Some(v);
                }
                ui.label("oy");
                let mut w = oy.unwrap_or(0);
                if ui.add(egui::DragValue::new(&mut w)).changed() {
                    *oy = Some(w);
                }
                ui.label("(offset from image)");
            });
        }
    }

    fn ui_search(&mut self, ui: &mut egui::Ui) {
        if let ActionKind::SearchPicture {
            image,
            timeout_ms,
            save_x,
            save_y,
            ..
        } = &mut self.draft
        {
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "image_file"));
                ui.text_edit_singleline(image);
            });
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "timeout"));
                ui.add(egui::DragValue::new(timeout_ms));
            });
            ui.horizontal(|ui| {
                ui.label("Save X →");
                ui.text_edit_singleline(save_x);
                ui.label("Save Y →");
                ui.text_edit_singleline(save_y);
            });
        }
    }

    fn ui_window(&mut self, ui: &mut egui::Ui) {
        let mut close = matches!(self.draft, ActionKind::CloseWindow { .. });
        let mut title = match &self.draft {
            ActionKind::ActivateWindow { title } | ActionKind::CloseWindow { title } => {
                title.clone()
            }
            _ => String::new(),
        };
        ui.checkbox(&mut close, t(self.lang, "win_close"));
        ui.horizontal(|ui| {
            ui.label(t(self.lang, "window_title"));
            ui.text_edit_singleline(&mut title);
        });
        self.draft = if close {
            ActionKind::CloseWindow { title }
        } else {
            ActionKind::ActivateWindow { title }
        };
    }

    fn ui_file(&mut self, ui: &mut egui::Ui) {
        let mut mode = match &self.draft {
            ActionKind::OpenUrl { .. } => 1,
            ActionKind::OpenFolder { .. } => 2,
            _ => 0,
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut mode, 0, t(self.lang, "file_open"));
            ui.selectable_value(&mut mode, 1, t(self.lang, "file_url"));
            ui.selectable_value(&mut mode, 2, t(self.lang, "file_folder"));
        });
        let mut s = match &self.draft {
            ActionKind::OpenFile { path } | ActionKind::OpenFolder { path } => path.clone(),
            ActionKind::OpenUrl { url } => url.clone(),
            _ => String::new(),
        };
        ui.horizontal(|ui| {
            ui.label(if mode == 1 {
                t(self.lang, "url")
            } else {
                t(self.lang, "path")
            });
            ui.text_edit_singleline(&mut s);
        });
        self.draft = match mode {
            1 => ActionKind::OpenUrl { url: s },
            2 => ActionKind::OpenFolder { path: s },
            _ => ActionKind::OpenFile { path: s },
        };
    }
}
impl AmkApp {
    fn get_icon(&mut self, ctx: &egui::Context, name: &str) -> egui::TextureHandle {
        if let Some(tex) = self.icons.get(name) {
            return tex.clone();
        }
        
        let mapped_name = match name {
            "tb_mouse" => "mouse",
            "tb_key" => "keyboard",
            "tb_eye" | "eye" => "image",
            "smart_click" => "smart_click",
            "tb_win" => "window",
            "tb_folder" => "file",
            "tb_if" => "flow",
            "tb_var" => "var",
            "tb_fn" => "fn",
            "tb_sch" => "schedule",
            "tb_more" => "more",
            "bug" => "bug",
            "grid" => "optimize",
            n => n,
        };

        let img = if let Some(bytes) = crate::icons::get_icon_bytes(mapped_name) {
            let mut pixels = Vec::with_capacity(24 * 24);
            for chunk in bytes.chunks_exact(4) {
                // Apply white with alpha blending
                let a = chunk[3];
                // using pre-multiplied alpha. Since RGB=255, we multiply by alpha
                pixels.push(Color32::from_rgba_premultiplied(a, a, a, a));
            }
            egui::ColorImage {
                size: [24, 24],
                pixels,
            }
        } else {
            egui::ColorImage {
                size: [24, 24],
                pixels: vec![Color32::TRANSPARENT; 24 * 24],
            }
        };

        let tex = ctx.load_texture(name, img, egui::TextureOptions::LINEAR);
        self.icons.insert(name.to_string(), tex.clone());
        tex
    }
}

fn big_tool(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    label: &str,
    color: Color32,
    bg_override: Option<Color32>,
    border_override: Option<Color32>,
) -> egui::Response {
    let font_id = egui::FontId::proportional(11.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font_id, color);
    let width = galley.size().x.max(48.0) + 12.0;

    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 56.0), Sense::click());

    let is_hovered = resp.hovered();

    let mut bg = bg_override;
    let mut border = border_override;

    if is_hovered {
        bg = Some(Color32::from_rgb(230, 240, 255));
        border = Some(Color32::from_rgb(180, 210, 255));
    }

    if let Some(c) = bg {
        ui.painter().rect_filled(rect, 4.0, c);
    }
    if let Some(c) = border {
        ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0_f32, c));
    }

    let center_x = rect.center().x;
    let img_rect = egui::Rect::from_center_size(
        egui::pos2(center_x, rect.min.y + 18.0),
        Vec2::new(24.0, 24.0),
    );
    egui::Image::new(tex).tint(color).paint_at(ui, img_rect);

    ui.painter().galley(
        egui::pos2(center_x - galley.size().x / 2.0, rect.max.y - 18.0),
        galley,
        color,
    );

    resp
}

fn header_cell(ui: &mut egui::Ui, text: &str, w: f32, h: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgb(240, 240, 240));
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, Color32::from_rgb(210, 210, 210)),
    );
    ui.painter().text(
        rect.left_center() + Vec2::new(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(12.0),
        Color32::from_rgb(20, 20, 20),
    );
}
