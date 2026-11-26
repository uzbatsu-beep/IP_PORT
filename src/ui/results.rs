use egui::{Align, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::app::{App, DeviceRow, HostSort, ResultFilters, ResultsViewMode, SortColumn};
use crate::scanner::{PortState, ScanProtocol};
use crate::ui::{
    components::{self, Tone},
    preferences::Language,
};

pub fn show(app: &mut App, ui: &mut Ui) {
    let lang = app.workspace.language;
    ui.horizontal_wrapped(|ui| {
        components::section_heading(ui, lang.text("Results", "Результаты"));
        let previous = app.view_mode;
        ui.selectable_value(
            &mut app.view_mode,
            ResultsViewMode::GroupedByHost,
            lang.text("Devices", "Устройства"),
        );
        ui.selectable_value(
            &mut app.view_mode,
            ResultsViewMode::Flat,
            lang.text("Ports", "Порты"),
        );
        if previous != app.view_mode {
            app.scroll_to_selected = true;
        }
        if ui
            .add_enabled(
                !app.results.is_empty(),
                components::secondary_button(lang.text("Clear results", "Очистить результаты")),
            )
            .on_hover_text(lang.text(
                "Clears stored results, not the scan",
                "Удаляет накопленные результаты, не останавливая сканирование",
            ))
            .clicked()
        {
            app.clear_results();
        }
    });
    ui.horizontal_wrapped(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.results_filter)
                .id(egui::Id::unique("results_filter"))
                .desired_width(220.0)
                .hint_text(lang.text(
                    "IP, hostname, MAC, port, service",
                    "IP, имя, MAC, порт, сервис",
                )),
        );
        if (response.has_focus() || response.lost_focus())
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            app.results_filter.clear();
        }
        if ui
            .small_button(lang.text("Reset filters", "Сброс фильтров"))
            .clicked()
        {
            app.results_filter.clear();
            app.filters = ResultFilters::default();
        }
    });
    egui::CollapsingHeader::new(lang.text("Filters", "Фильтры"))
        .default_open(false)
        .show(ui, |ui| {
            filters(app, ui, lang);
        });
    app.ensure_results_view();
    ui.horizontal_wrapped(|ui| {
        ui.label(format!(
            "{}: {} · {}: {}/{}",
            lang.text("Matching hosts", "Узлы в выборке"),
            app.host_groups().len(),
            lang.text("Endpoints shown", "Порты в выборке"),
            app.visible_indices().len(),
            app.results.len()
        ));
        if app.view_mode == ResultsViewMode::GroupedByHost
            && ui
                .small_button(lang.text("Collapse all", "Свернуть все"))
                .clicked()
        {
            app.collapse_hosts();
        }
        if let Some(index) = app.selected_result
            && !app.visible_indices().contains(&index)
        {
            ui.label(
                RichText::new(lang.text(
                    "Selected endpoint outside filter",
                    "Выбранный порт вне фильтра",
                ))
                .weak()
                .small(),
            );
        }
    });
    ui.separator();
    // RTT distribution is available in Diagnostics; leave it collapsed here.
    if let Some(stats) = app.current_rtt_stats() {
        egui::CollapsingHeader::new(lang.text("Latency distribution", "Распределение RTT"))
            .default_open(false)
            .show(ui, |ui| {
                crate::ui::rtt_view::show_rtt_panel(app, &stats, ui)
            });
    }
    if app.visible_indices().is_empty() {
        let msg = if app.results.is_empty() {
            if app.is_stopping() {
                lang.text("Stopping…", "Остановка…")
            } else if app.is_running() {
                lang.text("Scanning…", "Сканирование…")
            } else {
                lang.text(
                    "Configure targets and press Start",
                    "Настройте цели и нажмите Пуск",
                )
            }
        } else {
            lang.text(
                "No matches. Reset filters to show results.",
                "Нет совпадений. Сбросьте фильтры.",
            )
        };
        components::empty_state(ui, msg);
        return;
    }
    // Reserve space for the selected-endpoint actions below the scrollable table.
    if let Some(index) = app.selected_result.filter(|_| !app.selected_host_only) {
        crate::ui::quick_actions::show_inspector_toolbar(app, index, ui);
    }
    if app.view_mode == ResultsViewMode::GroupedByHost {
        app.ensure_device_rows();
        render_devices(app, ui, lang);
    } else {
        render_ports(app, ui, lang);
    }
}

