use egui::{RichText, Ui};

use crate::app::{App, ScopePreview, StatusLevel};
use crate::scanner::presets;
use crate::ui::{components, theme};

pub fn show_appearance(app: &mut App, ui: &mut Ui) {
    egui::CollapsingHeader::new("Appearance")
        .default_open(true)
        .show(ui, |ui| {
            let previous = app.appearance;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.appearance.theme, theme::Theme::Light, "Light");
                ui.selectable_value(&mut app.appearance.theme, theme::Theme::Dark, "Dark");
            });
            egui::ComboBox::from_id_salt("appearance_palette")
                .selected_text(app.appearance.palette.label())
                .show_ui(ui, |ui| {
                    for palette in theme::Palette::ALL {
                        ui.selectable_value(&mut app.appearance.palette, palette, palette.label());
                    }
                });
            let reset = ui.button("Reset appearance").clicked();
            if reset {
                app.appearance = theme::Appearance::default();
            }
            if previous != app.appearance || reset {
                app.save_appearance(ui.ctx());
            }
            if let Some(error) = &app.appearance_error {
                ui.label(RichText::new(error).color(theme::error(ui)).small());
                if ui.button("Retry save").clicked() {
                    app.save_appearance(ui.ctx());
                }
            }
        });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.add_space(4.0);
    components::section_heading(ui, "IP / PORT Scanner");
    ui.label(RichText::new("Async TCP & UDP port scanner").weak());
    ui.add_space(12.0);

    let running = app.is_running();
    ui.add_enabled_ui(!running, |ui| {
        ui.label(RichText::new("Protocol").strong());
        ui.horizontal(|ui| {
            let tcp_active = app.protocol == crate::scanner::ScanProtocol::Tcp;
            let udp_active = app.protocol == crate::scanner::ScanProtocol::Udp;
            if ui.selectable_label(tcp_active, "TCP").clicked() {
                app.protocol = crate::scanner::ScanProtocol::Tcp;
            }
            if ui.selectable_label(udp_active, "UDP").clicked() {
                app.protocol = crate::scanner::ScanProtocol::Udp;
                if app.concurrency > 64 {
                    app.concurrency = 32;
                }
            }
        });
        if app.protocol == crate::scanner::ScanProtocol::Udp {
            ui.label(
                RichText::new(
                    "UDP is connectionless. Silent ports are marked Open|Filtered (concurrency ≤ 64 recommended).",
                )
                .weak()
                .small(),
            );
        }

        ui.add_space(8.0);
        ui.label(RichText::new("Targets").strong());
        ui.add(
            egui::TextEdit::multiline(&mut app.targets_input)
                .desired_rows(6)
                .desired_width(f32::INFINITY)
                .hint_text("10.0.0.1\n192.168.1.0/24\n10.0.0.1-10.0.0.50"),
        )
        .on_hover_text(
            "One target per line: IPv4/IPv6 address, CIDR subnet or IPv4 range (# comments ignored)",
        );

        ui.add_space(8.0);
        egui::CollapsingHeader::new(RichText::new("Exclude targets (safety scope)").strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut app.exclusions_input)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY)
                        .hint_text("192.168.1.1, 192.168.1.200-254\n10.0.0.0/24\n2001:db8::/64"),
                )
                .on_hover_text(
                    "Excluded IPs, ranges and CIDRs are never probed. Also applies after hostname DNS resolution. Comments start with #.",
                );
                ui.label(
                    RichText::new("Invalid exclusions block scanning. DNS target exclusions apply after A/AAAA lookup.")
                        .small()
                        .weak(),
                );
            });

        ui.add_space(8.0);
        ui.label(RichText::new("Ports").strong());
        ui.add(
            egui::TextEdit::singleline(&mut app.ports_input)
                .desired_width(f32::INFINITY)
                .hint_text("22,80,443 or 1-1024"),
        );

        ui.horizontal_wrapped(|ui| {
            let active_presets = presets::presets_for_protocol(app.protocol);
            let append_mode = ui.input(|i| i.modifiers.command || i.modifiers.ctrl || i.modifiers.shift);
            for preset in active_presets {
                let active = presets::ports_input_matches(&app.ports_input, preset.ports);
                let button = egui::Button::new(preset.name).selected(active);
                let tooltip = format!(
                    "{}\n\nPorts ({}):\n{}\n\nTip: Hold Ctrl or Shift to append without replacing",
                    preset.description,
                    preset.count,
                    preset.ports
                );
                if ui.add(button).on_hover_text(tooltip).clicked() {
                    app.ports_input = presets::apply_preset(&app.ports_input, preset.ports, append_mode);
                }
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Network interface (Source IP)").strong());
            if ui
                .small_button("🔄")
                .on_hover_text("Refresh network interfaces list")
                .clicked()
            {
                app.refresh_interfaces();
            }
        });

        let current_bind = app.bind_addr_input.trim().to_owned();
        let selected_label = if current_bind.is_empty()
            || current_bind.eq_ignore_ascii_case("default")
            || current_bind.eq_ignore_ascii_case("any")
        {
            "Default (Operating system routing)".to_owned()
        } else if let Some(iface) = app
            .interfaces
            .iter()
            .find(|i| i.ip.to_string() == current_bind)
        {
            crate::scanner::interfaces::format_interface_label(iface)
        } else {
            format!("Custom IP: {current_bind}")
        };

        let mut chosen_bind = None;
        egui::ComboBox::from_id_salt("bind_interface_selector")
            .selected_text(selected_label)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                let is_default = current_bind.is_empty()
                    || current_bind.eq_ignore_ascii_case("default")
                    || current_bind.eq_ignore_ascii_case("any");
                if ui
                    .selectable_label(is_default, "Default (Operating system routing)")
                    .clicked()
                {
                    chosen_bind = Some(String::new());
                }
                for iface in &app.interfaces {
                    let ip_str = iface.ip.to_string();
                    let is_active = current_bind == ip_str;
                    let label = crate::scanner::interfaces::format_interface_label(iface);
                    if ui.selectable_label(is_active, label).clicked() {
                        chosen_bind = Some(ip_str);
                    }
                }
            });

        if let Some(new_bind) = chosen_bind {
            app.bind_addr_input = new_bind;
        }

        ui.add(
            egui::TextEdit::singleline(&mut app.bind_addr_input)
                .desired_width(f32::INFINITY)
                .hint_text("Default (Any) or custom IP e.g. 192.168.1.50"),
        )
        .on_hover_text("Select an interface above or manually type an IP to bind outgoing sockets");

        ui.add_space(8.0);
        ui.label(RichText::new("Connection timeout").strong());
        ui.add(
            egui::Slider::new(&mut app.timeout_ms, 50..=5000)
                .suffix(" ms")
                .logarithmic(true),
        );

        ui.add_space(4.0);
        ui.label(RichText::new("Concurrency").strong());
        ui.add(egui::Slider::new(&mut app.concurrency, 1..=1024).logarithmic(true));
        ui.label(RichText::new("Parallel connection attempts").weak().small());

        ui.add_space(8.0);
        ui.checkbox(&mut app.banner_grabbing, "Grab service banners")
            .on_hover_text(
                "Read service greetings or send probe to identify confirmed services (SSH, HTTP, FTP, etc.)",
            );

        if app.banner_grabbing {
            ui.add_space(4.0);
            ui.label(RichText::new("Banner timeout").strong());
            ui.add(
                egui::Slider::new(&mut app.banner_timeout_ms, 100..=5000)
                    .suffix(" ms")
                    .logarithmic(true),
            );
            ui.label(
                RichText::new("Per-port banner read timeout (max 256 bytes)")
                    .weak()
                    .small(),
            );
        }

        ui.add_space(8.0);
        egui::CollapsingHeader::new(RichText::new("Host identification").strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.checkbox(&mut app.resolve_ptr, "Resolve reverse DNS (PTR)")
                    .on_hover_text("Background OS resolver lookup for each discovered unique IP. May contact configured DNS servers.");
                ui.checkbox(&mut app.read_neighbor_cache, "Read local ARP / NDP cache (MAC + OUI)")
                    .on_hover_text("Read-only local operating system neighbor cache; no active discovery packets and no admin access required.");
                ui.label(RichText::new("Neighbor-cache MACs are read-only. IEEE CSV imports are offline and do not send probes.")
                    .small().weak());
                show_oui(app, ui);
                if app.resolve_ptr {
                    ui.add(
                        egui::Slider::new(&mut app.ptr_soft_deadline_ms, 500..=30_000)
                            .text("PTR warning (ms)"),
                    ).on_hover_text("Soft deadline: mark a long-running lookup as overdue. The operating system DNS call is not forcibly stopped.");
                }
                if app.resolve_ptr || app.read_neighbor_cache {
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "Processed: {} · Pending: {} · Backlog: {}",
                            app.host_info_stats.completed,
                            app.host_info_pending_len(),
                            app.host_info_backlog_len(),
                        )).small(),
                    );
                    if app.resolve_ptr {
                        ui.label(
                            RichText::new(format!(
                                "PTR: {} resolved · {} no name · {} errors",
                                app.host_info_stats.ptr_resolved,
                                app.host_info_stats.ptr_no_name,
                                app.host_info_stats.ptr_failed,
                            )).small(),
                        );
                        ui.label(RichText::new(format!(
                            "PTR exceeded warning threshold: {} total · {} still running",
                            app.host_info_stats.ptr_over_limit,
                            app.ptr_currently_overdue(),
                        )).small().color(theme::warning(ui)));
                        ui.label(RichText::new("Soft deadline only; the OS resolver is allowed to finish.").small().weak());
                        if let Some(error) = &app.host_info_stats.last_ptr_error {
                            ui.label(RichText::new(format!("Last PTR error: {error}"))
                                .small().color(theme::warning(ui)));
                        }
                    }
                    if app.read_neighbor_cache {
                        ui.label(
                            RichText::new(format!(
                                "Cached MAC: {} · Neighbor-cache errors: {}",
                                app.host_info_stats.mac_found,
                                app.host_info_stats.neighbor_failed,
                            )).small(),
                        );
                        if let Some(error) = &app.host_info_stats.last_neighbor_error {
                            ui.label(RichText::new(format!("Neighbor cache: {error}"))
                                .small().color(theme::warning(ui)));
                        }
                    }
                }
            });

        ui.add_space(8.0);
        egui::CollapsingHeader::new(RichText::new("Services database").small().strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(crate::scanner::services::services_db_info())
                        .small()
                        .weak(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .small_button("Load file...")
                        .on_hover_text(
                            "Load an external nmap-services, /etc/services or IANA CSV file",
                        )
                        .clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Services DB", &["txt", "services", "csv", "*"])
                            .pick_file()
                    {
                        match crate::scanner::services::load_services_file(&path) {
                            Ok(count) => {
                                app.set_status(
                                    StatusLevel::Success,
                                    format!("Loaded {count} services from {}", path.display()),
                                );
                            }
                            Err(err) => {
                                app.set_status(
                                    StatusLevel::Error,
                                    format!("Services DB error: {err}"),
                                );
                            }
                        }
                    }
                    if ui
                        .small_button("Reset")
                        .on_hover_text("Reset to built-in default services")
                        .clicked()
                    {
                        crate::scanner::services::reset_services_to_default();
                        app.set_status(
                            StatusLevel::Success,
                            "Services database reset to built-in defaults".to_owned(),
                        );
                    }
                });
            });
    });

    let preview = app.refresh_input_preview().map_err(str::to_owned);
    ui.add_space(8.0);
    scope_feedback(&preview, running, ui);
    ui.add_space(10.0);

    if !app.status.is_empty() {
        ui.add_space(6.0);
        let status_text = components::status_text(ui, &app.status, app.status_level);
        if let Some(path) = &app.last_export_path
            && app.status.starts_with("Saved ")
        {
            ui.label(status_text);
            let display_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("open file");
            if ui
                .link(format!("📁 {display_name}"))
                .on_hover_text(format!("Click to reveal in folder:\n{}", path.display()))
                .clicked()
            {
                crate::export::reveal_in_file_manager(path);
            }
        } else {
            ui.label(status_text);
        }
    }

    components::section_divider(ui);
    summary(app, &preview, ui);
}

