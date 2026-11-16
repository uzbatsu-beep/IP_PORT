use egui::{Color32, Stroke, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(0x3d, 0x9b, 0xff);
pub const SUCCESS: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
pub const WARNING: Color32 = Color32::from_rgb(0xf0, 0xb4, 0x4c);
pub const ERROR: Color32 = Color32::from_rgb(0xf2, 0x6d, 0x6d);

/// Применяет современное тёмное оформление к контексту egui.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);

    let mut visuals = Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(0x16, 0x18, 0x1d);
    visuals.window_fill = Color32::from_rgb(0x1c, 0x1f, 0x26);
    visuals.extreme_bg_color = Color32::from_rgb(0x11, 0x13, 0x17);
    visuals.faint_bg_color = Color32::from_rgb(0x20, 0x24, 0x2c);
    visuals.override_text_color = Some(Color32::from_rgb(0xd8, 0xdd, 0xe6));
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.widgets.noninteractive.bg_stroke =
        Stroke::new(1.0, Color32::from_rgb(0x2c, 0x31, 0x3b));
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(0x25, 0x2a, 0x33);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x25, 0x2a, 0x33);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x2f, 0x36, 0x42);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.8));
    visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);

    ctx.set_visuals_of(egui::Theme::Dark, visuals);

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 26.0;
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(20.0));
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
    });
}