fn filters(app: &mut App, ui: &mut Ui, lang: Language) {
    ui.horizontal_wrapped(|ui| {
        ui.label(lang.text("OUI owner", "Владелец OUI"));
        ui.add(egui::TextEdit::singleline(&mut app.filters.vendor).desired_width(120.0));
        ui.label(lang.text("Service", "Сервис"));
        ui.add(egui::TextEdit::singleline(&mut app.filters.service).desired_width(100.0));
    });
    ui.horizontal_wrapped(|ui| {
        let all = lang.text("All", "Все");
        egui::ComboBox::from_id_salt("filter_transport")
            .selected_text(app.filters.transport.map_or(all, ScanProtocol::as_str))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.filters.transport, None, all);
                for protocol in [ScanProtocol::Tcp, ScanProtocol::Udp] {
                    ui.selectable_value(
                        &mut app.filters.transport,
                        Some(protocol),
                        protocol.as_str(),
                    );
                }
            });
        egui::ComboBox::from_id_salt("filter_state")
            .selected_text(app.filters.state.map_or(all, PortState::as_str))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.filters.state, None, all);
                for state in [PortState::Open, PortState::OpenFiltered] {
                    ui.selectable_value(&mut app.filters.state, Some(state), state.as_str());
                }
            });
        egui::ComboBox::from_id_salt("filter_evidence")
            .selected_text(match app.filters.evidence {
                None => lang.text("Any evidence", "Любая достоверность"),
                Some(true) => lang.text("Service verified", "Сервис подтверждён"),
                Some(false) => lang.text("Unverified", "Не подтверждён"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.filters.evidence,
                    None,
                    lang.text("Any evidence", "Любая достоверность"),
                );
                ui.selectable_value(
                    &mut app.filters.evidence,
                    Some(true),
                    lang.text("Service verified", "Сервис подтверждён"),
                );
                ui.selectable_value(
                    &mut app.filters.evidence,
                    Some(false),
                    lang.text("Unverified", "Не подтверждён"),
                );
            });
    });
    ui.label(
        RichText::new(lang.text(
            "Filters combine with AND; export still includes all stored endpoints.",
            "Фильтры объединяются по И; экспорт включает все накопленные порты.",
        ))
        .small()
        .weak(),
    );
}

fn header(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).strong().small());
}
fn cell(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(text).truncate())
        .on_hover_text(text);
}
fn endpoint_cells(
    row: &mut egui_extras::TableRow<'_, '_>,
    entry: &crate::app::ResultRow,
    lang: Language,
) {
    row.col(|ui| {
        cell(ui, &entry.port_text);
    });
    row.col(|ui| {
        cell(ui, entry.protocol_text);
    });
    row.col(|ui| {
        let tone = if entry.raw.state == PortState::OpenFiltered {
            Tone::Warning
        } else {
            Tone::Success
        };
        ui.label(components::semantic_text(ui, entry.state_text, tone))
            .on_hover_text(lang.text(
                "Open|Filtered: no UDP response; not proof of an open port.",
                "Open|Filtered: UDP не ответил; открытый порт не подтверждён.",
            ));
    });
    row.col(|ui| {
        cell(ui, &entry.raw.service);
    });
    row.col(|ui| {
        let verified = entry.raw.service_verified && entry.raw.state == PortState::Open;
        let label = if verified {
            lang.text("Verified", "Подтверждён")
        } else {
            lang.text("Unverified", "Не подтверждён")
        };
        ui.label(
            components::semantic_text(
                ui,
                label,
                if verified {
                    Tone::Success
                } else {
                    Tone::Neutral
                },
            )
            .small(),
        )
        .on_hover_text(entry.raw.banner.as_deref().unwrap_or(lang.text(
            "Service evidence, not device identity",
            "Признак сервиса, не идентификация устройства",
        )));
    });
    row.col(|ui| {
        cell(ui, &entry.latency_text);
    });
}

