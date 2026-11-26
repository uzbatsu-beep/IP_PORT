//! Shared visual building blocks. Keep scanner behavior in the calling views.
use egui::{Button, Color32, Frame, RichText, Stroke, Ui};

use super::theme;
use crate::app::StatusLevel;

pub const TABLE_ROW_HEIGHT: f32 = 22.0;
pub const SECTION_GAP: f32 = 8.0;

#[derive(Clone, Copy)]
pub enum Tone {
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

pub fn tone_color(ui: &Ui, tone: Tone) -> Color32 {
    match tone {
        Tone::Neutral => ui.visuals().text_color(),
        Tone::Accent => theme::accent(ui),
        Tone::Success => theme::success(ui),
        Tone::Warning => theme::warning(ui),
        Tone::Danger => theme::error(ui),
    }
}

pub fn semantic_text(ui: &Ui, text: impl Into<String>, tone: Tone) -> RichText {
    RichText::new(text).color(tone_color(ui, tone))
}

pub fn status_text(ui: &Ui, text: &str, level: StatusLevel) -> RichText {
    let tone = match level {
        StatusLevel::Idle => return RichText::new(text).weak(),
        StatusLevel::Running => Tone::Accent,
        StatusLevel::Stopping => Tone::Warning,
        StatusLevel::Success => Tone::Success,
        StatusLevel::Error => Tone::Danger,
    };
    semantic_text(ui, text, tone)
}

pub fn primary_button(ui: &Ui, text: &str) -> Button<'static> {
    Button::new(RichText::new(text).strong())
        .fill(ui.visuals().selection.bg_fill)
        .stroke(Stroke::new(1.0, tone_color(ui, Tone::Accent)))
}

pub fn danger_button(ui: &Ui, text: &str, emphasized: bool) -> Button<'static> {
    let button = Button::new(RichText::new(text));
    if emphasized {
        let danger = tone_color(ui, Tone::Danger);
        button
            .fill(theme::tint(ui.visuals().panel_fill, danger, 12))
            .stroke(Stroke::new(1.0, danger))
    } else {
        button
    }
}

pub fn secondary_button(text: &str) -> Button<'static> {
    Button::new(RichText::new(text))
}

pub fn surface(ui: &Ui) -> Frame {
    Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .corner_radius(6.0)
}

pub fn section_heading(ui: &mut Ui, title: &str) {
    ui.heading(RichText::new(title).strong());
}

pub fn section_divider(ui: &mut Ui) {
    ui.add_space(SECTION_GAP);
    ui.separator();
    ui.add_space(SECTION_GAP);
}

pub fn empty_state(ui: &mut Ui, message: &str) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(message).weak());
    });
}

pub fn metric_row(ui: &mut Ui, label: &str, value: impl ToString, emphasized: bool) {
    ui.label(RichText::new(label).weak());
    let text = RichText::new(value.to_string()).monospace();
    ui.label(if emphasized { text.strong() } else { text });
    ui.end_row();
}

/// RTT severity is independent of the accent palette. Use darker shades on
/// light surfaces without changing bucket order or numerical thresholds.
pub fn rtt_color(ui: &Ui, index: usize, dark_color: Color32) -> Color32 {
    if ui.visuals().dark_mode {
        return dark_color;
    }
    let (r, g, b) = match index {
        0 => (19, 107, 61),
        1 => (71, 110, 20),
        2 => (133, 77, 14),
        3 => (154, 61, 12),
        4 => (159, 29, 35),
        _ => return tone_color(ui, Tone::Neutral),
    };
    Color32::from_rgb(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::{Appearance, Palette, Theme};

    #[test]
    fn components_render_in_every_appearance() {
        for theme in [Theme::Light, Theme::Dark] {
            for palette in Palette::ALL {
                let ctx = egui::Context::default();
                theme::apply_appearance(&ctx, Appearance { theme, palette });
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    section_heading(ui, "Test");
                    let _ = ui.add(primary_button(ui, "Start"));
                    let _ = ui.add(danger_button(ui, "Stop", true));
                    let _ = ui.add_enabled(false, secondary_button("Clear"));
                    for level in [
                        StatusLevel::Idle,
                        StatusLevel::Running,
                        StatusLevel::Stopping,
                        StatusLevel::Success,
                        StatusLevel::Error,
                    ] {
                        ui.label(status_text(ui, "Status", level));
                    }
                    surface(ui).show(ui, |ui| {
                        egui::Grid::new("component_metrics").show(ui, |ui| {
                            metric_row(ui, "Open", 42, true);
                        });
                    });
                    section_divider(ui);
                    empty_state(ui, "No results");
                });
                // No renderer in this headless test; discard texture updates explicitly.
                output.textures_delta.clear();
                assert!(!output.shapes.is_empty());
            }
        }
    }
}
