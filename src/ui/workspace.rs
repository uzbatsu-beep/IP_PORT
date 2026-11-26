//! UI-03.1: workspace shell, without changes to discovery or asset storage.
use egui::{RichText, Ui};

use crate::app::{App, ResultsViewMode, StatusLevel};
use crate::ui::navigation::Section;
pub use crate::ui::preferences::Language;
use crate::ui::{components, navigation, preferences, results, sidebar, toolbar, views};

#[derive(Default)]
pub struct Workspace {
    pub section: Section,
    pub language: Language,
    pub expanded: bool,
    pub knowledge_query: String,
    pub discovery: crate::ui::discovery::DiscoveryState,
    preference_path: Option<std::path::PathBuf>,
    pub preference_error: Option<String>,
}

impl Workspace {
    pub fn restore_preferences(&mut self) {
        self.preference_path = preferences::path();
        if let Some(path) = &self.preference_path {
            match preferences::load(path) {
                Ok(language) => self.language = language,
                Err(error) => self.preference_error = Some(error.to_string()),
            }
        } else {
            self.preference_error = Some("Settings directory unavailable".to_owned());
        }
    }

    pub fn save_language(&mut self) {
        self.preference_error = match &self.preference_path {
            Some(path) => preferences::save(path, self.language)
                .err()
                .map(|error| error.to_string()),
            None => Some("Settings directory unavailable".to_owned()),
        };
    }
}

pub fn initial_view() -> ResultsViewMode {
    ResultsViewMode::GroupedByHost
}

// Logical points, not physical pixels. Keep space for the existing port table.
fn wide_layout(width: f32) -> bool {
    width >= 1100.0
}

fn select_section(app: &mut App, section: Section) {
    app.workspace.section = section;
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let wide = wide_layout(ui.available_width() - if app.workspace.expanded { 132.0 } else { 0.0 });
    let language = app.workspace.language;

    toolbar::show(app, ui);

    egui::Panel::bottom("workspace_status")
        .resizable(false)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let level = if app.is_stopping() {
                    StatusLevel::Stopping
                } else if app.is_running() {
                    StatusLevel::Running
                } else {
                    app.status_level
                };
                let state = match level {
                    StatusLevel::Idle => language.text("Ready", "Готово"),
                    StatusLevel::Running => language.text("Scanning", "Сканирование"),
                    StatusLevel::Stopping => language.text("Stopping", "Остановка"),
                    StatusLevel::Success => language.text("Completed", "Завершено"),
                    StatusLevel::Error => language.text("Error", "Ошибка"),
                };
                ui.label(components::status_text(ui, state, level));
                ui.separator();
                ui.label(format!(
                    "{} {}/{}",
                    language.text("Probes:", "Проверки:"),
                    app.done_probes,
                    app.total_probes
                ));
                ui.separator();
                ui.label(format!(
                    "{} {}",
                    language.text("Stored:", "Сохранено:"),
                    app.results.len()
                ));
                if !app.status.is_empty() {
                    ui.label(RichText::new(&app.status).small().weak());
                }
            });
        });

    if let Some(section) = navigation::show(app, ui) {
        select_section(app, section);
    }

    if app.workspace.section == Section::Discovery {
        let max_width = if wide { 340.0 } else { 460.0 };
        egui::Panel::left("controls")
            .resizable(true)
            .default_size(300.0)
            .size_range(280.0..=max_width)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    sidebar::show(app, ui);
                });
            });
    }

    if wide && app.workspace.section.has_results() {
        egui::Panel::right("workspace_inspector")
            .resizable(false)
            .default_size(240.0)
            .size_range(240.0..=240.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    inspector(app, ui, language);
                });
            });
    }

    egui::CentralPanel::default().show(ui, |ui| {
        if app.workspace.section.has_results() && !wide && app.selected_result.is_some() {
            egui::CollapsingHeader::new(language.text("Device inspector", "Карточка устройства"))
                .default_open(false)
                .show(ui, |ui| inspector(app, ui, language));
        }
        match app.workspace.section {
            Section::Discovery | Section::Devices => results::show(app, ui),
            section => views::show(section, app, ui),
        }
    });
}