fn render_ports(app: &mut App, ui: &mut Ui, lang: Language) {
    let mut sort = app.sort_column;
    let mut ascending = app.sort_ascending;
    let len = app.visible_indices().len();
    egui::ScrollArea::horizontal()
        .id_salt("ports_horizontal")
        .show(ui, |ui| {
            ui.set_min_width(700.0);
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(Align::Center))
                .column(Column::initial(145.0).at_least(105.0))
                .column(Column::initial(55.0).at_least(45.0))
                .column(Column::initial(65.0).at_least(55.0))
                .column(Column::initial(105.0).at_least(80.0))
                .column(Column::remainder().at_least(100.0))
                .column(Column::initial(115.0).at_least(95.0))
                .column(Column::initial(80.0).at_least(65.0));
            if app.scroll_to_selected {
                app.scroll_to_selected = false;
                if let Some(pos) = app
                    .selected_result
                    .and_then(|idx| app.visible_indices().iter().position(|&i| i == idx))
                {
                    table = table.scroll_to_row(pos, Some(Align::Center));
                }
            }
            table
                .header(components::TABLE_ROW_HEIGHT, |mut row| {
                    for (name, key) in [
                        (lang.text("Address", "Адрес"), Some(SortColumn::Address)),
                        (lang.text("Port", "Порт"), Some(SortColumn::Port)),
                        (lang.text("Transport", "Транспорт"), None),
                        (lang.text("State", "Состояние"), None),
                        (lang.text("Service", "Сервис"), Some(SortColumn::Service)),
                        (lang.text("Evidence", "Достоверность"), None),
                        ("RTT", Some(SortColumn::Latency)),
                    ] {
                        row.col(|ui| {
                            if let Some(key) = key {
                                let suffix = if sort == key {
                                    if ascending { " ↑" } else { " ↓" }
                                } else {
                                    ""
                                };
                                if ui
                                    .selectable_label(sort == key, format!("{name}{suffix}"))
                                    .clicked()
                                {
                                    ascending = if sort == key { !ascending } else { true };
                                    sort = key;
                                }
                            } else {
                                header(ui, name);
                            }
                        });
                    }
                })
                .body(|body| {
                    body.rows(components::TABLE_ROW_HEIGHT, len, |mut row| {
                        let index = app.visible_indices()[row.index()];
                        let entry = app.row(index).clone();
                        row.set_selected(
                            app.selected_result == Some(index) && !app.selected_host_only,
                        );
                        row.col(|ui| {
                            cell(ui, &entry.ip_text);
                        });
                        endpoint_cells(&mut row, &entry, lang);
                        if row.response().clicked() || row.response().secondary_clicked() {
                            app.select_endpoint(index);
                        }
                        row.response().context_menu(|ui| {
                            crate::ui::quick_actions::show_row_context_menu(app, &entry, ui)
                        });
                    });
                });
        });
    app.sort_column = sort;
    app.sort_ascending = ascending;
}

