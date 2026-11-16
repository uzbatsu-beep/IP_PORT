use egui::{RichText, Ui};

use crate::app::{App, ScopePreview, StatusLevel};
use crate::ui::theme;

const PRESETS: &[(&str, &str)] = &[
    ("Common", "21,22,23,25,53,80,110,139,143,443,445,3389"),
    ("Web", "80,443,3000,5000,8000,8080,8081,8443,8888"),
    ("Database", "1433,1521,3306,5432,6379,9042,11211,27017"),
    ("Remote", "22,23,3389,5900,5985,5986"),
];

fn ports_input_matches(input: &str, ports: &str) -> bool {
    match (
        crate::scanner::targets::parse_ports(input),
        crate::scanner::targets::parse_ports(ports),
    ) {
        (Ok(input), Ok(ports)) => input == ports,
        _ => false,
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.add_space(4.0);
    ui.heading(RichText::new("IP / PORT Scanner").strong());
    ui.label(RichText::new("Async TCP port scanner").weak());
    ui.add_space(12.0);

    let running = app.is_running();
    let stopping = app.is_stopping();
    ui.add_enabled_ui(!running, |ui| {
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
        ui.label(RichText::new("Ports").strong());
        ui.add(
            egui::TextEdit::singleline(&mut app.ports_input)
                .desired_width(f32::INFINITY)
                .hint_text("22,80,443 or 1-1024"),
        );

        ui.horizontal_wrapped(|ui| {
            for (label, ports) in PRESETS {
                let active = ports_input_matches(&app.ports_input, ports);
                let button = egui::Button::new(*label).selected(active);
                if ui
                    .add(button)
                    .on_hover_text(format!("Ports: {ports}"))
                    .clicked()
                {
                    app.ports_input = (*ports).to_owned();
                }
            }
        });

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

        let can_export = !app.results.is_empty();
        let export_hint = if running {
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
            })
            .response
            .on_hover_text(format!("{export_hint} (CSV / JSON)"));
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
            ui.label(
                RichText::new(format!(
                    "Planned scope: {} target{} × {} port{} ({} probes)",
                    scope.targets,
                    if scope.targets == 1 { "" } else { "s" },
                    scope.ports,
                    if scope.ports == 1 { "" } else { "s" },
                    scope.probes
                ))
                .weak()
                .small(),
            );
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

            ui.label(RichText::new("Open ports").weak());
            ui.label(RichText::new(app.results.len().to_string()).strong());
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.add(
        egui::ProgressBar::new(app.progress_fraction())
            .text(app.progress_text())
            .animate(app.is_running()),
    );
}