fn scope_feedback(preview: &Result<ScopePreview, String>, running: bool, ui: &mut Ui) {
    match preview {
        Ok(scope) if !running => {
            let targets_str = if scope.hostnames > 0 {
                format!(
                    "{} target{} ({} hostname{})",
                    scope.targets,
                    if scope.targets == 1 { "" } else { "s" },
                    scope.hostnames,
                    if scope.hostnames == 1 { "" } else { "s" },
                )
            } else {
                format!(
                    "{} target{}",
                    scope.targets,
                    if scope.targets == 1 { "" } else { "s" },
                )
            };
            ui.label(
                RichText::new(format!(
                    "Planned scope: {targets_str} × {} port{} (~{} probes) · {} excluded",
                    scope.ports,
                    if scope.ports == 1 { "" } else { "s" },
                    scope.probes,
                    scope.excluded,
                ))
                .weak()
                .small(),
            );
            if scope.hostnames > 0 {
                ui.label(
                    RichText::new(
                        "DNS addresses are excluded after resolution; final scope may be smaller.",
                    )
                    .small()
                    .weak(),
                );
            }
        }
        Ok(_) => {}
        Err(err) => {
            ui.label(RichText::new(err).color(theme::error(ui)).small());
        }
    }
}

fn summary(app: &App, preview: &Result<ScopePreview, String>, ui: &mut Ui) {
    let (targets, ports, probes) = if app.is_running() || app.total_probes > 0 {
        (app.target_count, app.port_count, app.total_probes)
    } else if let Ok(scope) = preview {
        (scope.targets, scope.ports, scope.probes)
    } else {
        (0, 0, 0)
    };

    egui::Grid::new("scan_summary")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            components::metric_row(ui, "Targets", targets, false);

            components::metric_row(ui, "Ports", ports, false);

            components::metric_row(ui, "Probes", probes, false);

            components::metric_row(ui, "Stored open results", app.results.len(), true);

            components::metric_row(ui, "Open probes", app.probe_counts.open, false);

            if app.probe_counts.open_filtered > 0 {
                ui.label(RichText::new("Open|Filtered").color(theme::warning(ui)))
                    .on_hover_text(
                        "No response received (UDP service is silent or packet filtered by firewall)",
                    );
                ui.label(app.probe_counts.open_filtered.to_string());
                ui.end_row();
            }

            components::metric_row(ui, "Refused", app.probe_counts.refused, false);

            components::metric_row(ui, "Timed out", app.probe_counts.timed_out, false);

            if app.probe_counts.unreachable > 0 {
                ui.label(RichText::new("Unreachable").color(theme::warning(ui)))
                    .on_hover_text(
                        "Network or host unreachable (e.g. IPv6 unrouted or disabled on host)",
                    );
                ui.label(app.probe_counts.unreachable.to_string());
                ui.end_row();
            }

            components::metric_row(ui, "Other errors", app.probe_counts.errors, false);

            if app.dropped_open_results > 0 {
                ui.label(RichText::new("Not stored (limit)").color(theme::warning(ui)));
                ui.label(app.dropped_open_results.to_string());
                ui.end_row();
            }
        });

    ui.add_space(6.0);
    ui.add(
        egui::ProgressBar::new(app.progress_fraction())
            .text(app.progress_text())
            .animate(app.is_running()),
    );
}

pub fn show_oui(app: &mut App, ui: &mut Ui) {
    ui.add_enabled_ui(!app.is_loading_oui(), |ui| {
        ui.horizontal_wrapped(|ui| {
        if ui.button("Import IEEE CSV...")
            .on_hover_text("Import official IEEE MA-L (oui.csv), MA-M (mam/oui28.csv), or MA-S (oui36.csv) data. Imports replace only that registry class.")
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("IEEE CSV", &["csv"])
                .pick_file()
        {
            app.load_oui_file(&path);
        }
        if ui.button("Reset OUI").clicked() {
            app.reset_oui_registry();
        }
        });
    });
    if app.is_loading_oui() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(
                RichText::new("Processing IEEE CSV in background...")
                    .small()
                    .weak(),
            );
        });
    }
    let (mal, mam, mas) = app.oui_registry.counts();
    ui.label(
        RichText::new(format!(
            "IEEE OUI entries: MA-L {mal} · MA-M {mam} · MA-S {mas}"
        ))
        .small()
        .weak(),
    );
    ui.label(RichText::new(&app.oui_status).small().weak());
}