fn render_devices(app: &mut App, ui: &mut Ui, lang: Language) {
    let len = app.device_rows().len();
    let mut sort = app.host_sort;
    let mut ascending = app.host_sort_ascending;
    egui::ScrollArea::horizontal()
        .id_salt("devices_horizontal")
        .show(ui, |ui| {
            ui.set_min_width(850.0);
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .sense(egui::Sense::click())
                .cell_layout(egui::Layout::left_to_right(Align::Center))
                .column(Column::initial(175.0).at_least(130.0))
                .column(Column::initial(125.0).at_least(100.0))
                .column(Column::initial(125.0).at_least(100.0))
                .column(Column::initial(70.0).at_least(55.0))
                .column(Column::remainder().at_least(140.0))
                .column(Column::initial(85.0).at_least(65.0))
                .column(Column::initial(120.0).at_least(95.0));
            if app.scroll_to_selected {
                app.scroll_to_selected = false;
                if let Some(pos) = app.selected_device_position() {
                    table = table.scroll_to_row(pos, Some(Align::Center));
                }
            }
            table
                .header(components::TABLE_ROW_HEIGHT, |mut row| {
                    for (name, key) in [
                        (
                            lang.text("IP / Hostname", "IP / Имя узла"),
                            Some(HostSort::Address),
                        ),
                        ("MAC", None),
                        (
                            lang.text("OUI owner", "Владелец OUI"),
                            Some(HostSort::Vendor),
                        ),
                        (lang.text("Ports", "Порты"), Some(HostSort::Endpoints)),
                        (lang.text("Services", "Сервисы"), None),
                        ("Min RTT", Some(HostSort::Rtt)),
                        (lang.text("Evidence", "Достоверность"), None),
                    ] {
                        row.col(|ui| {
                            if let Some(key) = key {
                                let suffix = if sort == key {
                                    if ascending { " ↑" } else { " ↓" }
                                } else {
                                    ""
                                };
                                if ui
                                    .selectable_label(sort == key, format!("{name}{suffix}"))
                                    .clicked()
                                {
                                    ascending = if sort == key { !ascending } else { true };
                                    sort = key;
                                }
                            } else {
                                header(ui, name);
                            }
                        });
                    }
                })
                .body(|body| {
                    body.rows(components::TABLE_ROW_HEIGHT, len, |mut row| {
                        match app.device_rows()[row.index()] {
                            DeviceRow::Host(group_index) => {
                                let group = &app.host_groups()[group_index];
                                let ip = group.ip;
                                let ip_text = group.ip_text.clone();
                                let count = group.ports_count;
                                let services = group.services_summary.clone();
                                let rtt = if group.has_measured_rtt {
                                    format!("{:.2} ms", group.min_rtt.as_secs_f64() * 1000.0)
                                } else {
                                    "—".to_owned()
                                };
                                let info = app.host_info.get(&ip);
                                let name = info
                                    .and_then(|i| i.hostname.as_deref())
                                    .unwrap_or("—")
                                    .to_owned();
                                let mac = info
                                    .and_then(|i| i.mac.as_deref())
                                    .unwrap_or("—")
                                    .to_owned();
                                let vendor = info
                                    .and_then(|i| i.vendor.as_deref())
                                    .unwrap_or("—")
                                    .to_owned();
                                let ambiguous = info.is_some_and(|i| i.neighbor_ambiguous);
                                let selected = app.selected_result.is_some_and(|idx| {
                                    app.results.get(idx).is_some_and(|result| result.ip == ip)
                                });
                                row.set_selected(selected);
                                let mut toggle = false;
                                row.col(|ui| {
                                    ui.horizontal(|ui| {
                                        toggle = ui
                                            .small_button(if app.host_expanded(ip) {
                                                "▾"
                                            } else {
                                                "▸"
                                            })
                                            .clicked();
                                        cell(ui, &format!("{ip_text} {name}"));
                                    });
                                });
                                row.col(|ui| {
                                    cell(ui, &mac);
                                });
                                row.col(|ui| {
                                    cell(ui, &vendor);
                                });
                                row.col(|ui| {
                                    cell(ui, &count.to_string());
                                });
                                row.col(|ui| {
                                    cell(ui, &services);
                                });
                                row.col(|ui| {
                                    cell(ui, &rtt);
                                });
                                row.col(|ui| {
                                    let label = if ambiguous {
                                        lang.text("Ambiguous MAC", "MAC неоднозначен")
                                    } else if vendor != "—" {
                                        lang.text("OUI candidate", "OUI-кандидат")
                                    } else {
                                        lang.text("Unidentified", "Не определён")
                                    };
                                    ui.label(
                                        components::semantic_text(
                                            ui,
                                            label,
                                            if ambiguous {
                                                Tone::Warning
                                            } else {
                                                Tone::Neutral
                                            },
                                        )
                                        .small(),
                                    )
                                    .on_hover_text(lang.text(
                                        "Observed IP, not a verified physical device",
                                        "Наблюдаемый IP, физическое устройство не подтверждено",
                                    ));
                                });
                                if toggle {
                                    app.toggle_host(ip);
                                }
                                if toggle
                                    || row.response().clicked()
                                    || row.response().secondary_clicked()
                                {
                                    app.select_host(group_index);
                                }
                                row.response().context_menu(|ui| {
                                    if ui.button(lang.text("Copy IP", "Копировать IP")).clicked()
                                    {
                                        app.copy_text_with_toast(
                                            ui.ctx(),
                                            ip_text.clone(),
                                            format!("Copied {ip_text}"),
                                        );
                                        ui.close();
                                    }
                                    if ui
                                        .button(lang.text("Filter by IP", "Фильтр по IP"))
                                        .clicked()
                                    {
                                        app.results_filter = ip_text.clone();
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            !app.is_running(),
                                            egui::Button::new(lang.text(
                                                "Set scan target",
                                                "Задать цель сканирования",
                                            )),
                                        )
                                        .clicked()
                                    {
                                        app.targets_input = ip_text.clone();
                                        app.ports_input = "1-1024".to_owned();
                                        ui.close();
                                    }
                                });
                            }
                            DeviceRow::Endpoint(index) => {
                                let entry = app.row(index).clone();
                                row.set_selected(
                                    app.selected_result == Some(index) && !app.selected_host_only,
                                );
                                row.col(|ui| {
                                    cell(
                                        ui,
                                        &format!("    {}/{}", entry.port_text, entry.protocol_text),
                                    );
                                });
                                row.col(|ui| {
                                    cell(ui, entry.state_text);
                                });
                                row.col(|ui| {
                                    cell(ui, &entry.raw.service);
                                });
                                row.col(|ui| {
                                    cell(ui, "");
                                });
                                row.col(|ui| {
                                    cell(ui, entry.raw.banner.as_deref().unwrap_or("—"));
                                });
                                row.col(|ui| {
                                    cell(ui, &entry.latency_text);
                                });
                                row.col(|ui| {
                                    let verified = entry.raw.service_verified
                                        && entry.raw.state == PortState::Open;
                                    ui.label(
                                        components::semantic_text(
                                            ui,
                                            if verified {
                                                lang.text("Service verified", "Сервис подтверждён")
                                            } else {
                                                lang.text("Unverified", "Не подтверждён")
                                            },
                                            if verified {
                                                Tone::Success
                                            } else {
                                                Tone::Neutral
                                            },
                                        )
                                        .small(),
                                    );
                                });
                                if row.response().clicked() || row.response().secondary_clicked() {
                                    app.select_endpoint(index);
                                }
                                row.response().context_menu(|ui| {
                                    crate::ui::quick_actions::show_row_context_menu(app, &entry, ui)
                                });
                            }
                        }
                    });
                });
        });
    app.host_sort = sort;
    app.host_sort_ascending = ascending;
}
