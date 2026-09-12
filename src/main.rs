mod app;
mod capture;
mod engine;
mod eval;
mod i18n;
mod model;
mod record;
mod vision;
mod icons;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("--run") {
        let path = args.get(2).cloned().unwrap_or_else(|| "script.amk".into());
        match std::fs::read_to_string(&path) {
            Ok(s) => match model::Script::load_json(&s) {
                Ok(script) => {
                    let dir = std::path::Path::new(&path)
                        .parent()
                        .map(|p| p.to_path_buf());
                    let engine = engine::Engine::new();
                    engine.play(script, 2.5, 1, None, dir);
                    while engine.snapshot_state() != engine::RunState::Idle {
                        std::thread::sleep(std::time::Duration::from_millis(40));
                    }
                    for line in engine.logs() {
                        println!("{}  {}", line.time, line.text);
                    }
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("load {path}: {e}");
                    std::process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("read {path}: {e}");
                std::process::exit(1);
            }
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 640.0])
            .with_min_inner_size([780.0, 480.0])
            .with_title("Automatic Mouse and Keyboard"),
        ..Default::default()
    };
    eframe::run_native(
        "Automatic Mouse and Keyboard",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            apply_style(&cc.egui_ctx);
            Box::new(app::AmkApp::new())
        }),
    )
}

/// WINDIR\Fonts\segoeui.ttf via join (not one Windows-style PathBuf component).
fn segoe_ui_path() -> Option<std::path::PathBuf> {
    let windir = std::env::var_os("WINDIR")?;
    Some(
        std::path::PathBuf::from(windir)
            .join("Fonts")
            .join("segoeui.ttf"),
    )
}

fn segoe_emoji_path() -> Option<std::path::PathBuf> {
    let windir = std::env::var_os("WINDIR")?;
    Some(
        std::path::PathBuf::from(windir)
            .join("Fonts")
            .join("seguiemj.ttf"),
    )
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut changed = false;
    if let Some(path) = segoe_ui_path() {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("segoe".to_owned(), egui::FontData::from_owned(bytes));
            if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                fam.insert(0, "segoe".to_owned());
            }
            changed = true;
        }
    }
    // Emoji only as a later fallback — never ahead of Segoe (tofu on ă/ê/ư).
    if let Some(path) = segoe_emoji_path() {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("segoe_emoji".to_owned(), egui::FontData::from_owned(bytes));
            if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                fam.push("segoe_emoji".to_owned());
            }
            changed = true;
        }
    }
    if changed {
        ctx.set_fonts(fonts);
    }
}

fn apply_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.window_fill = egui::Color32::from_rgb(240, 240, 240);
    style.visuals.panel_fill = egui::Color32::from_rgb(240, 240, 240);
    style.visuals.faint_bg_color = egui::Color32::from_rgb(250, 250, 250);

    // Widget states: ensure radio/checkbox decorations are VISIBLE.
    // inactive = normal, un-hovered widget
    style.visuals.widgets.inactive.bg_fill = egui::Color32::WHITE;
    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(248, 248, 248);
    style.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(180, 180, 180));
    style.visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(60, 60, 60));
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(3.0);

    // hovered widget
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(229, 241, 251);
    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(229, 241, 251);
    style.visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(100, 160, 220));
    style.visuals.widgets.hovered.fg_stroke =
        egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(30, 30, 30));

    // active (pressed) widget
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(200, 225, 245);
    style.visuals.widgets.active.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(60, 120, 200));
    style.visuals.widgets.active.fg_stroke =
        egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 0, 0));

    // open (combo box opened, etc.)
    style.visuals.widgets.open.bg_fill = egui::Color32::from_rgb(220, 235, 250);

    // noninteractive (labels, separators)
    style.visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(220, 220, 220));

    style.visuals.selection.bg_fill = egui::Color32::from_rgb(51, 153, 255);
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    ctx.set_style(style);
}

#[cfg(test)]
mod font_tests {
    use super::*;

    #[test]
    fn segoe_path_uses_join() {
        let Some(p) = segoe_ui_path() else {
            return;
        };
        assert_eq!(p.file_name().and_then(|s| s.to_str()), Some("segoeui.ttf"));
        assert!(
            p.components().count() >= 3,
            "must be WINDIR/Fonts/segoeui.ttf, not one path component"
        );
    }
}
