use crate::capture;
use crate::engine::{Engine, RunState};
use crate::hotkeys::{bindings_from_options, Combo, HotCmd, Hotkeys};
use crate::i18n::{t, Lang};
use crate::model::*;
use crate::record::Recorder;
use crate::vision::{self, RgbImage};
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
    Clipboard,
    If,
    For,
    While,
    Label,
    Goto,
    Message,
    Comment,
    CallFn,
    FnDef,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SmartPhase {
    WaitHide,
    PickRegion,
    PickImages,
}

struct SmartWizard {
    phase: SmartPhase,
    hide_at: Instant,
    shot: Option<RgbImage>,
    /// Absolute screen coordinates of the screenshot's top-left (virtual
    /// screen origin on multi-monitor setups).
    origin: (i32, i32),
    region: Option<(i32, i32, i32, i32)>,
    templates: Vec<(i32, i32, i32, i32)>,
    drag0: Option<egui::Pos2>,
    drag1: Option<egui::Pos2>,
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
    last_sched_check: Instant,
    fired_dates: std::collections::HashMap<String, (i32, u32, u32)>,
    hotkeys: Hotkeys,
    minimize_after_play: bool,
    smart: Option<SmartWizard>,
    smart_tex: Option<egui::TextureHandle>,
    saved_session: AppSession,
    session_wait: Option<Instant>,
    last_title: String,
}

impl AmkApp {
    pub fn new() -> Self {
        let session = AppSession::load(&session_path());
        let lang = Lang::from_code(&session.lang);
        let options = session.options.clone();
        let hotkeys = Hotkeys::start(&options);
        let mut recorder = Recorder::new();
        recorder.warm_hook();
        let mut app = Self {
            icons: std::collections::HashMap::new(),
            status: t(lang, "status_ready").to_string(),
            lang,
            script: Script::default(),
            path: None,
            dirty: false,
            selected: Some(1),
            options,
            engine: Engine::new(),
            recorder,
            last_drain: Instant::now(),
            dialog: Dialog::None,
            edit_index: None,
            draft: ActionKind::Delay { ms: 500 },
            draft_name: String::new(),
            draft_delay: 0,
            clipboard: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            repeat: session.repeat_mode(),
            repeat_n: session.repeat_n.max(1),
            duration_n: session.duration_n.max(1),
            duration_unit: session.duration_unit(),
            show_toolbox: true,
            show_play_opts: true,
            show_status: true,
            hover_tb: None,
            tasks: session.tasks.clone(),
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
            speed_enabled: session.speed_enabled,
            last_sched_check: Instant::now(),
            fired_dates: std::collections::HashMap::new(),
            hotkeys,
            minimize_after_play: false,
            smart: None,
            smart_tex: None,
            saved_session: session,
            session_wait: None,
            last_title: String::new(),
        };
        app.sync_placeholder_comment();
        // Dev affordance: AMK_DIALOG=mouse|keyboard|options|hotkeys|schedule|
        // clicker|presser|log|about|help opens that dialog on launch so the
        // UI can be inspected without driving input.
        if let Ok(d) = std::env::var("AMK_DIALOG") {
            if d == "mouse" {
                app.draft = ActionKind::MouseClick {
                    button: MouseBtn::Left,
                    x: 960,
                    y: 540,
                    clicks: 1,
                };
            }
            if d == "keyboard" {
                app.draft = ActionKind::TypeText {
                    text: "Hello".into(),
                    interval_ms: 20,
                };
            }
            app.dialog = match d.as_str() {
                "mouse" => Dialog::Mouse,
                "keyboard" => Dialog::Keyboard,
                "delay" => Dialog::Delay,
                "options" => Dialog::Options,
                "hotkeys" => Dialog::Hotkeys,
                "schedule" => Dialog::Schedule,
                "clicker" => Dialog::Clicker,
                "presser" => Dialog::Presser,
                "log" => Dialog::Log,
                "about" => Dialog::About,
                "help" => Dialog::Help,
                _ => Dialog::None,
            };
        }
        app
    }

    fn to_session(&self) -> AppSession {
        AppSession {
            lang: self.lang.code().into(),
            options: self.options.clone(),
            tasks: self.tasks.clone(),
            repeat: self.repeat.code().into(),
            repeat_n: self.repeat_n,
            duration_n: self.duration_n,
            duration_unit: self.duration_unit.code().into(),
            speed_enabled: self.speed_enabled,
        }
    }

    fn sync_session(&mut self, force: bool) {
        let snap = self.to_session();
        if snap == self.saved_session {
            self.session_wait = None;
            return;
        }
        if !force {
            let started = self.session_wait.get_or_insert_with(Instant::now);
            if started.elapsed() < Duration::from_millis(500) {
                return;
            }
        }
        if snap.save(&session_path()) {
            self.saved_session = snap;
            self.session_wait = None;
        } else {
            // Retry after the throttle window instead of every frame.
            self.session_wait = Some(Instant::now());
        }
    }

    fn sync_placeholder_comment(&mut self) {
        let en = t(Lang::En, "default_comment");
        let vi = t(Lang::Vi, "default_comment");
        let want = t(self.lang, "default_comment");
        for a in &mut self.script.actions {
            if let ActionKind::Comment { text } = &mut a.kind {
                if text == en || text == vi {
                    *text = want.to_string();
                    a.name = a.kind.default_name();
                }
            }
        }
    }

    fn begin_smart_capture(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        self.smart = Some(SmartWizard {
            phase: SmartPhase::WaitHide,
            hide_at: Instant::now(),
            shot: None,
            origin: (0, 0),
            region: None,
            templates: Vec::new(),
            drag0: None,
            drag1: None,
        });
        self.smart_tex = None;
        self.status = t(self.lang, "smart_hide_hint").to_string();
    }

