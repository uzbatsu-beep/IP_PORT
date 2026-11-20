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

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_grouped = app.view_mode == crate::app::ResultsViewMode::GroupedByHost;
            if ui
                .selectable_label(is_grouped, "🗂 By Host")
                .on_hover_text("Group results by host in a collapsible tree")
                .clicked()
            {
                app.view_mode = crate::app::ResultsViewMode::GroupedByHost;
            }
            if ui
                .selectable_label(!is_grouped, "☰ Flat")
                .on_hover_text("Show all ports in a flat table")
                .clicked()
            {
                app.view_mode = crate::app::ResultsViewMode::Flat;
            }
        });
    });
    ui.separator();

    if let Some(stats) = app.current_rtt_stats() {
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

    if app.view_mode == crate::app::ResultsViewMode::GroupedByHost {
        render_grouped_by_host(app, ui);
    } else {
        render_flat_table(app, ui);
    }

    if let Some(selected_idx) = app.selected_result {
        crate::ui::quick_actions::show_inspector_toolbar(app, selected_idx, ui);
    }
}

fn render_flat_table(app: &mut App, ui: &mut Ui) {
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
                    let info = app.host_info.get(&entry.raw.ip).cloned();
                    let (_, address_response) = row.col(|ui| {
                        ui.horizontal(|ui| {
                            ui.monospace(&entry.ip_text);
                            if let Some(hostname) = info.as_ref().and_then(|i| i.hostname.as_deref()) {
                                ui.label(RichText::new(hostname).small().weak());
                            }
                        });
                    });
                    if let Some(info) = &info {
                        let ptr_latency = info.ptr_duration_ms
                            .map(|ms| format!("{ms} ms"))
                            .unwrap_or_else(|| "N/A".to_owned());
                        let ptr_error = match &info.ptr_status {
                            crate::host_info::PtrStatus::Failed(reason) =>
                                format!("\nPTR error: {reason}"),
                            _ => String::new(),
                        };
                        let cache_error = info.neighbor_error.as_deref()
                            .map(|reason| format!("\nNeighbor-cache error: {reason}"))
                            .unwrap_or_default();
                        let tooltip = format!(
                            "PTR: {}\nPTR status: {} ({}){}\nCached MAC: {}\nOUI: {}\nVendor: {}{}",
                            info.hostname.as_deref().unwrap_or("N/A"),
                            info.ptr_status.summary(),
                            ptr_latency,
                            ptr_error,
                            info.mac.as_deref().unwrap_or("Not in local cache"),
                            info.oui.as_deref().unwrap_or("N/A"),
                            info.vendor.as_deref().unwrap_or("Unknown"),
                            cache_error,
                        );
                        address_response.clone().on_hover_text(tooltip);
                    }
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
                                ui.label(RichText::new(entry.raw.service.as_str()).strong());
                            });
                        } else {
                            ui.label(entry.raw.service.as_str());
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

    if app.sort_column != sort_column || app.sort_ascending != sort_ascending {
        app.sort_column = sort_column;
        app.sort_ascending = sort_ascending;
    }
}

fn render_grouped_by_host(app: &mut App, ui: &mut Ui) {
    let host_count = app.host_groups().len();
    let total_ports: usize = app.host_groups().iter().map(|h| h.ports_count).sum();

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{host_count} host{}, {total_ports} open port{}",
                if host_count == 1 { "" } else { "s" },
                if total_ports == 1 { "" } else { "s" }
            ))
            .small()
            .weak(),
        );
    });
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let host_groups = app.host_groups().to_vec();
            for group in &host_groups {
                let info = app.host_info.get(&group.ip);
                let display_host = info
                    .and_then(|i| i.hostname.as_deref())
                    .map(|name| format!("{} ({name})", group.ip_text))
                    .unwrap_or_else(|| group.ip_text.clone());
                let identity = info
                    .and_then(|i| i.mac.as_deref())
                    .map(|mac| format!(" · MAC {mac}"))
                    .unwrap_or_default();
                let latency_text = if group.has_measured_rtt {
                    format!("{:.2} ms", group.min_rtt.as_secs_f64() * 1000.0)
                } else {
                    "N/A".to_owned()
                };
                let header_title = format!(
                    "🖥  {} — {} port{} ({}) · {}",
                    display_host,
                    group.ports_count,
                    if group.ports_count == 1 { "" } else { "s" },
                    group.services_summary,
                    latency_text
                ) + &identity;

                egui::CollapsingHeader::new(RichText::new(header_title).strong())
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.small_button("📋 Copy IP").clicked() {
                                app.copy_text_with_toast(
                                    ui.ctx(),
                                    group.ip_text.clone(),
                                    format!("Copied {}", group.ip_text),
                                );
                            }
                            if ui.small_button("🔍 Filter").clicked() {
                                app.results_filter = group.ip_text.clone();
                            }
                            if ui.small_button("🎯 Scan Host").clicked() {
                                app.targets_input = group.ip_text.clone();
                                app.ports_input = "1-1024".to_owned();
                                app.show_toast(format!(
                                    "Targets set to {} (ports 1-1024)",
                                    group.ip_text
                                ));
                            }
                        });

                        ui.add_space(2.0);

                        for &entry_idx in &group.indices {
                            let entry = app.row(entry_idx).clone();
                            let is_selected = app.selected_result == Some(entry_idx);

                            let banner_suffix = if let Some(b) = &entry.raw.banner {
                                format!("  —  {b}")
                            } else {
                                String::new()
                            };

                            let port_label = format!(
                                "Port {}/{} · {} · {} · {}{}",
                                entry.port_text,
                                entry.protocol_text,
                                entry.raw.service,
                                entry.state_text,
                                entry.latency_text,
                                banner_suffix
                            );

                            let response = ui.selectable_label(is_selected, port_label);
                            if response.clicked() {
                                app.selected_result = Some(entry_idx);
                            }
                            response
                                .on_hover_text(
                                    "Right-click for quick actions, Enter to open in browser",
                                )
                                .context_menu(|ui| {
                                    crate::ui::quick_actions::show_row_context_menu(
                                        app, &entry, ui,
                                    );
                                });
                        }
                    });
                ui.add_space(4.0);
            }
        });
}
