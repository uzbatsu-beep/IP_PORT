//! Routes reuse existing application services rather than owning scanner state.
use egui::{RichText, Ui};

use super::{components, knowledge, navigation::Section, rtt_view, sidebar};
use crate::app::App;

pub fn show(section: Section, app: &mut App, ui: &mut Ui) {
    let language = app.workspace.language;
    egui::ScrollArea::vertical().show(ui, |ui| {
        components::section_heading(ui, section.name(language));
        match section {
            Section::Diagnostics => {
                ui.label(app.progress_text());
                if let Some(stats) = app.current_rtt_stats() {
                    rtt_view::show_rtt_panel(app, &stats, ui);
                } else {
                    ui.label(language.text("No measured RTT", "Нет измерений RTT"));
                }
                ui.label(format!(
                    "PTR: {} / {}",
                    app.host_info_stats.ptr_resolved, app.host_info_stats.completed
                ));
                ui.label(format!("MAC: {}", app.host_info_stats.mac_found));
                if let Some(error) = &app.host_info_stats.last_ptr_error {
                    ui.label(RichText::new(error).color(super::theme::warning(ui)));
                }
            }
            Section::Knowledge => {
                egui::CollapsingHeader::new("IEEE OUI")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!app.is_running(), |ui| sidebar::show_oui(app, ui));
                    });
                ui.separator();
                knowledge::show(&mut app.workspace.knowledge_query, language, ui);
            }
            Section::Reports => {
                ui.label(language.text(
                    "Export all stored results (not the filter).",
                    "Экспорт всех сохранённых результатов без учёта фильтра.",
                ));
                let enabled = !app.results.is_empty() && !app.is_exporting();
                ui.add_enabled_ui(enabled, |ui| {
                    if ui.button("CSV").clicked() {
                        app.export_csv();
                    }
                    if ui.button("JSON").clicked() {
                        app.export_json();
                    }
                    if ui.button("HTML").clicked() {
                        app.export_html();
                    }
                });
                if app.is_exporting() {
                    ui.spinner();
                }
                if let Some(path) = &app.last_export_path
                    && ui.link(path.display().to_string()).clicked()
                {
                    crate::export::reveal_in_file_manager(path);
                }
            }
            Section::Settings => {
                sidebar::show_appearance(app, ui);
                ui.label(language.text("Shell language", "Язык оболочки"));
                let previous = app.workspace.language;
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut app.workspace.language,
                        super::preferences::Language::English,
                        "EN",
                    );
                    ui.selectable_value(
                        &mut app.workspace.language,
                        super::preferences::Language::Russian,
                        "RU",
                    );
                });
                if previous != app.workspace.language {
                    app.workspace.save_language();
                }
                if let Some(error) = &app.workspace.preference_error {
                    ui.label(RichText::new(error).color(super::theme::error(ui)));
                    if ui
                        .button(language.text("Retry save", "Повторить сохранение"))
                        .clicked()
                    {
                        app.workspace.save_language();
                    }
                }
                ui.label(language.text(
                    "Existing scanner messages are not fully translated yet.",
                    "Сообщения сканера пока переведены не полностью.",
                ));
            }
            Section::Discovery | Section::Devices => {}
        }
    });
}
