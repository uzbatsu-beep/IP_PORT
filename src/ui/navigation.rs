//! Typed routes independent of scanner lifecycle.
use egui::{RichText, Ui};

use super::icons::{self, Icon};
use super::preferences::Language;
use crate::app::App;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Discovery,
    Devices,
    Diagnostics,
    Knowledge,
    Reports,
    Settings,
}

impl Section {
    pub const ALL: [Self; 6] = [
        Self::Discovery,
        Self::Devices,
        Self::Diagnostics,
        Self::Knowledge,
        Self::Reports,
        Self::Settings,
    ];

    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Discovery => language.text("Discovery", "Обнаружение"),
            Self::Devices => language.text("Devices", "Устройства"),
            Self::Diagnostics => language.text("Diagnostics", "Диагностика"),
            Self::Knowledge => language.text("Knowledge Base", "База знаний"),
            Self::Reports => language.text("Reports", "Отчёты"),
            Self::Settings => language.text("Settings", "Настройки"),
        }
    }

    pub fn has_results(self) -> bool {
        matches!(self, Self::Discovery | Self::Devices)
    }

    fn icon(self) -> Icon {
        match self {
            Self::Discovery => Icon::Discovery,
            Self::Devices => Icon::Devices,
            Self::Diagnostics => Icon::Diagnostics,
            Self::Knowledge => Icon::Knowledge,
            Self::Reports => Icon::Reports,
            Self::Settings => Icon::Settings,
        }
    }
}

fn item(ui: &mut Ui, icon: Icon, name: &str, expanded: bool, active: bool) -> bool {
    let label = if expanded { name } else { "" };
    let response = ui.add_sized(
        [ui.available_width(), 32.0],
        egui::Button::new(RichText::new(format!("    {label}"))).selected(active),
    );
    let center = egui::pos2(response.rect.left() + 16.0, response.rect.center().y);
    let rect = egui::Rect::from_center_size(center, egui::vec2(20.0, 20.0));
    let color = if active {
        super::theme::accent(ui)
    } else {
        ui.visuals().text_color()
    };
    icons::paint(ui.painter(), rect, icon, color);
    response.on_hover_text(name).clicked()
}

pub fn show(app: &mut App, ui: &mut Ui) -> Option<Section> {
    let expanded = app.workspace.expanded;
    let width = if expanded { 176.0 } else { 44.0 };
    let language = app.workspace.language;
    let mut next = None;
    egui::Panel::left("workspace_navigation")
        .resizable(false)
        .default_size(width)
        .size_range(width..=width)
        .show(ui, |ui| {
            if item(
                ui,
                Icon::Menu,
                language.text("Navigation", "Навигация"),
                expanded,
                false,
            ) {
                app.workspace.expanded = !expanded;
                ui.ctx().request_repaint();
            }
            ui.separator();
            for section in Section::ALL {
                if section == Section::Settings {
                    ui.separator();
                }
                if item(
                    ui,
                    section.icon(),
                    section.name(language),
                    expanded,
                    app.workspace.section == section,
                ) {
                    next = Some(section);
                }
            }
        });
    next
}
