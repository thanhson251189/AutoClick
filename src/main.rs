mod app;
mod capture;
mod clipboard;
mod engine;
mod eval;
mod hotkeys;
mod i18n;
mod icons;
mod model;
mod record;
mod vision;

use eframe::egui;

/// Marker between the copied executable image and the embedded script.
pub(crate) const EMBED_MARKER: &[u8] = b"
<<AMK-EMBED>>
";

/// Script JSON appended to a copied executable by "Compile to EXE".
fn extract_embedded(bytes: &[u8]) -> Option<String> {
    let pos = bytes
        .windows(EMBED_MARKER.len())
        .rposition(|w| w == EMBED_MARKER)?;
    let rest = &bytes[pos + EMBED_MARKER.len()..];
    if rest.is_empty() {
        return None;
    }
    String::from_utf8(rest.to_vec()).ok()
}

/// Run one script headlessly with streamed logs; exit code 1 on failure.
fn run_headless(script: model::Script, dir: Option<std::path::PathBuf>) -> ! {
    let engine = engine::Engine::new();
    engine.play(script, 2.5, 1, None, dir, false);
    let mut printed = 0usize;
    while engine.snapshot_state() != engine::RunState::Idle {
        let logs = engine.logs();
        while printed < logs.len() {
            println!("{}  {}", logs[printed].time, logs[printed].text);
            printed += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
    for line in engine.logs().iter().skip(printed) {
        println!("{}  {}", line.time, line.text);
    }
    if !engine.last_run_ok() {
        std::process::exit(1);
    }
    std::process::exit(0);
}

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
                    run_headless(script, dir);
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
    // A compiled single-file EXE carries its script after the marker.
    if args.len() == 1 {
        if let Ok(bytes) = std::fs::read(std::env::current_exe().unwrap_or_default()) {
            if let Some(json) = extract_embedded(&bytes) {
                match model::Script::load_json(&json) {
                    Ok(script) => run_headless(script, None),
                    Err(e) => {
                        eprintln!("embedded script: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 640.0])
            .with_min_inner_size([720.0, 480.0])
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
    // Slightly above egui defaults: the dense action table reads better.
    style.text_styles = [
        (egui::TextStyle::Heading, egui::FontId::proportional(19.0)),
        (egui::TextStyle::Body, egui::FontId::proportional(14.0)),
        (egui::TextStyle::Button, egui::FontId::proportional(14.0)),
        (egui::TextStyle::Small, egui::FontId::proportional(11.0)),
        (egui::TextStyle::Monospace, egui::FontId::monospace(13.0)),
    ]
    .into();
    let line = egui::Color32::from_rgb(226, 230, 235);
    let ink = egui::Color32::from_rgb(28, 36, 48);
    style.visuals.window_fill = egui::Color32::from_rgb(244, 246, 248);
    style.visuals.panel_fill = egui::Color32::WHITE;
    style.visuals.faint_bg_color = egui::Color32::from_rgb(247, 248, 250);
    style.visuals.extreme_bg_color = egui::Color32::WHITE;
    style.visuals.window_rounding = egui::Rounding::same(8.0);
    style.visuals.menu_rounding = egui::Rounding::same(6.0);

    style.visuals.widgets.inactive.bg_fill = egui::Color32::WHITE;
    style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(247, 248, 250);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, line);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0);

    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(232, 241, 251);
    style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(243, 246, 249);
    style.visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(47, 111, 237));
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(6.0);

    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(214, 228, 246);
    style.visuals.widgets.active.bg_stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(47, 111, 237));
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    style.visuals.widgets.active.rounding = egui::Rounding::same(6.0);

    style.visuals.widgets.open.bg_fill = egui::Color32::WHITE;
    style.visuals.widgets.open.rounding = egui::Rounding::same(6.0);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, line);

    style.visuals.selection.bg_fill = egui::Color32::from_rgb(232, 241, 251);
    style.visuals.selection.stroke =
        egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(18, 48, 85));
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(6.0);
    ctx.set_style(style);
}

#[cfg(test)]
mod embed_tests {
    use super::*;

    #[test]
    fn extract_embedded_roundtrips_and_rejects_plain_executables() {
        let script = r#"{"version":"1.0","name":"t","actions":[]}"#;
        let mut bytes = b"MZ fake pe image".to_vec();
        bytes.extend_from_slice(EMBED_MARKER);
        bytes.extend_from_slice(script.as_bytes());
        assert_eq!(extract_embedded(&bytes).as_deref(), Some(script));
        assert_eq!(extract_embedded(b"MZ no marker"), None);
    }
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
