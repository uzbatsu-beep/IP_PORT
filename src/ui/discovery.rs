//! Discovery modes select configuration only; all validation stays in ScopePreview.
use egui::{RichText, Ui};

use super::preferences::Language;
use crate::app::{App, ScopePreview};
use crate::knowledge_service::{self, Selection};
use crate::scanner::{ScanProtocol, presets};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    General,
    Industrial,
    Custom,
}

#[derive(Default)]
pub struct DiscoveryState {
    pub mode: Mode,
    by_protocol: bool,
    vendor: String,
    family: String,
    protocol_group: String,
}

impl DiscoveryState {
    pub fn custom() -> Self {
        Self {
            mode: Mode::Custom,
            ..Self::default()
        }
    }
}

pub fn show_mode(app: &mut App, ui: &mut Ui, lang: Language) {
    ui.horizontal_wrapped(|ui| {
        for (mode, name) in [
            (Mode::General, lang.text("General", "Общий")),
            (Mode::Industrial, lang.text("Industrial", "Промышленный")),
            (Mode::Custom, lang.text("Custom", "Ручной")),
        ] {
            ui.selectable_value(&mut app.workspace.discovery.mode, mode, name);
        }
    });
    ui.label(RichText::new(lang.text(
        "Switching modes does not change scan inputs.",
        "Смена режима не изменяет параметры сканирования.",
    )).small().weak());
}

pub fn show_presets(app: &mut App, ui: &mut Ui, lang: Language) {
    match app.workspace.discovery.mode {
        Mode::General => {
            ui.horizontal_wrapped(|ui| {
                let append = ui.input(|i| i.modifiers.command || i.modifiers.ctrl || i.modifiers.shift);
                for preset in presets::presets_for_protocol(app.protocol) {
                    let active = presets::ports_input_matches(&app.ports_input, preset.ports);
                    if ui.add(egui::Button::new(preset.name).selected(active))
                        .on_hover_text(format!("{}\n{}\nCtrl/Shift: append", preset.description, preset.ports))
                        .clicked()
                    {
                        app.ports_input = presets::apply_preset(&app.ports_input, preset.ports, append);
                    }
                }
            });
        }
        Mode::Industrial => industrial(app, ui, lang),
        Mode::Custom => {
            ui.label(RichText::new(lang.text("Manual port list", "Ручной список портов")).small().weak());
        }
    }
}

fn industrial(app: &mut App, ui: &mut Ui, lang: Language) {
    let Ok(data) = knowledge_service::catalog() else {
        ui.label(lang.text("Catalog unavailable", "Каталог недоступен"));
        return;
    };
    let state = &mut app.workspace.discovery;
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut state.by_protocol, false, lang.text("Vendor", "Производитель"));
        ui.selectable_value(&mut state.by_protocol, true, lang.text("Protocol", "Протокол"));
    });
    let selection = if state.by_protocol {
        if state.protocol_group.is_empty() {
            state.protocol_group = knowledge_service::PROTOCOL_GROUPS[0].0.to_owned();
        }
        egui::ComboBox::from_id_salt("industrial_protocol")
            .selected_text(&state.protocol_group)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for &(name, _) in knowledge_service::PROTOCOL_GROUPS {
                    ui.selectable_value(&mut state.protocol_group, name.to_owned(), name);
                }
            });
        Selection::Protocol(&state.protocol_group)
    } else {
        if state.vendor.is_empty() {
            state.vendor = "Weintek".to_owned();
        }
        let previous = state.vendor.clone();
        egui::ComboBox::from_id_salt("industrial_vendor")
            .selected_text(&state.vendor)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for name in knowledge_service::vendors(data) {
                    ui.selectable_value(&mut state.vendor, name.to_owned(), name);
                }
            });
        if previous != state.vendor {
            state.family.clear();
        }
        let all = lang.text("All catalog families", "Все семейства каталога");
        egui::ComboBox::from_id_salt("industrial_family")
            .selected_text(if state.family.is_empty() { all } else { &state.family })
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.family, String::new(), all);
                for name in knowledge_service::families(data, &state.vendor) {
                    ui.selectable_value(&mut state.family, name.to_owned(), name);
                }
            });
        Selection::Vendor {
            vendor: &state.vendor,
            family: (!state.family.is_empty()).then_some(state.family.as_str()),
        }
    };
    let result = knowledge_service::candidates(selection, app.protocol);
    let candidates = match result {
        Ok(value) => value,
        Err(error) => {
            ui.label(RichText::new(error).color(super::theme::error(ui)));
            return;
        }
    };
    if candidates.ports.is_empty() {
        ui.label(lang.text("No reviewed candidates for this selection/transport.", "Нет проверенных портов-кандидатов для выбранного семейства/транспорта."));
        return;
    }
    let ports = candidates.port_text();
    ui.label(format!("{}: {ports}", lang.text("Port candidates", "Порты-кандидаты")));
    ui.label(RichText::new(lang.text(
        "Optional services; an open port does not confirm the protocol or model.",
        "Сервисы могут быть отключены; открытый порт не подтверждает протокол или модель.",
    )).small().weak());
    if app.protocol == ScanProtocol::Udp {
        ui.label(RichText::new(lang.text(
            "Generic UDP probe is not protocol discovery. Silence is inconclusive.",
            "Универсальный UDP-запрос не идентифицирует протокол. Отсутствие ответа ничего не доказывает.",
        )).color(super::theme::warning(ui)).small());
    }
    egui::CollapsingHeader::new(lang.text("Evidence / sources", "Источники и ограничения"))
        .show(ui, |ui| {
            for entry in candidates.entries {
                ui.push_id(&entry.id, |ui| {
                    ui.label(format!("{} · {} · {}", entry.family, entry.purpose, entry.ports));
                    ui.label(RichText::new(&entry.notes).small().weak());
                    ui.hyperlink_to(lang.text("Source", "Источник"), &entry.source);
                });
            }
        });
    ui.horizontal_wrapped(|ui| {
        if ui.button(lang.text("Replace ports", "Заменить порты")).clicked() {
            app.ports_input = ports.clone();
        }
        let can_append = crate::scanner::targets::parse_ports(&app.ports_input).is_ok()
            || app.ports_input.trim().is_empty();
        if ui.add_enabled(can_append, egui::Button::new(lang.text("Append", "Добавить"))).clicked() {
            app.ports_input = presets::apply_preset(&app.ports_input, &ports, true);
        }
    });
}