    /// Returns true when the wizard owns the frame (skip normal UI).
    fn tick_smart_wizard(&mut self, ctx: &egui::Context) -> bool {
        let Some(wiz) = self.smart.as_mut() else {
            return false;
        };
        if wiz.phase == SmartPhase::WaitHide {
            ctx.request_repaint();
            if wiz.hide_at.elapsed() < Duration::from_millis(400) {
                return true;
            }
            let shot = capture::grab_screen();
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            let Some((img, ox, oy)) = shot else {
                self.smart = None;
                self.status = t(self.lang, "smart_grab_fail").to_string();
                return false;
            };
            self.smart_tex = Some(ctx.load_texture(
                "smart_shot",
                img.to_color_image(),
                egui::TextureOptions::LINEAR,
            ));
            if let Some(w) = self.smart.as_mut() {
                w.shot = Some(img);
                w.origin = (ox, oy);
                w.phase = SmartPhase::PickRegion;
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel_smart();
            return false;
        }
        self.draw_smart_overlay(ctx);
        true
    }

    fn cancel_smart(&mut self) {
        self.smart = None;
        self.smart_tex = None;
        self.status = t(self.lang, "status_ready").to_string();
    }

    fn finish_smart(&mut self) {
        let Some(wiz) = self.smart.take() else {
            return;
        };
        self.smart_tex = None;
        let Some(shot) = wiz.shot else {
            self.status = t(self.lang, "smart_grab_fail").to_string();
            return;
        };
        if wiz.templates.is_empty() {
            self.status = t(self.lang, "smart_need_template").to_string();
            return;
        }
        // Template crops stay in image space; the region and prefer point the
        // engine searches with are absolute screen coordinates.
        let region = wiz
            .region
            .map(|(x, y, w, h)| (x + wiz.origin.0, y + wiz.origin.1, w, h))
            .unwrap_or_else(|| {
                let (fx, fy) = wiz
                    .templates
                    .first()
                    .map(|&(x, y, _, _)| (x + wiz.origin.0, y + wiz.origin.1))
                    .unwrap_or((wiz.origin.0, wiz.origin.1));
                (fx, fy, shot.w, shot.h)
            });
        let dir = captures_dir();
        let _ = std::fs::create_dir_all(&dir);
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let mut kinds = Vec::new();
        for (i, &(x, y, w, h)) in wiz.templates.iter().enumerate() {
            let Some(crop) = shot.crop(x, y, w, h) else {
                continue;
            };
            let path = dir.join(format!("smart_{ts}_{}.bmp", i + 1));
            let path_s = path.to_string_lossy().into_owned();
            if !vision::save_bmp24(&path_s, &crop) {
                continue;
            }
            let mut k = ActionKind::smart_click(region.0, region.1, path_s, 5000);
            if let ActionKind::SmartClick {
                rw,
                rh,
                ox,
                oy,
                px,
                py,
                ..
            } = &mut k
            {
                *rw = Some(region.2);
                *rh = Some(region.3);
                *ox = Some(crop.w / 2);
                *oy = Some(crop.h / 2);
                *px = Some(x + wiz.origin.0);
                *py = Some(y + wiz.origin.1);
            }
            kinds.push(k);
        }
        if kinds.is_empty() {
            self.status = t(self.lang, "smart_grab_fail").to_string();
            return;
        }
        self.insert_block(kinds);
        self.status = t(self.lang, "smart_ready_play").to_string();
        self.play();
    }

    fn draw_smart_overlay(&mut self, ctx: &egui::Context) {
        let hint = match self.smart.as_ref().map(|w| w.phase) {
            Some(SmartPhase::PickRegion) => t(self.lang, "smart_hint_region"),
            Some(SmartPhase::PickImages) => t(self.lang, "smart_hint_images"),
            _ => "",
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(20, 20, 20)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(hint)
                            .color(Color32::WHITE)
                            .size(18.0)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t(self.lang, "cancel")).clicked() {
                            self.cancel_smart();
                        }
                        let can_finish = self
                            .smart
                            .as_ref()
                            .map(|w| !w.templates.is_empty())
                            .unwrap_or(false);
                        if ui
                            .add_enabled(
                                can_finish,
                                egui::Button::new(t(self.lang, "smart_finish")),
                            )
                            .clicked()
                        {
                            self.finish_smart();
                        }
                    });
                });
                ui.add_space(8.0);
                let Some(tex) = self.smart_tex.clone() else {
                    return;
                };
                let (img_w_i, img_h_i) = match self.smart.as_ref().and_then(|w| w.shot.as_ref()) {
                    Some(s) => (s.w, s.h),
                    None => return,
                };
                let img_w = img_w_i as f32;
                let img_h = img_h_i as f32;
                let avail = ui.available_size();
                let scale = (avail.x / img_w).min(avail.y / img_h).min(1.0);
                let disp = Vec2::new(img_w * scale, img_h * scale);
                let (rect, resp) = ui.allocate_exact_size(disp, Sense::drag());
                ui.painter().image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );

                let to_img = |p: egui::Pos2| -> (i32, i32) {
                    let x = ((p.x - rect.min.x) / scale).round() as i32;
                    let y = ((p.y - rect.min.y) / scale).round() as i32;
                    (x.clamp(0, img_w_i - 1), y.clamp(0, img_h_i - 1))
                };
                let to_screen = |x: i32, y: i32| -> egui::Pos2 {
                    egui::pos2(rect.min.x + x as f32 * scale, rect.min.y + y as f32 * scale)
                };

                if let Some(wiz) = self.smart.as_ref() {
                    if let Some((x, y, w, h)) = wiz.region {
                        let r = egui::Rect::from_min_max(to_screen(x, y), to_screen(x + w, y + h));
                        ui.painter().rect_stroke(
                            r,
                            0.0,
                            Stroke::new(2.0_f32, Color32::from_rgb(50, 200, 80)),
                        );
                    }
                    for (i, &(x, y, w, h)) in wiz.templates.iter().enumerate() {
                        let r = egui::Rect::from_min_max(to_screen(x, y), to_screen(x + w, y + h));
                        ui.painter().rect_stroke(
                            r,
                            0.0,
                            Stroke::new(2.0_f32, Color32::from_rgb(40, 140, 255)),
                        );
                        ui.painter().text(
                            r.min + Vec2::new(4.0, 4.0),
                            egui::Align2::LEFT_TOP,
                            format!("{}", i + 1),
                            egui::FontId::proportional(14.0),
                            Color32::from_rgb(40, 140, 255),
                        );
                    }
                }

                if resp.drag_started() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        if let Some(w) = self.smart.as_mut() {
                            w.drag0 = Some(p);
                            w.drag1 = Some(p);
                        }
                    }
                }
                if resp.dragged() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        if let Some(w) = self.smart.as_mut() {
                            w.drag1 = Some(p);
                        }
                    }
                }
                if let Some(w) = self.smart.as_ref() {
                    if let (Some(a), Some(b)) = (w.drag0, w.drag1) {
                        ui.painter().rect_stroke(
                            egui::Rect::from_two_pos(a, b),
                            0.0,
                            Stroke::new(2.0_f32, Color32::YELLOW),
                        );
                    }
                }
                if ctx.input(|i| i.pointer.any_released())
                    && self
                        .smart
                        .as_ref()
                        .map(|w| w.drag0.is_some())
                        .unwrap_or(false)
                {
                    let (a, b, phase) = {
                        let w = self.smart.as_ref();
                        (
                            w.and_then(|w| w.drag0),
                            w.and_then(|w| w.drag1),
                            w.map(|w| w.phase),
                        )
                    };
                    if let (Some(a), Some(b), Some(phase)) = (a, b, phase) {
                        let (x0, y0) = to_img(a);
                        let (x1, y1) = to_img(b);
                        if let Some(r) = norm_rect(x0, y0, x1, y1) {
                            if let Some(w) = self.smart.as_mut() {
                                match phase {
                                    SmartPhase::PickRegion => {
                                        w.region = Some(r);
                                        w.phase = SmartPhase::PickImages;
                                    }
                                    SmartPhase::PickImages => w.templates.push(r),
                                    SmartPhase::WaitHide => {}
                                }
                                w.drag0 = None;
                                w.drag1 = None;
                            }
                        }
                    }
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space))
                    && self
                        .smart
                        .as_ref()
                        .map(|w| !w.templates.is_empty())
                        .unwrap_or(false)
                {
                    self.finish_smart();
                }
            });
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
            self.clamp_selection();
            self.dirty = true;
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.script.clone());
            self.script = next;
            self.clamp_selection();
            self.dirty = true;
        }
    }

    /// A restored snapshot may be shorter than the current selection.
    fn clamp_selection(&mut self) {
        self.edit_index = None;
        if let Some(i) = self.selected {
            let last = self.script.actions.len().saturating_sub(1);
            self.selected = Some(i.min(last));
        }
    }

    /// Insert the clipboard action whole: name and delay survive the clipboard.
    fn paste_clipboard(&mut self) {
        let Some(a) = self.clipboard.first().cloned() else {
            return;
        };
        let mut b = a;
        b.id = uuid::Uuid::new_v4().to_string();
        self.snapshot();
        let idx = self
            .selected
            .map(|i| i + 1)
            .unwrap_or(self.script.actions.len());
        let at = idx.min(self.script.actions.len());
        self.script.actions.insert(at, b);
        self.selected = Some(at.min(self.script.actions.len().saturating_sub(1)));
        self.dirty = true;
    }

    fn insert_kind(&mut self, kind: ActionKind) {
        self.insert_block(vec![kind]);
    }

    fn insert_block(&mut self, kinds: Vec<ActionKind>) {
        if kinds.is_empty() {
            return;
        }
        self.snapshot();
        let mut idx = self
            .selected
            .map(|i| (i + 1).min(self.script.actions.len()))
            .unwrap_or(self.script.actions.len());
        if idx == 0 {
            idx = 1.min(self.script.actions.len());
        }
        let start = idx;
        for kind in kinds {
            let mut a = Action::new(kind);
            a.name = a.kind.default_name();
            idx = idx.min(self.script.actions.len());
            self.script.actions.insert(idx, a);
            idx += 1;
        }
        self.selected = Some(start.min(self.script.actions.len().saturating_sub(1)));
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
            ActionKind::KeyDown { .. } | ActionKind::KeyUp { .. } => Dialog::Keyboard,
            ActionKind::Delay { .. } => Dialog::Delay,
            ActionKind::SmartClick { .. } => Dialog::Smart,
            ActionKind::SearchPicture { .. } => Dialog::Search,
            ActionKind::ActivateWindow { .. }
            | ActionKind::CloseWindow { .. }
            | ActionKind::WaitWindow { .. } => Dialog::Window,
            ActionKind::SetClipboard { .. } | ActionKind::GetClipboard { .. } => Dialog::Clipboard,
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
            ActionKind::FunctionEntry => Dialog::FnDef,
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
                match &self.script.actions[i].kind {
                    ActionKind::FunctionEntry if i == 0 => return,
                    ActionKind::EndFunction => {
                        let ends = self
                            .script
                            .actions
                            .iter()
                            .filter(|a| matches!(a.kind, ActionKind::EndFunction))
                            .count();
                        if ends <= 1 {
                            return;
                        }
                    }
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
                    self.status = t(self.lang, "saved").to_string();
                } else {
                    self.status = t(self.lang, "save_failed").to_string();
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
        self.recorder.set_skip_hotkeys(&self.options);
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
        let len = self.script.actions.len();
        let idx = self
            .selected
            .map(|i| i + 1)
            .unwrap_or(len.saturating_sub(1))
            .min(len);
        let idx = if len == 0 { 0 } else { idx.max(1) };
        let mut inserted = 0usize;
        for a in rec.drain(..) {
            if crate::record::insert_recorded(&mut self.script.actions, idx + inserted, a) {
                inserted += 1;
            }
        }
        self.selected = Some(
            (idx + inserted.saturating_sub(1)).min(self.script.actions.len().saturating_sub(1)),
        );
        self.dirty = true;
        self.status = t(self.lang, "status_ready").to_string();
    }

    fn play(&mut self) {
        self.start_play(false);
    }

    fn play_debug(&mut self) {
        self.start_play(true);
    }

    fn start_play(&mut self, start_paused: bool) {
        self.start_play_with(start_paused, None);
    }

    fn start_play_with(&mut self, start_paused: bool, speed_override: Option<f32>) {
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
        let speed = speed_override.unwrap_or(if self.speed_enabled {
            self.options.play_speed
        } else {
            1.0
        });
        self.engine
            .play(self.script.clone(), speed, times, dur, dir, start_paused);
        if !start_paused && self.options.minimize_on_play {
            self.minimize_after_play = true;
        }
        if start_paused {
            self.status = t(self.lang, "status_debug").to_string();
        }
    }

    fn apply_hot_cmd(&mut self, cmd: HotCmd) {
        match cmd {
            HotCmd::PlayOrPause => {
                if self.recorder.is_running() {
                    self.status = t(self.lang, "status_recording").to_string();
                    return;
                }
                match self.engine.snapshot_state() {
                    RunState::Idle => self.play(),
                    _ => self.engine.toggle_pause(),
                }
            }
            HotCmd::Stop => {
                if self.recorder.is_running() {
                    self.stop_record();
                }
                self.engine.request_stop();
            }
            HotCmd::ToggleRecord => {
                if self.engine.snapshot_state() != RunState::Idle {
                    return;
                }
                if self.recorder.is_running() {
                    self.stop_record();
                } else {
                    self.start_record();
                }
            }
            HotCmd::Pause => {
                self.engine.toggle_pause();
            }
            HotCmd::StepInto => {
                if self.engine.snapshot_state() == RunState::Idle {
                    self.play_debug();
                }
                self.engine.request_step_into();
            }
            HotCmd::StepOver => {
                if self.engine.snapshot_state() == RunState::Idle {
                    self.play_debug();
                }
                self.engine.request_step_over();
            }
        }
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
                s.actions.push(Action::new(ActionKind::MouseMove {
                    x: 960,
                    y: 540,
                    ms: 0,
                }));
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
        self.path = None; // Ctrl+S must not overwrite the previously opened file
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
        self.status = format!(
            "Launcher (.cmd) written: {}  (needs automatic-mouse-keyboard.exe on PATH)",
            bat.display()
        );
    }

    fn poll_schedule(&mut self, ctx: &egui::Context) {
        if self.tasks.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        if self.last_sched_check.elapsed() < Duration::from_millis(400) {
            return;
        }
        self.last_sched_check = Instant::now();
        if self.engine.snapshot_state() != RunState::Idle || self.recorder.is_running() {
            return;
        }
        use chrono::{Datelike, Timelike};
        let now = chrono::Local::now();
        let hour = now.hour();
        let minute = now.minute();
        let today = (now.year(), now.month(), now.day());
        let tasks = self.tasks.clone();
        for task in tasks {
            let key = format!("{}|{}|{}", task.name, task.when, task.script);
            let already = self.fired_dates.get(&key).copied() == Some(today);
            if !crate::model::scheduled_task_due(&task.when, hour, minute, already) {
                continue;
            }
            self.fired_dates.insert(key, today);
            if task.script.trim().is_empty() {
                self.status = t(self.lang, "scheduled")
                    .replace("{}", &task.name)
                    .to_string();
                self.play();
                return;
            }
            let path = PathBuf::from(&task.script);
            match std::fs::read_to_string(&path) {
                Ok(s) => match Script::load_json(&s) {
                    Ok(sc) => {
                        let dir = path.parent().map(|p| p.to_path_buf());
                        self.status = t(self.lang, "scheduled")
                            .replace("{}", &task.name)
                            .to_string();
                        let speed = if self.speed_enabled {
                            self.options.play_speed
                        } else {
                            1.0
                        };
                        self.engine.play(sc, speed, 1, None, dir, false);
                    }
                    Err(e) => self.status = e,
                },
                Err(e) => self.status = e.to_string(),
            }
            return;
        }
    }
}