fn inspector(app: &App, ui: &mut Ui, language: Language) {
    components::section_heading(ui, language.text("Device inspector", "Карточка устройства"));
    let Some(index) = app
        .selected_result
        .filter(|&index| index < app.results.len())
    else {
        ui.label(
            RichText::new(language.text(
                "Select a result to inspect its observed host.",
                "Выберите результат для просмотра наблюдаемого узла.",
            ))
            .weak(),
        );
        return;
    };
    let row = app.row(index);
    let info = app.host_info.get(&row.raw.ip);
    let unavailable = language.text("Not observed", "Нет наблюдения");
    ui.label(RichText::new(&row.ip_text).monospace().strong());
    ui.label(
        RichText::new(language.text(
            "Observed IP host; physical identity unverified.",
            "Наблюдаемый IP-узел; физическое устройство не подтверждено.",
        ))
        .small()
        .weak(),
    );
    components::section_divider(ui);
    for (label, value) in [
        (
            language.text("Hostname (PTR)", "Имя узла (PTR)"),
            info.and_then(|i| i.hostname.as_deref())
                .unwrap_or(unavailable),
        ),
        (
            language.text("MAC (neighbor cache)", "MAC (кэш соседей)"),
            info.and_then(|i| i.mac.as_deref()).unwrap_or(unavailable),
        ),
        (
            language.text("OUI vendor", "Владелец OUI"),
            info.and_then(|i| i.vendor.as_deref())
                .unwrap_or(unavailable),
        ),
    ] {
        ui.label(RichText::new(label).weak().small());
        ui.label(value);
    }
    if info.is_some_and(|i| i.neighbor_ambiguous) {
        ui.label(components::semantic_text(
            ui,
            language.text("Ambiguous neighbor records", "Неоднозначные записи соседей"),
            components::Tone::Warning,
        ));
    }
    components::section_divider(ui);
    ui.label(RichText::new(language.text("Selected endpoint", "Выбранный порт")).strong());
    ui.label(format!("{}/{}", row.port_text, row.protocol_text));
    ui.label(row.state_text);
    ui.label(&row.raw.service);
    ui.label(&row.latency_text);
    ui.label(
        RichText::new(language.text(
            "A port or OUI does not establish a device model.",
            "Порт или OUI не подтверждают модель устройства.",
        ))
        .small()
        .weak(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    #[test]
    fn startup_and_navigation_preserve_scan_configuration() {
        let mut app = App::default_state();
        assert_eq!(initial_view(), ResultsViewMode::GroupedByHost);
        let targets = app.targets_input.clone();
        app.push_result(crate::scanner::ScanResult::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            80,
            crate::scanner::probe::service_name(80),
            Duration::from_millis(1),
        ));
        app.selected_result = Some(0);
        app.view_mode = ResultsViewMode::Flat;
        select_section(&mut app, Section::Devices);
        assert_eq!(app.view_mode, ResultsViewMode::Flat);
        select_section(&mut app, Section::Settings);
        select_section(&mut app, Section::Discovery);
        assert_eq!(app.targets_input, targets);
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.selected_result, Some(0));
    }

    #[test]
    fn workspace_shell_renders_at_supported_widths_and_languages() {
        let mut app = App::default_state();
        app.push_result(crate::scanner::ScanResult::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            80,
            crate::scanner::probe::service_name(80),
            Duration::from_millis(1),
        ));
        app.selected_result = Some(0);
        assert!(!wide_layout(880.0));
        assert!(wide_layout(1180.0));
        for language in [Language::English, Language::Russian] {
            app.workspace.language = language;
            for section in Section::ALL {
                select_section(&mut app, section);
                for width in [880.0, 1180.0, 1920.0] {
                    let ctx = egui::Context::default();
                    crate::ui::theme::apply(&ctx);
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 760.0),
                        )),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| app.update_ui(ui));
                    output.textures_delta.clear();
                    assert!(!output.shapes.is_empty());
                    assert_eq!(app.selected_result, Some(0));
                    assert_eq!(app.results.len(), 1);
                }
            }
        }
    }
}
