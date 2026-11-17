use egui::{Align, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use crate::app::{App, SortColumn};

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.heading(RichText::new("Results").strong());
        ui.label(
            RichText::new(format!("{} stored", app.results.len()))
                .weak()
                .small(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let clear_hint = if app.is_running() {
                "Clear collected results so far; newly discovered open ports will continue to appear"
            } else {
                "Clear collected results"
            };
            if ui
                .add_enabled(!app.results.is_empty(), egui::Button::new("Clear"))
                .on_hover_text(clear_hint)
                .clicked()
            {
                app.clear_results();
            }
        });
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Filter").weak());
        let filter_response = ui.add(
            egui::TextEdit::singleline(&mut app.results_filter)
                .id(egui::Id::unique("results_filter"))
                .desired_width(240.0)
                .hint_text("Address, port or service"),
        );
        if (filter_response.has_focus() || filter_response.lost_focus())
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            app.results_filter.clear();
        }
        if !app.results_filter.is_empty() && ui.small_button("Clear filter").clicked() {
            app.results_filter.clear();
        }

        app.ensure_results_view();
        let visible_count = app.visible_indices().len();
        ui.label(
            RichText::new(format!("{visible_count} shown"))
                .weak()
                .small(),
        );
    });
    ui.separator();

    if let Some(stats) = crate::ui::rtt_view::calculate_rtt_stats(&app.results) {
        let header_title = format!(
            "Latency distribution — Min: {:.2} ms | Avg: {:.2} ms | Median: {:.2} ms | Max: {:.2} ms",
            stats.min_ms, stats.avg_ms, stats.median_ms, stats.max_ms
        );
        egui::CollapsingHeader::new(RichText::new(header_title).small().strong())
            .default_open(true)
            .show(ui, |ui| {
                crate::ui::rtt_view::show_rtt_panel(app, &stats, ui);
            });
        ui.add_space(4.0);
    }

    app.ensure_results_view();
    if app.visible_indices().is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            let message = if app.results.is_empty() {
                if app.is_stopping() {
                    "Stopping scan..."
                } else if app.is_running() {
                    "Scanning... open ports will appear here"
                } else {
                    "No results yet — configure targets on the left and press Start scan"
                }
            } else {
                "No results match the current filter"
            };
            ui.label(RichText::new(message).weak());
        });
        return;
    }

    let mut sort_column = app.sort_column;
    let mut sort_ascending = app.sort_ascending;
    let headers = ["Address", "Port", "Service", "Latency"];
    let columns = [
        SortColumn::Address,
        SortColumn::Port,
        SortColumn::Service,
        SortColumn::Latency,
    ];
    let row_height = 22.0;
    let visible_len = app.visible_indices().len();

    ui.scope(|ui| {
        // Убираем вертикальные разделители колонок в таблице результатов (визуальная чистота)
        ui.visuals_mut().widgets.noninteractive.bg_stroke = egui::Stroke::NONE;

        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(160.0).at_least(120.0))
            .column(Column::initial(80.0).at_least(60.0))
            .column(Column::remainder().at_least(140.0))
            .column(Column::initial(110.0).at_least(90.0));

        if app.scroll_to_selected {
            app.scroll_to_selected = false;
            if let Some(target_idx) = app.selected_result
                && let Some(target_pos) = app
                    .visible_indices()
                    .iter()
                    .position(|&idx| idx == target_idx)
            {
                builder = builder.scroll_to_row(target_pos, Some(Align::Center));
            }
        }

        builder
            .header(row_height, |mut header| {
                for (title, column) in headers.into_iter().zip(columns) {
                    header.col(|ui| {
                        let indicator = if sort_column == column {
                            if sort_ascending { " ↑" } else { " ↓" }
                        } else {
                            " ↕"
                        };
                        let response = ui
                            .selectable_label(
                                sort_column == column,
                                RichText::new(format!("{title}{indicator}"))
                                    .strong()
                                    .small(),
                            )
                            .on_hover_text(format!("Sort by {title}"));
                        if response.clicked() {
                            if sort_column == column {
                                sort_ascending = !sort_ascending;
                            } else {
                                sort_column = column;
                                sort_ascending = true;
                            }
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(row_height, visible_len, |mut row| {
                    let entry_idx = app.visible_indices()[row.index()];
                    let entry = app.row(entry_idx).clone();
                    row.set_selected(app.selected_result == Some(entry_idx));
                    let (_, address_response) = row.col(|ui| {
                        ui.monospace(&entry.ip_text);
                    });
                    if address_response.clicked() {
                        app.selected_result = Some(entry_idx);
                    }
                    row.col(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let proto_suffix = match entry.raw.protocol {
                                crate::scanner::ScanProtocol::Tcp => "",
                                crate::scanner::ScanProtocol::Udp => "/udp",
                            };
                            ui.label(
                                RichText::new(format!("{}{proto_suffix}", entry.port_text))
                                    .monospace(),
                            );
                        });
                    });
                    let (_, service_response) = row.col(|ui| {
                        if entry.raw.state == crate::scanner::PortState::OpenFiltered {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("?").color(crate::ui::theme::WARNING).small(),
                                );
                                ui.label(
                                    RichText::new(format!("{} (open|filtered)", entry.raw.service))
                                        .color(crate::ui::theme::WARNING),
                                );
                            });
                        } else if entry.raw.service_verified {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("✔").color(crate::ui::theme::SUCCESS).small(),
                                );
                                ui.label(RichText::new(entry.raw.service).strong());
                            });
                        } else {
                            ui.label(entry.raw.service);
                        }
                    });
                    if entry.raw.state == crate::scanner::PortState::OpenFiltered {
                        service_response.on_hover_text(
                            "UDP Open|Filtered: no response received within timeout.\nPort may be filtered by firewall or service is silent.",
                        );
                    } else if entry.raw.service_verified {
                        let hint = if let Some(banner) = &entry.raw.banner {
                            format!("Service confirmed by banner:\n{banner}")
                        } else {
                            "Service confirmed by response datagram".to_owned()
                        };
                        service_response.on_hover_text(hint);
                    } else if let Some(banner) = &entry.raw.banner {
                        service_response.on_hover_text(format!(
                            "Port-based guess (banner unconfirmed):\n{banner}"
                        ));
                    } else {
                        service_response.on_hover_text("Port-based guess (no banner confirmed)");
                    }
                    let (_, latency_response) = row.col(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(&entry.latency_text).monospace());
                        });
                    });
                    let latency_hint = match entry.raw.protocol {
                        crate::scanner::ScanProtocol::Tcp => "TCP connect time, not ICMP ping",
                        crate::scanner::ScanProtocol::Udp => "UDP round-trip time",
                    };
                    latency_response.on_hover_text(latency_hint);
                    if row.response().clicked() || row.response().secondary_clicked() {
                        app.selected_result = Some(entry_idx);
                    }
                    row.response()
                        .on_hover_text("Right-click for quick actions")
                        .context_menu(|ui| {
                            crate::ui::quick_actions::show_row_context_menu(app, &entry, ui);
                        });
                });
            });
    });

    if let Some(selected_idx) = app.selected_result {
        crate::ui::quick_actions::show_inspector_toolbar(app, selected_idx, ui);
    }

    if app.sort_column != sort_column || app.sort_ascending != sort_ascending {
        app.sort_column = sort_column;
        app.sort_ascending = sort_ascending;
    }
}