impl eframe::App for AmkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let title = self.title();
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        if self.minimize_after_play {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            self.minimize_after_play = false;
        }

        if self.tick_smart_wizard(ctx) {
            return;
        }

        if self.recorder.is_running() && self.last_drain.elapsed() > Duration::from_millis(200) {
            let extra = self.recorder.drain();
            if !extra.is_empty() {
                let len = self.script.actions.len();
                let idx = self.selected.map(|i| i + 1).unwrap_or(len).min(len);
                let idx = if len == 0 { 0 } else { idx.max(1) };
                let mut inserted = 0usize;
                for a in extra {
                    if crate::record::insert_recorded(&mut self.script.actions, idx + inserted, a) {
                        inserted += 1;
                    }
                }
                if inserted > 0 {
                    self.selected =
                        Some((idx + inserted - 1).min(self.script.actions.len().saturating_sub(1)));
                    self.dirty = true;
                }
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

        self.poll_schedule(ctx);
        ctx.request_repaint_after(Duration::from_millis(50));
        self.hotkeys(ctx);
        self.menu_bar(ctx);
        self.toolbar(ctx);

        if self.show_status {
            egui::TopBottomPanel::bottom("status")
                .frame(
                    egui::Frame::none()
                        .fill(Color32::WHITE)
                        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(226, 230, 235))),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let (color, text) = if self.status.contains("Ready")
                            || self.status.contains("Sẵn")
                        {
                            (Color32::from_rgb(47, 111, 237), &self.status)
                        } else if self.status.contains("Play") || self.status.contains("Chạy") {
                            (Color32::from_rgb(22, 140, 78), &self.status)
                        } else if self.status.contains("Pause")
                            || self.status.contains("Tạm")
                            || self.status.contains("tạm")
                        {
                            (Color32::from_rgb(176, 122, 16), &self.status)
                        } else if self.recorder.is_running() {
                            (Color32::from_rgb(196, 48, 48), &self.status)
                        } else {
                            (Color32::from_rgb(92, 107, 122), &self.status)
                        };

                        let (dot, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        ui.painter().circle_filled(dot.center(), 3.5_f32, color);
                        ui.label(
                            RichText::new(text.as_str())
                                .size(12.0)
                                .color(Color32::from_rgb(28, 36, 48)),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(8.0);
                            let total = self.script.actions.len();
                            let current = self.selected.map(|i| i + 1).unwrap_or(0);
                            ui.label(
                                RichText::new(tf(self.lang, "line_of", current, total))
                                    .size(11.0)
                                    .color(Color32::from_rgb(60, 60, 60)),
                            );
                        });
                    });
                });
        }

        let screen_w = ctx.available_rect().width();
        let toolbox_max = 200.0_f32;
        let play_max = (screen_w - 120.0 - 280.0).clamp(180.0, 300.0);
        if self.show_toolbox {
            egui::SidePanel::left("toolbox_panel")
                .resizable(true)
                .min_width(120.0)
                .default_width(164.0)
                .max_width(toolbox_max)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::WHITE)
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(226, 230, 235))),
                )
                .show_separator_line(true)
                .show(ctx, |ui| {
                    self.toolbox(ui);
                });
        }

        if self.show_play_opts {
            egui::SidePanel::right("play_opts_panel")
                .resizable(true)
                .min_width(180.0)
                .default_width(256.0)
                .max_width(play_max)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::WHITE)
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(226, 230, 235))),
                )
                .show(ctx, |ui| {
                    self.play_options(ui);
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::WHITE))
            .show(ctx, |ui| {
                self.action_table(ui);
            });

        self.draw_dialogs(ctx);
        self.file_path_windows(ctx);
        self.sync_session(false);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.sync_session(true);
    }
}