fn timeout_estimate_seconds(scope: ScopePreview, concurrency: usize, timeout_ms: u64) -> f64 {
    scope.probes.div_ceil(concurrency.max(1)) as f64 * timeout_ms as f64 / 1000.0
}

pub fn show_scope(app: &App, preview: &Result<ScopePreview, String>, ui: &mut Ui, lang: Language) {
    super::components::section_heading(ui, lang.text("Scan scope", "Область сканирования"));
    if app.is_running() {
        ui.label(lang.text("Active scan — actual counters below", "Активное сканирование — фактические счётчики ниже"));
        return;
    }
    match preview {
        Ok(scope) => {
            egui::Grid::new("discovery_scope").num_columns(2).show(ui, |ui| {
                for (name, count) in [
                    (lang.text("Targets", "Цели"), scope.targets),
                    (lang.text("Hostnames", "Имена узлов"), scope.hostnames),
                    (lang.text("Excluded IPs", "Исключённые IP"), scope.excluded),
                    (lang.text("Ports", "Порты"), scope.ports),
                    (lang.text("Planned probes", "План проверок"), scope.probes),
                ] {
                    super::components::metric_row(ui, name, count, false);
                }
            });
            ui.label(format!("{}: {:.1} s", lang.text("Timeout-only estimate", "Оценка по тайм-аутам, с"), timeout_estimate_seconds(*scope, app.concurrency, app.timeout_ms)));
            ui.label(RichText::new(lang.text(
                "Approximate; excludes DNS, banner reads and scheduling overhead.",
                "Приблизительно; без DNS, баннеров и накладных расходов планирования.",
            )).small().weak());
            if scope.hostnames > 0 {
                ui.label(lang.text("DNS resolution may change the final scope.", "DNS-разрешение может изменить итоговый объём."));
            }
        }
        Err(error) => {
            ui.label(RichText::new(error).color(super::theme::error(ui)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_preview_remains_single_source_of_truth() {
        let mut app = App::default_state();
        app.targets_input = "192.0.2.1-192.0.2.3".to_owned();
        app.exclusions_input = "192.0.2.2".to_owned();
        app.ports_input = "102,502".to_owned();
        let scope = app.refresh_input_preview().unwrap();
        assert_eq!(scope.targets, 2);
        assert_eq!(scope.excluded, 1);
        assert_eq!(scope.probes, 4);
        assert_eq!(timeout_estimate_seconds(scope, 3, 500), 1.0);
        assert!(app.can_start_scan());
        app.workspace.discovery.mode = Mode::Industrial;
        assert_eq!(app.refresh_input_preview().unwrap(), scope);
        app.exclusions_input = "192.0.2.1-192.0.2.3".to_owned();
        assert!(app.refresh_input_preview().is_err());
        assert!(!app.can_start_scan());
    }

    #[test]
    fn render_all_discovery_modes_without_mutating_inputs() {
        let mut app = App::default_state();
        let original = app.ports_input.clone();
        for mode in [Mode::General, Mode::Industrial, Mode::Custom] {
            app.workspace.discovery.mode = mode;
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                show_mode(&mut app, ui, Language::Russian);
                show_presets(&mut app, ui, Language::Russian);
            });
            output.textures_delta.clear();
            assert_eq!(app.ports_input, original);
        }
    }
}
