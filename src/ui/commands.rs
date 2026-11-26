//! Shared availability and dispatch for application-level commands.
use super::{navigation::Section, theme::Theme};
use crate::app::App;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCommand {
    StartScan,
    StopScan,
    LoadProfile,
    SaveProfile,
    ExportCsv,
    ExportJson,
    ExportHtml,
    OpenSettings,
    ToggleTheme,
}

pub fn available(app: &mut App, action: AppCommand) -> bool {
    match action {
        AppCommand::StartScan => app.can_start_scan(),
        AppCommand::StopScan => app.is_running() && !app.is_stopping(),
        AppCommand::LoadProfile | AppCommand::SaveProfile => !app.is_running(),
        AppCommand::ExportCsv | AppCommand::ExportJson | AppCommand::ExportHtml => {
            !app.results.is_empty() && !app.is_exporting()
        }
        AppCommand::OpenSettings | AppCommand::ToggleTheme => true,
    }
}

/// Recheck availability at dispatch time, even if the caller already did so.
/// Cancellation of a native file dialog is not an error and changes no state.
pub fn execute(app: &mut App, ctx: &egui::Context, action: AppCommand) {
    if !available(app, action) {
        return;
    }
    match action {
        AppCommand::StartScan => app.start_scan(),
        AppCommand::StopScan => app.stop_scan(),
        AppCommand::LoadProfile => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("JSON profile", &["json"])
                .pick_file()
            {
                app.load_profile(&path);
            }
        }
        AppCommand::SaveProfile => {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("scan_profile.json")
                .add_filter("JSON profile", &["json"])
                .save_file()
            {
                app.save_profile(&path);
            }
        }
        AppCommand::ExportCsv => app.export_csv(),
        AppCommand::ExportJson => app.export_json(),
        AppCommand::ExportHtml => app.export_html(),
        AppCommand::OpenSettings => app.workspace.section = Section::Settings,
        AppCommand::ToggleTheme => {
            app.appearance.theme = match app.appearance.theme {
                Theme::Light => Theme::Dark,
                Theme::Dark => Theme::Light,
            };
            app.save_appearance(ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_scope_and_empty_export_are_disabled() {
        let mut app = App::default_state();
        assert!(available(&mut app, AppCommand::StartScan));
        app.targets_input.clear();
        assert!(!available(&mut app, AppCommand::StartScan));
        for action in [
            AppCommand::ExportCsv,
            AppCommand::ExportJson,
            AppCommand::ExportHtml,
        ] {
            assert!(!available(&mut app, action));
        }
        assert!(available(&mut app, AppCommand::LoadProfile));
        assert!(available(&mut app, AppCommand::SaveProfile));
        assert!(available(&mut app, AppCommand::ToggleTheme));
    }
}