impl AmkApp {
    fn hotkeys(&mut self, ctx: &egui::Context) {
        if self.dialog != Dialog::Hotkeys {
            for cmd in self.hotkeys.poll() {
                self.apply_hot_cmd(cmd);
            }
        }
        let typing = ctx.wants_keyboard_input();
        let (ctrl, shift, alt, z, y, s, o, n, del, c, x, v, pressed) = ctx.input(|i| {
            (
                i.modifiers.ctrl,
                i.modifiers.shift,
                i.modifiers.alt,
                i.key_pressed(egui::Key::Z),
                i.key_pressed(egui::Key::Y),
                i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::O),
                i.key_pressed(egui::Key::N),
                i.key_pressed(egui::Key::Delete),
                i.key_pressed(egui::Key::C),
                i.key_pressed(egui::Key::X),
                i.key_pressed(egui::Key::V),
                egui_pressed_key(i),
            )
        });
        if typing {
            return;
        }
        if self.dialog != Dialog::Hotkeys {
            if let Some(key) = pressed {
                for (cmd, spec) in bindings_from_options(&self.options) {
                    if self.hotkeys.is_global(cmd) {
                        continue;
                    }
                    if let Some(combo) = Combo::parse(&spec) {
                        if combo.matches(ctrl, shift, alt, key) {
                            self.apply_hot_cmd(cmd);
                        }
                    }
                }
            }
        }
        // Edit shortcuts act on the script behind the dialog; with a dialog
        // open (egui windows are non-modal) they must not fire — a stale
        // edit_index would otherwise commit into the wrong row.
        if self.dialog != Dialog::None {
            return;
        }
        if ctrl && z {
            self.undo();
        }
        if ctrl && y {
            self.redo();
        }
        if ctrl && s && !shift {
            self.save();
        }
        if ctrl && o {
            self.open_file();
        }
        if ctrl && n {
            self.new_script();
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
            self.paste_clipboard();
        }
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu")
            .frame(
                egui::Frame::none()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(8.0, 2.0)),
            )
            .show(ctx, |ui| {
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
                            self.paste_clipboard();
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
                            self.dirty = true;
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "disable")).clicked() {
                            if let Some(i) = self.selected {
                                if let Some(a) = self.script.actions.get_mut(i) {
                                    a.enabled = false;
                                }
                            }
                            self.dirty = true;
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
                            self.begin_smart_capture(ctx);
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
                            self.insert_block(vec![
                                ActionKind::If {
                                    expr: "true".into(),
                                },
                                ActionKind::Else,
                                ActionKind::EndIf,
                            ]);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_for")).clicked() {
                            self.insert_block(vec![
                                ActionKind::For {
                                    var: "i".into(),
                                    from: 1,
                                    to: 10,
                                    step: 1,
                                },
                                ActionKind::EndFor,
                            ]);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_while")).clicked() {
                            self.insert_block(vec![
                                ActionKind::While {
                                    expr: "true".into(),
                                },
                                ActionKind::EndWhile,
                            ]);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_break")).clicked() {
                            self.insert_kind(ActionKind::Break);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_continue")).clicked() {
                            self.insert_kind(ActionKind::Continue);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_wait_win")).clicked() {
                            self.open_edit(
                                ActionKind::WaitWindow {
                                    title: String::new(),
                                    timeout_ms: 5000,
                                    on_fail: "stop".into(),
                                },
                                None,
                            );
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_clip")).clicked() {
                            self.open_edit(
                                ActionKind::SetClipboard {
                                    text: String::new(),
                                },
                                None,
                            );
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_label")).clicked() {
                            self.open_edit(ActionKind::Label { name: "L1".into() }, None);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_goto")).clicked() {
                            self.open_edit(ActionKind::Goto { name: "L1".into() }, None);
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
                            self.open_edit(ActionKind::CallFunction { name: "fn1".into() }, None);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_fn_def")).clicked() {
                            self.snapshot();
                            let mut entry = Action::new(ActionKind::FunctionEntry);
                            entry.name = "fn1".into();
                            self.script.actions.push(entry);
                            self.script
                                .actions
                                .push(Action::new(ActionKind::EndFunction));
                            let idx = self.script.actions.len() - 2;
                            self.selected = Some(idx);
                            self.open_edit(ActionKind::FunctionEntry, Some(idx));
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "insert_play_script")).clicked() {
                            self.open_edit(
                                ActionKind::PlayScript {
                                    path: String::new(),
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
                            self.play_debug();
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
                            self.sync_placeholder_comment();
                            ui.close_menu();
                        }
                        if ui
                            .selectable_label(self.lang == Lang::Vi, "Tiếng Việt")
                            .clicked()
                        {
                            self.lang = Lang::Vi;
                            self.sync_placeholder_comment();
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
                        if ui.button(t(self.lang, "sample_click")).clicked() {
                            self.load_sample(0);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "sample_type")).clicked() {
                            self.load_sample(1);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "sample_loop")).clicked() {
                            self.load_sample(2);
                            ui.close_menu();
                        }
                        if ui.button(t(self.lang, "sample_smart")).clicked() {
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
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(226, 230, 235))),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::horizontal()
                    .id_source("toolbar_scroll")
                    .scroll_bar_visibility(
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .show(ui, |ui| {
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
                                format!("{} ({})", t(self.lang, "stop"), self.options.hk_record)
                            } else {
                                format!("{} ({})", t(self.lang, "record"), self.options.hk_record)
                            };
                            let rec_color = if rec {
                                Color32::from_rgb(180, 30, 30)
                            } else {
                                Color32::from_rgb(200, 40, 40)
                            };
                            let tex_record = self.get_icon(ctx, "record");
                            if big_tool(ui, &tex_record, &rec_label, rec_color, None, None)
                                .clicked()
                            {
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
                                self.begin_smart_capture(ctx);
                            }

                            let playing = self.engine.snapshot_state() != RunState::Idle;
                            let play_label = if playing {
                                format!("{} ({})", t(self.lang, "stop"), self.options.hk_stop)
                            } else {
                                format!("{} ({})", t(self.lang, "play"), self.options.hk_play)
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
                                &play_label,
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
                                // Slow-motion for this run only: the user's
                                // saved play speed must stay untouched.
                                self.start_play_with(false, Some(0.4));
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
                            if big_tool(
                                ui,
                                &tex_up,
                                t(self.lang, "move_up"),
                                default_color,
                                None,
                                None,
                            )
                            .clicked()
                            {
                                self.move_sel(-1);
                            }

                            let tex_down = self.get_icon(ctx, "down");
                            if big_tool(
                                ui,
                                &tex_down,
                                t(self.lang, "move_down"),
                                default_color,
                                None,
                                None,
                            )
                            .clicked()
                            {
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

                            ui.add_space(8.0);
                            let lab = self.lang.label().to_string();
                            if ui
                                .add(
                                    egui::Button::new(RichText::new(lab).strong())
                                        .min_size(Vec2::new(44.0, 28.0)),
                                )
                                .clicked()
                            {
                                self.lang = self.lang.toggle();
                                self.sync_placeholder_comment();
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
        let frame = egui::Frame::none().fill(Color32::WHITE).inner_margin(0.0);

        frame.show(ui, |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), ui.available_height()),
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
                        egui::Image::new(&tex_wrench)
                            .tint(Color32::from_rgb(60, 80, 100))
                            .paint_at(ui, img_rect);
                        ui.add_space(20.0);
                        ui.label(
                            RichText::new(t(self.lang, "tb_title"))
                                .strong()
                                .color(Color32::from_rgb(60, 80, 100))
                                .size(13.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(12.0);
                        });
                    });
                    ui.add_space(12.0);

                    let stroke = Stroke::new(1.0_f32, Color32::from_rgb(220, 225, 230));
                    let (rect, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                    ui.painter()
                        .line_segment([rect.left_top(), rect.right_top()], stroke);
                    ui.add_space(12.0);

                    let groups = [
                        (
                            "tb_group_input",
                            [
                                ("tb_mouse", 0, ToolboxId::Mouse, "tb_mouse"),
                                ("tb_keyboard", 1, ToolboxId::Keyboard, "tb_key"),
                                ("tb_image", 2, ToolboxId::Image, "tb_eye"),
                            ]
                            .as_slice(),
                        ),
                        (
                            "tb_group_flow",
                            [
                                ("tb_flow", 5, ToolboxId::Flow, "tb_if"),
                                ("tb_var", 6, ToolboxId::Var, "tb_var"),
                                ("tb_fn", 7, ToolboxId::Fn, "tb_fn"),
                            ]
                            .as_slice(),
                        ),
                        (
                            "tb_group_system",
                            [
                                ("tb_window", 3, ToolboxId::Window, "tb_win"),
                                ("tb_file", 4, ToolboxId::File, "tb_folder"),
                                ("tb_schedule", 9, ToolboxId::Schedule, "tb_sch"),
                                ("tb_more", 8, ToolboxId::More, "tb_more"),
                            ]
                            .as_slice(),
                        ),
                    ];

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (i, (group_title, items)) in groups.into_iter().enumerate() {
                            if i > 0 {
                                ui.add_space(16.0);
                            }

                            ui.horizontal(|ui| {
                                ui.add_space(16.0);
                                ui.label(
                                    RichText::new(t(self.lang, group_title))
                                        .size(11.0)
                                        .strong()
                                        .color(Color32::from_rgb(130, 140, 150)),
                                );
                            });
                            ui.add_space(6.0);

                            for &(key, id, kind, icon_name) in items {
                                let selected = self.hover_tb == Some(id);
                                let tip = t(self.lang, key);
                                let caption =
                                    tip.split(['/', '&', '(']).next().unwrap_or(tip).trim();

                                let (rect, resp) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 36.0),
                                    Sense::click(),
                                );

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
                                let img_color = if selected {
                                    Color32::from_rgb(0, 80, 180)
                                } else {
                                    Color32::from_rgb(60, 70, 80)
                                };
                                egui::Image::new(&tex)
                                    .tint(img_color)
                                    .paint_at(ui, img_rect);

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
                                        ToolboxId::Image => self.begin_smart_capture(ui.ctx()),
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
                                            self.insert_block(vec![
                                                ActionKind::If {
                                                    expr: "true".into(),
                                                },
                                                ActionKind::Else,
                                                ActionKind::EndIf,
                                            ]);
                                        }
                                        ToolboxId::Var => self.open_edit(
                                            ActionKind::SetVar {
                                                name: "v".into(),
                                                value: "0".into(),
                                            },
                                            None,
                                        ),
                                        ToolboxId::Fn => self.open_edit(
                                            ActionKind::CallFunction { name: "fn1".into() },
                                            None,
                                        ),
                                        ToolboxId::More => self.dialog = Dialog::Options,
                                        ToolboxId::Schedule => self.dialog = Dialog::Schedule,
                                    }
                                }
                            }
                        }
                    });
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

        let header_h = 34.0;
        let row_h = 38.0;
        let view_w = ui.available_width().max(1.0);
        let compact = view_w < 560.0;
        let w_step = if compact { 44.0 } else { 52.0 };
        let w_delay = if compact { 0.0 } else { 72.0 };
        let w_target = if compact {
            0.0
        } else {
            (view_w - w_step - w_delay) * 0.28
        };
        let rest = (view_w - w_step - w_delay - w_target).max(1.0);
        let w_action = if compact { rest * 0.42 } else { rest * 0.38 };
        let w_details = rest - w_action;

        let mut edit_i = None;
        let mut sel_i = None;
        let mut del_i = None;
        let mut toggle_i = None;

        // One pass: indent_of() per row would make each frame O(n²) on long
        // recordings.
        let indents: Vec<i32> = {
            let mut v = Vec::with_capacity(self.script.actions.len());
            let mut indent = 0i32;
            for a in &self.script.actions {
                v.push(
                    if a.kind.is_structure_end() || matches!(a.kind, ActionKind::Else) {
                        (indent - 1).max(0)
                    } else {
                        indent.max(0)
                    },
                );
                indent = (indent + a.kind.indent_delta()).max(0);
            }
            v
        };

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            header_cell(ui, t(self.lang, "col_id"), w_step, header_h);
            header_cell(ui, t(self.lang, "col_action"), w_action, header_h);
            if w_target > 0.0 {
                header_cell(ui, t(self.lang, "col_target"), w_target, header_h);
            }
            header_cell(ui, t(self.lang, "col_details"), w_details, header_h);
            if w_delay > 0.0 {
                header_cell(ui, t(self.lang, "col_wait"), w_delay, header_h);
            }
        });

        egui::ScrollArea::vertical()
            .id_source("action_table_scroll")
            .auto_shrink([false, false])
            .max_height((ui.available_height() - 4.0).max(0.0))
            // Uniform row height: render only the visible slice so long
            // recordings cost O(screen) per frame, not O(script).
            .show_rows(ui, row_h, self.script.actions.len(), |ui, row_range| {
                ui.set_width(view_w);
                ui.spacing_mut().item_spacing.y = 0.0;
                for i in row_range {
                    let indent = indents[i];
                    let selected = self.selected == Some(i);
                    let playing = playing_idx == Some(i);

                    let icon_name = match self.script.actions[i].kind {
                        ActionKind::MouseClick { .. } | ActionKind::MouseMove { .. } => "tb_mouse",
                        ActionKind::TypeText { .. } | ActionKind::KeyPress { .. } => "tb_key",
                        ActionKind::KeyDown { .. } | ActionKind::KeyUp { .. } => "tb_key",
                        ActionKind::Delay { .. } => "tb_sch",
                        ActionKind::SmartClick { .. } | ActionKind::SearchPicture { .. } => {
                            "tb_eye"
                        }
                        ActionKind::If { .. }
                        | ActionKind::For { .. }
                        | ActionKind::While { .. }
                        | ActionKind::Break
                        | ActionKind::Continue => "tb_if",
                        ActionKind::SetVar { .. }
                        | ActionKind::SetClipboard { .. }
                        | ActionKind::GetClipboard { .. } => "tb_var",
                        ActionKind::FunctionEntry | ActionKind::CallFunction { .. } => "tb_fn",
                        ActionKind::ActivateWindow { .. }
                        | ActionKind::CloseWindow { .. }
                        | ActionKind::WaitWindow { .. } => "tb_win",
                        ActionKind::OpenFile { .. } => "tb_folder",
                        _ => "tb_more",
                    };
                    let tex = self.get_icon(ui.ctx(), icon_name);

                    let a = &self.script.actions[i];

                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(w_step + w_action + w_target + w_details + w_delay, row_h),
                        Sense::click(),
                    );
                    let bg = if selected {
                        Color32::from_rgb(232, 241, 251)
                    } else if playing {
                        Color32::from_rgb(255, 248, 232)
                    } else if resp.hovered() {
                        Color32::from_rgb(239, 243, 248)
                    } else if i % 2 == 0 {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(245, 247, 250)
                    };
                    ui.painter().rect_filled(rect, 0.0, bg);
                    if selected || playing {
                        let accent = if selected {
                            Color32::from_rgb(47, 111, 237)
                        } else {
                            Color32::from_rgb(196, 140, 24)
                        };
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(
                                rect.left_top(),
                                egui::pos2(rect.left() + 3.0, rect.bottom()),
                            ),
                            0.0,
                            accent,
                        );
                    }

                    let (name_color, delay_color, step_color) = if !a.enabled {
                        (
                            Color32::from_rgb(150, 158, 168),
                            Color32::from_rgb(150, 158, 168),
                            Color32::from_rgb(150, 158, 168),
                        )
                    } else {
                        let nc = match &a.kind {
                            ActionKind::FunctionEntry | ActionKind::EndFunction => {
                                Color32::from_rgb(16, 122, 64)
                            }
                            ActionKind::Comment { .. } => Color32::from_rgb(16, 122, 64),
                            ActionKind::If { .. }
                            | ActionKind::Else
                            | ActionKind::EndIf
                            | ActionKind::For { .. }
                            | ActionKind::EndFor
                            | ActionKind::While { .. }
                            | ActionKind::EndWhile
                            | ActionKind::Goto { .. }
                            | ActionKind::Label { .. } => Color32::from_rgb(112, 48, 160),
                            _ => Color32::from_rgb(28, 36, 48),
                        };
                        (
                            nc,
                            Color32::from_rgb(92, 107, 122),
                            Color32::from_rgb(92, 107, 122),
                        )
                    };

                    let (_act, tgt, mut det) = a.kind.format_columns();
                    if matches!(a.kind, ActionKind::FunctionEntry | ActionKind::EndFunction) {
                        det = t(self.lang, "act_entry").to_string();
                        if matches!(a.kind, ActionKind::EndFunction) {
                            det = t(self.lang, "act_function").to_string();
                        }
                    }
                    let full_det = det.clone();
                    det = shorten_detail(&det);
                    let act_str = t(self.lang, a.kind.type_key()).to_string();
                    let pad = 12.0 + indent as f32 * 14.0;

                    let mut x = rect.min.x;
                    let row_line = Stroke::new(1.0_f32, Color32::from_rgb(236, 238, 242));
                    let body = egui::FontId::proportional(13.0);
                    let small = egui::FontId::monospace(12.0);

                    let id_rect = egui::Rect::from_min_size(
                        egui::pos2(x + 10.0, rect.top()),
                        Vec2::new(w_step - 14.0, rect.height()),
                    );
                    paint_ellipsis(
                        ui,
                        id_rect,
                        &format!("{:03}", i + 1),
                        small.clone(),
                        step_color,
                    );
                    x += w_step;

                    let img_rect = egui::Rect::from_center_size(
                        egui::pos2(x + pad + 2.0, rect.center().y),
                        Vec2::new(16.0, 16.0),
                    );
                    egui::Image::new(&tex)
                        .tint(name_color)
                        .paint_at(ui, img_rect);
                    let act_rect = egui::Rect::from_min_size(
                        egui::pos2(x + pad + 16.0, rect.top()),
                        Vec2::new((w_action - pad - 20.0).max(8.0), rect.height()),
                    );
                    paint_ellipsis(ui, act_rect, &act_str, body.clone(), name_color);
                    x += w_action;

                    if w_target > 0.0 {
                        let tgt_rect = egui::Rect::from_min_size(
                            egui::pos2(x + 8.0, rect.top()),
                            Vec2::new((w_target - 16.0).max(0.0), rect.height()),
                        );
                        paint_ellipsis(ui, tgt_rect, &tgt, body.clone(), name_color);
                        x += w_target;
                    }

                    let det_rect = egui::Rect::from_min_size(
                        egui::pos2(x + 8.0, rect.top()),
                        Vec2::new((w_details - 16.0).max(0.0), rect.height()),
                    );
                    paint_ellipsis(ui, det_rect, &det, body, name_color);
                    x += w_details;

                    let delay_txt = if w_delay == 0.0 {
                        String::new()
                    } else if a.delay_ms > 0 {
                        format!("{} ms", a.delay_ms)
                    } else if selected {
                        "0 ms".to_string()
                    } else {
                        String::new()
                    };
                    if !delay_txt.is_empty() {
                        let delay_rect = egui::Rect::from_min_size(
                            egui::pos2(x + 8.0, rect.top()),
                            Vec2::new(w_delay - 12.0, rect.height()),
                        );
                        paint_ellipsis(ui, delay_rect, &delay_txt, small, delay_color);
                    }

                    ui.painter()
                        .line_segment([rect.left_bottom(), rect.right_bottom()], row_line);
                    if full_det != det {
                        resp.clone().on_hover_text(full_det);
                    }

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
                            del_i = Some(i);
                            ui.close_menu();
                        }
                    });
                }
            });

        if let Some(i) = del_i {
            self.selected = Some(i);
            self.delete_selected();
        }
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
            self.dirty = true;
        }
    }

    /// A play-options row with a fixed height and vertical centering, so
    /// radios, number boxes, and combo boxes share one optical line even with
    /// their different native widget heights.
    fn option_row(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 28.0),
            egui::Layout::left_to_right(egui::Align::Center),
            add,
        );
    }

    fn play_options(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_source("play_opts_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.play_options_inner(ui);
            });
    }

    fn play_options_inner(&mut self, ui: &mut egui::Ui) {
        // Header
        egui::Frame::none()
            .fill(Color32::from_rgb(248, 249, 250))
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t(self.lang, "play_opts"))
                            .strong()
                            .size(14.0)
                            .color(Color32::from_rgb(28, 36, 48)),
                    );
                });
            });

        let sep_stroke = Stroke::new(1.0_f32, Color32::from_rgb(220, 224, 229));
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter()
            .line_segment([r.left_top(), r.right_top()], sep_stroke);

        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(8.0, 0.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                // Uniform control height: the combo box otherwise grows with
                // the global button padding and every row misaligns.
                ui.spacing_mut().interact_size.y = 24.0;
                ui.spacing_mut().button_padding.y = 4.0;

                // ── Play Repetition ───────────────────────────────────────
                ui.add_space(2.0);
                ui.label(
                    RichText::new(t(self.lang, "play_rep"))
                        .strong()
                        .size(12.0)
                        .color(Color32::from_rgb(50, 65, 80)),
                );
                ui.add_space(4.0);

                Self::option_row(ui, |ui| {
                    ui.radio_value(
                        &mut self.repeat,
                        RepeatMode::Once,
                        t(self.lang, "play_once"),
                    );
                });

                Self::option_row(ui, |ui| {
                    ui.radio_value(
                        &mut self.repeat,
                        RepeatMode::Times,
                        t(self.lang, "play_script_n"),
                    );
                    ui.add_enabled_ui(self.repeat == RepeatMode::Times, |ui| {
                        ui.add_sized(
                            egui::vec2(52.0, 24.0),
                            egui::DragValue::new(&mut self.repeat_n)
                                .clamp_range(1..=1_000_000)
                                .speed(1),
                        );
                    });
                    ui.label(t(self.lang, "times"));
                });

                Self::option_row(ui, |ui| {
                    ui.radio_value(
                        &mut self.repeat,
                        RepeatMode::Duration,
                        t(self.lang, "play_for"),
                    );
                    ui.add_enabled_ui(self.repeat == RepeatMode::Duration, |ui| {
                        ui.add_sized(
                            egui::vec2(52.0, 24.0),
                            egui::DragValue::new(&mut self.duration_n)
                                .clamp_range(1..=10_000)
                                .speed(1),
                        );
                    });
                    egui::ComboBox::from_id_source("dur_unit")
                        .selected_text(match self.duration_unit {
                            DurationUnit::Seconds => t(self.lang, "secs"),
                            DurationUnit::Minutes => t(self.lang, "mins"),
                            DurationUnit::Hours => t(self.lang, "hrs"),
                        })
                        .width(56.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.duration_unit,
                                DurationUnit::Seconds,
                                t(self.lang, "secs"),
                            );
                            ui.selectable_value(
                                &mut self.duration_unit,
                                DurationUnit::Minutes,
                                t(self.lang, "mins"),
                            );
                            ui.selectable_value(
                                &mut self.duration_unit,
                                DurationUnit::Hours,
                                t(self.lang, "hrs"),
                            );
                        });
                });

                Self::option_row(ui, |ui| {
                    ui.radio_value(
                        &mut self.repeat,
                        RepeatMode::Infinite,
                        t(self.lang, "repeat_until_stop"),
                    );
                });

                // separator
                ui.add_space(2.0);
                let (r, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter()
                    .line_segment([r.left_top(), r.right_top()], sep_stroke);

                // ── Execution Options ─────────────────────────────────────
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t(self.lang, "exec_opts"))
                        .strong()
                        .size(12.0)
                        .color(Color32::from_rgb(50, 65, 80)),
                );
                ui.add_space(4.0);

                Self::option_row(ui, |ui| {
                    ui.checkbox(&mut self.speed_enabled, t(self.lang, "speed_short"));
                    ui.add_enabled_ui(self.speed_enabled, |ui| {
                        ui.add_sized(
                            egui::vec2(56.0, 24.0),
                            egui::DragValue::new(&mut self.options.play_speed)
                                .clamp_range(0.1..=10.0)
                                .speed(0.1)
                                .fixed_decimals(1)
                                .suffix("x"),
                        );
                    });
                });
                Self::option_row(ui, |ui| {
                    ui.checkbox(
                        &mut self.options.minimize_on_play,
                        t(self.lang, "minimize_on_start"),
                    );
                });

                // separator
                ui.add_space(4.0);
                let (r, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter()
                    .line_segment([r.left_top(), r.right_top()], sep_stroke);

                // ── Hotkeys ───────────────────────────────────────────────
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t(self.lang, "hk_section"))
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

                // Fixed-width label column: the badges line up as a column
                // instead of ragging left when the labels differ in width.
                let hk_label = |ui: &mut egui::Ui, key: &str| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(104.0, 24.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label(t(self.lang, key));
                        },
                    );
                };
                Self::option_row(ui, |ui| {
                    hk_label(ui, "hk_record_short");
                    draw_key_badge(ui, &self.options.hk_record, false);
                });
                Self::option_row(ui, |ui| {
                    hk_label(ui, "hk_play_short");
                    draw_key_badge(ui, &self.options.hk_play, false);
                });
                Self::option_row(ui, |ui| {
                    hk_label(ui, "hk_stop_short");
                    draw_key_badge(ui, &self.options.hk_stop, true);
                });
                ui.label(
                    RichText::new(t(self.lang, "hk_more"))
                        .size(11.0)
                        .color(Color32::from_rgb(122, 132, 144)),
                );
            });

        ui.add_space(10.0);
        let btn_h = 40.0;
        let width = (ui.available_width() - 20.0).max(40.0);
        let (full, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), btn_h), Sense::click());
        let rect = egui::Rect::from_min_size(
            egui::pos2(full.min.x + 10.0, full.min.y),
            Vec2::new(width, btn_h),
        );
        let bg = if resp.hovered() {
            Color32::from_rgb(36, 99, 214)
        } else {
            Color32::from_rgb(47, 111, 237)
        };
        ui.painter().rect_filled(rect, 8.0, bg);

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

        let play_caption = format!(
            "{} ({})",
            t(self.lang, "start_playback"),
            self.options.hk_play
        );
        let caption_rect = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + 40.0, rect.top()),
            egui::pos2(rect.max.x - 12.0, rect.bottom()),
        );
        paint_ellipsis(
            ui,
            caption_rect,
            &play_caption,
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
            Dialog::Clipboard => t(self.lang, "dlg_clip"),
            Dialog::If => t(self.lang, "insert_if"),
            Dialog::For => t(self.lang, "insert_for"),
            Dialog::While => t(self.lang, "insert_while"),
            Dialog::Label => t(self.lang, "label"),
            Dialog::Goto => "Goto",
            Dialog::Message => t(self.lang, "insert_msg"),
            Dialog::Comment => t(self.lang, "insert_comment"),
            Dialog::CallFn => t(self.lang, "insert_fn"),
            Dialog::FnDef => t(self.lang, "insert_fn_def"),
            Dialog::PlayScript => t(self.lang, "insert_play_script"),
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
            .min_width(380.0)
            .show(ctx, |ui| {
                // egui shrinks windows to content: keep the sparse dialogs
                // (options/schedule/tools) at a dignified width.
                if matches!(
                    self.dialog,
                    Dialog::Options | Dialog::Schedule | Dialog::Clicker | Dialog::Presser
                ) {
                    ui.set_min_width(340.0);
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
                        | Dialog::Clipboard
                ) {
                    ui.horizontal(|ui| {
                        ui.label(t(self.lang, "step_name"));
                        ui.text_edit_singleline(&mut self.draft_name);
                    });
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
                    Dialog::Clipboard => self.ui_clipboard(ui),
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
                            ui.label(t(self.lang, "var_value_hint"));
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
                            ui.label(t(self.lang, "call_fn_hint"));
                        }
                    }
                    Dialog::FnDef => {
                        ui.label(t(self.lang, "fn_def_hint"));
                    }
                    Dialog::PlayScript => {
                        if let ActionKind::PlayScript { path } = &mut self.draft {
                            ui.horizontal(|ui| {
                                ui.text_edit_singleline(path);
                            });
                        }
                    }
                    Dialog::Options => {
                        ui.spacing_mut().interact_size.y = 24.0;
                        Self::option_row(ui, |ui| {
                            ui.checkbox(
                                &mut self.options.minimize_on_play,
                                t(self.lang, "opt_minimize"),
                            );
                        });
                        Self::option_row(ui, |ui| {
                            ui.label(t(self.lang, "opt_sample"));
                            ui.add_sized(
                                egui::vec2(64.0, 24.0),
                                egui::DragValue::new(&mut self.options.sample_ms)
                                    .clamp_range(5..=500),
                            );
                        });
                        Self::option_row(ui, |ui| {
                            ui.label(t(self.lang, "opt_ignore_move"));
                            ui.add_sized(
                                egui::vec2(64.0, 24.0),
                                egui::DragValue::new(&mut self.options.ignore_px)
                                    .clamp_range(0..=50),
                            );
                        });
                        Self::option_row(ui, |ui| {
                            ui.label(t(self.lang, "speed"));
                            ui.add(egui::Slider::new(&mut self.options.play_speed, 0.1..=8.0));
                        });
                    }
                    Dialog::Hotkeys => {
                        ui.label(t(self.lang, "hk_help"));
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "hk_play"));
                            ui.text_edit_singleline(&mut self.options.hk_play);
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "hk_stop"));
                            ui.text_edit_singleline(&mut self.options.hk_stop);
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "hk_record"));
                            ui.text_edit_singleline(&mut self.options.hk_record);
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "hk_pause"));
                            ui.text_edit_singleline(&mut self.options.hk_pause);
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "hk_step_into"));
                            ui.text_edit_singleline(&mut self.options.hk_step_into);
                        });
                        ui.horizontal(|ui| {
                            ui.label(t(self.lang, "hk_step_over"));
                            ui.text_edit_singleline(&mut self.options.hk_step_over);
                        });
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
                        if ui.button(t(self.lang, "clear")).clicked() {
                            self.engine.clear_log();
                        }
                        egui::ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                // Borrow the log in place: cloning up to
                                // LOG_CAP lines every frame is real work.
                                self.engine.with_log(|lines| {
                                    for line in lines {
                                        ui.monospace(format!("{}  {}", line.time, line.text));
                                    }
                                });
                            });
                    }
                    Dialog::Schedule => {
                        if self.tasks.is_empty() {
                            ui.label(
                                RichText::new(t(self.lang, "schedule_empty"))
                                    .color(Color32::from_rgb(110, 120, 132)),
                            );
                        }
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
                        if dialog_primary_button(ui, t(self.lang, "play")).clicked() {
                            self.snapshot();
                            let mut s = Script::default();
                            s.actions.clear();
                            s.actions.push(Action::new(ActionKind::FunctionEntry));
                            s.actions.push(Action::new(ActionKind::For {
                                var: "c".into(),
                                from: 1,
                                // 0 = infinite, as the dialog label promises.
                                to: if self.clicker_count == 0 {
                                    i64::MAX
                                } else {
                                    self.clicker_count as i64
                                },
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
                            self.path = None; // Ctrl+S must not overwrite another file
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
                        if dialog_primary_button(ui, t(self.lang, "play")).clicked() {
                            self.snapshot();
                            let mut s = Script::default();
                            s.actions.clear();
                            s.actions.push(Action::new(ActionKind::FunctionEntry));
                            s.actions.push(Action::new(ActionKind::For {
                                var: "k".into(),
                                from: 1,
                                to: if self.presser_count == 0 {
                                    i64::MAX
                                } else {
                                    self.presser_count as i64
                                },
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
                            self.path = None; // Ctrl+S must not overwrite another file
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
                    // Primary action right-most, styled like the main CTA.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if dialog_primary_button(ui, t(self.lang, "ok")).clicked() {
                            self.commit_draft();
                        }
                        if dialog_secondary_button(ui, t(self.lang, "cancel")).clicked() {
                            self.dialog = Dialog::None;
                        }
                    });
                }
            });

        if !open {
            if self.dialog == Dialog::Hotkeys {
                self.recorder.set_skip_hotkeys(&self.options);
                self.hotkeys.rebind(&self.options);
            }
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
        let (mut x, mut y, mut x2, mut y2, mut btn, mut delta, mut ms) = match &self.draft {
            ActionKind::MouseMove { x, y, ms } => {
                (*x, *y, *x + 10, *y + 10, MouseBtn::Left, 0, *ms)
            }
            ActionKind::MouseClick { x, y, button, .. } => (*x, *y, *x, *y, *button, 0, 0),
            ActionKind::MouseDrag {
                x1,
                y1,
                x2,
                y2,
                button,
                ms,
            } => (*x1, *y1, *x2, *y2, *button, 0, *ms),
            ActionKind::MouseWheel { delta } => (0, 0, 0, 0, MouseBtn::Left, *delta, 0),
            _ => (0, 0, 0, 0, MouseBtn::Left, 0, 0),
        };
        if mode != 4 {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut y));
            });
        }
        if mode == 0 || mode == 3 {
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "move_ms"));
                ui.add(egui::DragValue::new(&mut ms).clamp_range(0..=60_000));
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
            0 => ActionKind::MouseMove { x, y, ms },
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
                ms,
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
        let mut mode = match &self.draft {
            ActionKind::KeyPress { .. } => 1,
            ActionKind::KeyDown { .. } => 2,
            ActionKind::KeyUp { .. } => 3,
            _ => 0,
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut mode, 0, t(self.lang, "type_text"));
            ui.selectable_value(&mut mode, 1, t(self.lang, "key_press"));
            ui.selectable_value(&mut mode, 2, t(self.lang, "key_down"));
            ui.selectable_value(&mut mode, 3, t(self.lang, "key_up"));
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
            let key = match &self.draft {
                ActionKind::KeyPress { key }
                | ActionKind::KeyDown { key }
                | ActionKind::KeyUp { key } => key.clone(),
                _ => "Enter".into(),
            };
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "key"));
                let mut k = key;
                ui.text_edit_singleline(&mut k);
                self.draft = match mode {
                    2 => ActionKind::KeyDown { key: k },
                    3 => ActionKind::KeyUp { key: k },
                    _ => ActionKind::KeyPress { key: k },
                };
            });
            ui.label(t(self.lang, "key_examples"));
            if mode == 2 || mode == 3 {
                ui.label(t(self.lang, "key_hold_hint"));
            }
        }
    }

    fn ui_clipboard(&mut self, ui: &mut egui::Ui) {
        let mut mode = match &self.draft {
            ActionKind::GetClipboard { .. } => 1,
            _ => 0,
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut mode, 0, t(self.lang, "clip_set"));
            ui.selectable_value(&mut mode, 1, t(self.lang, "clip_get"));
        });
        match mode {
            0 => {
                let mut text = match &self.draft {
                    ActionKind::SetClipboard { text } => text.clone(),
                    _ => String::new(),
                };
                ui.horizontal(|ui| {
                    ui.label(t(self.lang, "clip_text"));
                    ui.text_edit_singleline(&mut text);
                });
                self.draft = ActionKind::SetClipboard { text };
            }
            _ => {
                let mut name = match &self.draft {
                    ActionKind::GetClipboard { name } => name.clone(),
                    _ => "clip".into(),
                };
                ui.horizontal(|ui| {
                    ui.label(t(self.lang, "clip_var"));
                    ui.text_edit_singleline(&mut name);
                });
                self.draft = ActionKind::GetClipboard { name };
            }
        }
    }

    fn ui_smart(&mut self, ui: &mut egui::Ui) {
        let mut pick_screen = false;
        if let ActionKind::SmartClick {
            x,
            y,
            image,
            timeout_ms,
            confidence,
            on_fail,
            ox,
            oy,
            ..
        } = &mut self.draft
        {
            if ui.button(t(self.lang, "smart_pick_screen")).clicked() {
                pick_screen = true;
            }
            ui.label(t(self.lang, "smart_need_image"));
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
                ui.label(t(self.lang, "conf"));
                ui.add(egui::Slider::new(confidence, 0.75..=0.99));
            });
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "on_fail"));
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
                ui.label(t(self.lang, "offset_from_image"));
            });
        }
        if pick_screen {
            self.dialog = Dialog::None;
            self.begin_smart_capture(ui.ctx());
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
                ui.label(t(self.lang, "save_x"));
                ui.text_edit_singleline(save_x);
                ui.label(t(self.lang, "save_y"));
                ui.text_edit_singleline(save_y);
            });
        }
    }

    fn ui_window(&mut self, ui: &mut egui::Ui) {
        let mut mode = match &self.draft {
            ActionKind::CloseWindow { .. } => 1,
            ActionKind::WaitWindow { .. } => 2,
            _ => 0,
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut mode, 0, t(self.lang, "win_activate"));
            ui.selectable_value(&mut mode, 1, t(self.lang, "win_close"));
            ui.selectable_value(&mut mode, 2, t(self.lang, "win_wait"));
        });
        let mut title = match &self.draft {
            ActionKind::ActivateWindow { title }
            | ActionKind::CloseWindow { title }
            | ActionKind::WaitWindow { title, .. } => title.clone(),
            _ => String::new(),
        };
        ui.horizontal(|ui| {
            ui.label(t(self.lang, "window_title"));
            ui.text_edit_singleline(&mut title);
        });
        if mode == 2 {
            let (mut timeout_ms, mut on_fail) = match &self.draft {
                ActionKind::WaitWindow {
                    timeout_ms,
                    on_fail,
                    ..
                } => (*timeout_ms, on_fail.clone()),
                _ => (5000u64, "stop".to_string()),
            };
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "timeout"));
                ui.add(egui::DragValue::new(&mut timeout_ms).clamp_range(0..=3_600_000));
            });
            ui.horizontal(|ui| {
                ui.label(t(self.lang, "on_fail"));
                ui.text_edit_singleline(&mut on_fail);
            });
            self.draft = ActionKind::WaitWindow {
                title,
                timeout_ms,
                on_fail,
            };
            return;
        }
        self.draft = if mode == 1 {
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

fn norm_rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Option<(i32, i32, i32, i32)> {
    let x = x0.min(x1);
    let y = y0.min(y1);
    let w = (x0 - x1).abs();
    let h = (y0 - y1).abs();
    if w < 8 || h < 8 {
        None
    } else {
        Some((x, y, w, h))
    }
}

fn captures_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("captures")
}

