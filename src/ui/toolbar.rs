//! Global toolbar: commands and scan context are available in every route.
use super::commands::{self, AppCommand};
use super::{components, preferences::Language, theme::Theme};
use crate::app::{App, StatusLevel};
use crate::scanner::ScanProtocol;
use egui::{RichText, Ui};

/// Capture the inputs used by a successfully spawned scan, not later draft edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanContext {
    targets: String,
    ports: String,
    bind: String,
    exclusions: String,
    protocol: ScanProtocol,
}

impl ScanContext {
    pub fn from_app(app: &App) -> Self {
        Self {
            targets: app.targets_input.clone(),
            ports: app.ports_input.clone(),
            bind: app.bind_addr_input.clone(),
            exclusions: app.exclusions_input.clone(),
            protocol: app.protocol,
        }
    }
}

fn summary(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let mut text: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        text.push('…');
    }
    text
}

fn field(ui: &mut Ui, label: &str, value: &str, fallback: &str, limit: usize) {
    let value = if value.trim().is_empty() { fallback } else { value };
    ui.label(RichText::new(format!("{label}: {}", summary(value, limit))).small())
        .on_hover_text(value);
}

fn menu_action(ui: &mut Ui, app: &mut App, label: &str, action: AppCommand) {
    let enabled = commands::available(app, action);
    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
        commands::execute(app, ui.ctx(), action);
        ui.close();
    }
}

fn controls(app: &mut App, ui: &mut Ui, lang: Language) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("IP_PORT").strong());
        ui.separator();
        let can_start = commands::available(app, AppCommand::StartScan);
        let start = components::primary_button(ui, lang.text("Start", "Пуск"));
        let hint = match app.refresh_input_preview() {
            Ok(_) => format!("{}+Enter", crate::ui::cmd_key_name()),
            Err(error) => error.to_owned(),
        };
        if ui.add_enabled(can_start, start).on_hover_text(hint).clicked() {
            commands::execute(app, ui.ctx(), AppCommand::StartScan);
        }

        let stopping = app.is_stopping();
        let can_stop = commands::available(app, AppCommand::StopScan);
        let stop = components::danger_button(
            ui,
            if stopping {
                lang.text("Stopping…", "Остановка…")
            } else {
                lang.text("Stop", "Стоп")
            },
            can_stop,
        );
        if ui.add_enabled(can_stop, stop).on_hover_text("Esc").clicked() {
            commands::execute(app, ui.ctx(), AppCommand::StopScan);
        }
        ui.separator();

        let cmd = crate::ui::cmd_key_name();
        ui.add_enabled_ui(!app.is_running(), |ui| {
            ui.menu_button(lang.text("Profile", "Профиль"), |ui| {
                menu_action(ui, app, &format!("{} ({cmd}+O)", lang.text("Load", "Загрузить")), AppCommand::LoadProfile);
                menu_action(ui, app, &format!("{} ({cmd}+S)", lang.text("Save", "Сохранить")), AppCommand::SaveProfile);
            });
        });
        let can_export = commands::available(app, AppCommand::ExportCsv);
        ui.add_enabled_ui(can_export, |ui| {
            ui.menu_button(lang.text("Export", "Экспорт"), |ui| {
                menu_action(ui, app, &format!("CSV ({cmd}+E)"), AppCommand::ExportCsv);
                menu_action(ui, app, &format!("JSON ({cmd}+Shift+E)"), AppCommand::ExportJson);
                menu_action(ui, app, "HTML", AppCommand::ExportHtml);
            }).response.on_hover_text(lang.text("Export all stored results, ignoring filters", "Экспорт всех сохранённых результатов, без учёта фильтров"));
        });
        if app.is_exporting() {
            ui.spinner();
        }
        ui.separator();
        let theme_label = match app.appearance.theme {
            Theme::Dark => lang.text("Light", "Светлая"),
            Theme::Light => lang.text("Dark", "Тёмная"),
        };
        if ui.button(theme_label).clicked() {
            commands::execute(app, ui.ctx(), AppCommand::ToggleTheme);
        }
        if ui.button(lang.text("Settings", "Настройки")).clicked() {
            commands::execute(app, ui.ctx(), AppCommand::OpenSettings);
        }
        ui.selectable_value(&mut app.workspace.language, Language::English, "EN");
        ui.selectable_value(&mut app.workspace.language, Language::Russian, "RU");
    });
}

fn context(app: &App, ui: &mut Ui, lang: Language) {
    let running = app.is_running();
    // Mock scan handles used by headless tests have no real start snapshot.
    let draft;
    let current = if let Some(active) = app.active_scan_context.as_ref().filter(|_| running) {
        active
    } else {
        draft = ScanContext::from_app(app);
        &draft
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(if running {
            lang.text("Active scan", "Активное сканирование")
        } else {
            lang.text("Next scan", "Следующее сканирование")
        }).strong().small());
        field(ui, lang.text("Bind IP", "Локальный IP"), &current.bind, lang.text("Automatic", "Авто"), 28);
        field(ui, lang.text("Targets", "Цели"), &current.targets, "—", 42);
        field(ui, lang.text("Ports", "Порты"), &current.ports, "—", 30);
        field(ui, lang.text("Exclusions", "Исключения"), &current.exclusions, "—", 18);
        ui.label(match current.protocol {
            ScanProtocol::Tcp => "TCP",
            ScanProtocol::Udp => "UDP",
        });
    });
    if running {
        let (label, level) = if app.is_stopping() {
            (lang.text("Stopping…", "Остановка…"), StatusLevel::Stopping)
        } else {
            (lang.text("Scanning", "Сканирование"), StatusLevel::Running)
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(components::status_text(ui, label, level));
            ui.add(egui::ProgressBar::new(app.progress_fraction()).desired_width(120.0).show_percentage());
            ui.label(RichText::new(app.progress_text()).small());
        });
    } else if app.total_probes > 0 {
        ui.label(RichText::new(format!(
            "{}: {}/{}",
            lang.text("Last scan probes", "Последнее сканирование: проверок"),
            app.done_probes,
            app.total_probes
        )).small().weak());
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let lang = app.workspace.language;
    egui::Panel::top("workspace_header")
        .resizable(false)
        .show(ui, |ui| {
            controls(app, ui, lang);
            ui.separator();
            context(app, ui, lang);
        });
    if app.workspace.language != lang {
        app.workspace.save_language();
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_independent_of_draft_inputs() {
        let mut app = App::default_state();
        let snapshot = ScanContext::from_app(&app);
        app.targets_input = "192.0.2.1".to_owned();
        app.ports_input = "502".to_owned();
        assert_ne!(snapshot, ScanContext::from_app(&app));
        assert_eq!(snapshot.targets, "127.0.0.1");
        assert_eq!(summary("абвгд", 3), "абв…");
    }
}
