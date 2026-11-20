use egui::{RichText, Ui};

use crate::app::{App, ScopePreview, StatusLevel};
use crate::scanner::presets;
use crate::ui::theme;

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.add_space(4.0);
    ui.heading(RichText::new("IP / PORT Scanner").strong());
    ui.label(RichText::new("Async TCP & UDP port scanner").weak());
    ui.add_space(12.0);

    let running = app.is_running();
    let stopping = app.is_stopping();
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
                let (mal, mam, mas) = app.oui_registry.counts();
                ui.label(RichText::new(format!("IEEE OUI entries: MA-L {mal} · MA-M {mam} · MA-S {mas}"))
                    .small().weak());
                ui.label(RichText::new(&app.oui_status).small().weak());
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
                        )).small().color(theme::WARNING));
                        ui.label(RichText::new("Soft deadline only; the OS resolver is allowed to finish.").small().weak());
                        if let Some(error) = &app.host_info_stats.last_ptr_error {
                            ui.label(RichText::new(format!("Last PTR error: {error}"))
                                .small().color(theme::WARNING));
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
                                .small().color(theme::WARNING));
                        }
                    }
                }
            });

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Save profile...")
                .on_hover_text("Save targets, ports, timeout, concurrency and banner settings as JSON")
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_file_name("scan_profile.json")
                    .add_filter("JSON profile", &["json"])
                    .save_file()
            {
                app.save_profile(&path);
            }
            if ui
                .button("Load profile...")
                .on_hover_text("Load and validate settings from a JSON profile")
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON profile", &["json"])
                    .pick_file()
            {
                app.load_profile(&path);
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

    ui.horizontal_wrapped(|ui| {
        let cmd = crate::ui::cmd_key_name();
        let can_start = !running && preview.is_ok();
        let start = egui::Button::new(RichText::new("Start scan").strong())
            .fill(egui::Color32::from_rgb(0x2b, 0x7d, 0xd6));
        let start_hover = match &preview {
            Ok(_) => format!("Begin scanning the configured targets and ports ({cmd}+Enter)"),
            Err(err) => err.clone(),
        };
        if ui
            .add_enabled(can_start, start)
            .on_hover_text(&start_hover)
            .clicked()
        {
            app.start_scan();
        }

        let stop_label = if stopping { "Stopping..." } else { "Stop" };
        let mut stop_button = egui::Button::new(stop_label);
        if running && !stopping {
            stop_button = stop_button.fill(theme::ERROR.gamma_multiply(0.28));
        }
        if ui
            .add_enabled(running && !stopping, stop_button)
            .on_hover_text("Cancel the current scan (Esc)")
            .clicked()
        {
            app.stop_scan();
        }

        let can_export = !app.results.is_empty() && !app.is_exporting();
        let export_hint = if app.is_exporting() {
            "An export is already in progress"
        } else if running {
            "Save a snapshot of all collected open ports (ignores active table filter)"
        } else {
            "Save all collected open ports (ignores active table filter)"
        };
        ui.add_enabled_ui(can_export, |ui| {
            ui.menu_button("Export ▾", |ui| {
                ui.set_min_width(140.0);
                if ui
                    .button(format!("Export CSV ({cmd}+E)"))
                    .on_hover_text(format!("{export_hint} as CSV"))
                    .clicked()
                {
                    app.export_csv();
                    ui.close();
                }
                if ui
                    .button(format!("Export JSON ({cmd}+Shift+E)"))
                    .on_hover_text(format!("{export_hint} as JSON"))
                    .clicked()
                {
                    app.export_json();
                    ui.close();
                }
                if ui
                    .button("Export HTML report...")
                    .on_hover_text(format!("{export_hint} as interactive HTML report"))
                    .clicked()
                {
                    app.export_html();
                    ui.close();
                }
            })
            .response
            .on_hover_text(format!("{export_hint} (CSV / JSON / HTML)"));
        });
    });

    if !app.status.is_empty() {
        ui.add_space(6.0);
        let status_text = match app.status_level {
            StatusLevel::Idle => RichText::new(&app.status).weak(),
            StatusLevel::Running => RichText::new(&app.status).color(theme::ACCENT),
            StatusLevel::Stopping => RichText::new(&app.status).color(theme::WARNING),
            StatusLevel::Success => RichText::new(&app.status).color(theme::SUCCESS),
            StatusLevel::Error => RichText::new(&app.status).color(theme::ERROR),
        };
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

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
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
            ui.label(RichText::new(err).color(theme::ERROR).small());
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
            ui.label(RichText::new("Targets").weak());
            ui.label(targets.to_string());
            ui.end_row();

            ui.label(RichText::new("Ports").weak());
            ui.label(ports.to_string());
            ui.end_row();

            ui.label(RichText::new("Probes").weak());
            ui.label(probes.to_string());
            ui.end_row();

            ui.label(RichText::new("Stored open results").weak());
            ui.label(RichText::new(app.results.len().to_string()).strong());
            ui.end_row();

            ui.label(RichText::new("Open probes").weak());
            ui.label(app.probe_counts.open.to_string());
            ui.end_row();

            if app.probe_counts.open_filtered > 0 {
                ui.label(RichText::new("Open|Filtered").color(theme::WARNING))
                    .on_hover_text(
                        "No response received (UDP service is silent or packet filtered by firewall)",
                    );
                ui.label(app.probe_counts.open_filtered.to_string());
                ui.end_row();
            }

            ui.label(RichText::new("Refused").weak());
            ui.label(app.probe_counts.refused.to_string());
            ui.end_row();

            ui.label(RichText::new("Timed out").weak());
            ui.label(app.probe_counts.timed_out.to_string());
            ui.end_row();

            if app.probe_counts.unreachable > 0 {
                ui.label(RichText::new("Unreachable").color(theme::WARNING))
                    .on_hover_text(
                        "Network or host unreachable (e.g. IPv6 unrouted or disabled on host)",
                    );
                ui.label(app.probe_counts.unreachable.to_string());
                ui.end_row();
            }

            ui.label(RichText::new("Other errors").weak());
            ui.label(app.probe_counts.errors.to_string());
            ui.end_row();

            if app.dropped_open_results > 0 {
                ui.label(RichText::new("Not stored (limit)").color(theme::WARNING));
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