fn tf(lang: Lang, key: &str, a: impl std::fmt::Display, b: impl std::fmt::Display) -> String {
    t(lang, key)
        .replacen("{}", &a.to_string(), 1)
        .replacen("{}", &b.to_string(), 1)
}

fn egui_pressed_key(i: &egui::InputState) -> Option<&'static str> {
    use egui::Key::*;
    let pairs = [
        (F1, "F1"),
        (F2, "F2"),
        (F3, "F3"),
        (F4, "F4"),
        (F5, "F5"),
        (F6, "F6"),
        (F7, "F7"),
        (F8, "F8"),
        (F9, "F9"),
        (F10, "F10"),
        (F11, "F11"),
        (F12, "F12"),
        (A, "A"),
        (B, "B"),
        (C, "C"),
        (D, "D"),
        (E, "E"),
        (F, "F"),
        (G, "G"),
        (H, "H"),
        (I, "I"),
        (J, "J"),
        (K, "K"),
        (L, "L"),
        (M, "M"),
        (N, "N"),
        (O, "O"),
        (P, "P"),
        (Q, "Q"),
        (R, "R"),
        (S, "S"),
        (T, "T"),
        (U, "U"),
        (V, "V"),
        (W, "W"),
        (X, "X"),
        (Y, "Y"),
        (Z, "Z"),
        (Space, "SPACE"),
        (Escape, "ESC"),
        (Enter, "ENTER"),
        (Tab, "TAB"),
        (Delete, "DELETE"),
    ];
    for (k, name) in pairs {
        if i.key_pressed(k) {
            return Some(name);
        }
    }
    None
}

fn big_tool(
    ui: &mut egui::Ui,
    tex: &egui::TextureHandle,
    label: &str,
    color: Color32,
    bg_override: Option<Color32>,
    border_override: Option<Color32>,
) -> egui::Response {
    let text_w = ui
        .fonts(|f| {
            f.layout_no_wrap(label.to_string(), egui::FontId::proportional(11.0), color)
                .size()
                .x
        })
        .clamp(36.0, 84.0);
    let width = text_w + 16.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 58.0), Sense::click());
    let draw = rect.shrink2(egui::vec2(3.0, 2.0));

    let mut bg = bg_override;
    let mut border = border_override;
    if resp.hovered() {
        bg = Some(Color32::from_rgb(243, 246, 249));
        border = Some(Color32::from_rgb(226, 230, 235));
    }
    if resp.is_pointer_button_down_on() {
        bg = Some(Color32::from_rgb(232, 236, 241));
    }

    if let Some(c) = bg {
        ui.painter().rect_filled(draw, 8.0, c);
    }
    if let Some(c) = border {
        ui.painter().rect_stroke(draw, 8.0, Stroke::new(1.0_f32, c));
    }

    let img_rect = egui::Rect::from_center_size(
        egui::pos2(draw.center().x, draw.min.y + 18.0),
        Vec2::new(22.0, 22.0),
    );
    egui::Image::new(tex).tint(color).paint_at(ui, img_rect);

    let label_rect = egui::Rect::from_min_max(
        egui::pos2(draw.min.x + 4.0, draw.max.y - 20.0),
        egui::pos2(draw.max.x - 4.0, draw.max.y - 2.0),
    );
    let label_font = egui::FontId::proportional(11.0);
    // Center the label under the icon; only over-wide labels fall back to
    // left-aligned truncation.
    let label_w = ui.fonts(|f| {
        f.layout_no_wrap(label.to_string(), label_font.clone(), color)
            .size()
            .x
    });
    if label_w <= label_rect.width() {
        let galley = ui
            .painter()
            .layout_no_wrap(label.to_string(), label_font, color);
        let y = label_rect.center().y - galley.size().y * 0.5;
        ui.painter().galley(
            egui::pos2(label_rect.center().x - label_w * 0.5, y),
            galley,
            color,
        );
    } else {
        paint_ellipsis(ui, label_rect, label, label_font, color);
    }
    resp.on_hover_text(label)
}

/// Dialog footer buttons: the primary action mirrors the main blue CTA, the
/// cancel stays neutral — same pair everywhere so dialogs feel like one app.
fn dialog_primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(Color32::WHITE))
            .fill(Color32::from_rgb(47, 111, 237))
            .min_size(Vec2::new(88.0, 28.0)),
    )
}

fn dialog_secondary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(text).min_size(Vec2::new(72.0, 28.0)))
}

fn header_cell(ui: &mut egui::Ui, text: &str, w: f32, h: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_rgb(247, 248, 250));
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, Color32::from_rgb(226, 230, 235)),
    );
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top()),
        egui::pos2(rect.right() - 6.0, rect.bottom()),
    );
    paint_ellipsis(
        ui,
        text_rect,
        text,
        egui::FontId::proportional(12.0),
        Color32::from_rgb(92, 107, 122),
    );
}

fn paint_ellipsis(ui: &egui::Ui, rect: egui::Rect, text: &str, font: egui::FontId, color: Color32) {
    let width = rect.width();
    if width < 4.0 || text.is_empty() || rect.height() < 4.0 {
        return;
    }
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font,
            color,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    let galley = ui.fonts(|f| f.layout_job(job));
    let y = rect.center().y - galley.size().y * 0.5;
    ui.painter_at(rect)
        .galley(egui::pos2(rect.left(), y), galley, color);
}

/// Keep a leading label and the file name when a cell stores a full path.
fn shorten_detail(text: &str) -> String {
    let Some((label, rest)) = text.split_once(": ") else {
        return text.to_string();
    };
    let rest = rest.trim();
    let name = std::path::Path::new(rest)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(rest);
    if name.is_empty() || name == rest {
        text.to_string()
    } else {
        format!("{label}: {name}")
    }
}

#[cfg(test)]
mod ui_text {
    use super::shorten_detail;

    #[test]
    fn detail_keeps_the_file_name() {
        assert_eq!(
            shorten_detail(r"Image: D:\GITHUB\0. CLONE\AutoClick\captures\smart.bmp"),
            "Image: smart.bmp"
        );
        assert_eq!(shorten_detail("X: 152, Y: 346"), "X: 152, Y: 346");
        assert_eq!(shorten_detail("Bắt đầu"), "Bắt đầu");
    }
}
