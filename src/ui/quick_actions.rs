use egui::{RichText, Ui};
use std::net::IpAddr;

use crate::app::{App, ResultRow};

/// Определяет веб-ссылку (HTTP/HTTPS) для адреса и сервиса, если порт или протокол веб-совместим.
pub fn web_url_for(ip: IpAddr, port: u16, service: &str, banner: Option<&str>) -> Option<String> {
    let lower_svc = service.to_ascii_lowercase();
    let is_https = port == 443
        || port == 8443
        || port == 9443
        || lower_svc == "https"
        || lower_svc.contains("https")
        || lower_svc == "ssl/http";

    let is_http = port == 80
        || port == 8000
        || port == 8008
        || port == 8080
        || port == 8081
        || port == 8088
        || port == 8200
        || port == 8888
        || port == 3000
        || port == 5000
        || port == 9000
        || lower_svc == "http"
        || lower_svc.contains("http")
        || banner.is_some_and(|b| b.starts_with("HTTP/"));

    let host = if ip.is_ipv6() {
        format!("[{ip}]")
    } else {
        ip.to_string()
    };

    if is_https {
        Some(format!("https://{host}:{port}"))
    } else if is_http {
        Some(format!("http://{host}:{port}"))
    } else {
        None
    }
}

/// Генерирует команду curl для проверки эндпоинта.
pub fn curl_command(ip: IpAddr, port: u16, service: &str, banner: Option<&str>) -> String {
    if let Some(url) = web_url_for(ip, port, service, banner) {
        format!("curl -vk \"{url}\"")
    } else {
        let host = if ip.is_ipv6() {
            format!("[{ip}]")
        } else {
            ip.to_string()
        };
        format!("curl -v \"telnet://{host}:{port}\"")
    }
}

/// Генерирует команду SSH подключения, если порт или сервис соответствует SSH.
pub fn ssh_command(ip: IpAddr, port: u16, service: &str) -> Option<String> {
    let is_ssh = port == 22 || service.eq_ignore_ascii_case("ssh");
    if is_ssh {
        let host = if ip.is_ipv6() {
            format!("[{ip}]")
        } else {
            ip.to_string()
        };
        if port == 22 {
            Some(format!("ssh {host}"))
        } else {
            Some(format!("ssh {host} -p {port}"))
        }
    } else {
        None
    }
}

/// Генерирует CLI-команду подключения к популярным базам данных (MySQL, PostgreSQL, Redis).
pub fn database_cli_command(
    ip: IpAddr,
    port: u16,
    service: &str,
) -> Option<(&'static str, String)> {
    let lower = service.to_ascii_lowercase();
    let host = ip.to_string();
    if port == 3306 || lower == "mysql" || lower == "mariadb" {
        Some(("MySQL CLI", format!("mysql -h {host} -P {port} -u root -p")))
    } else if port == 5432 || lower == "postgresql" || lower == "postgres" {
        Some((
            "PostgreSQL CLI",
            format!("psql -h {host} -p {port} -U postgres"),
        ))
    } else if port == 6379 || lower == "redis" {
        Some(("Redis CLI", format!("redis-cli -h {host} -p {port}")))
    } else {
        None
    }
}

/// Отображает контекстное меню быстрых действий для строки результата.
pub fn show_row_context_menu(app: &mut App, entry: &ResultRow, ui: &mut egui::Ui) {
    let cmd = crate::ui::cmd_key_name();
    let proto = entry.raw.protocol.as_str();
    let ip = entry.raw.ip;
    let port = entry.raw.port;
    let service = entry.raw.service.as_str();
    let banner = entry.raw.banner.as_deref();
    let web_url = web_url_for(ip, port, service, banner);

    // 1. Сетевые и протокольные действия
    if let Some(url) = &web_url {
        if ui
            .button(RichText::new("🌐 Open in browser    (Enter)").strong())
            .clicked()
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            ui.close();
        }
        if ui.button("📋 Copy URL").clicked() {
            app.copy_text_with_toast(ui.ctx(), url.clone(), format!("Copied {url}"));
            ui.close();
        }
    }

    if let Some(ssh) = ssh_command(ip, port, service)
        && ui.button(format!("💻 Copy SSH command: {ssh}")).clicked()
    {
        app.copy_text_with_toast(ui.ctx(), ssh.clone(), format!("Copied {ssh}"));
        ui.close();
    }

    if let Some((label, db_cmd)) = database_cli_command(ip, port, service)
        && ui.button(format!("🗄 Copy {label}: {db_cmd}")).clicked()
    {
        app.copy_text_with_toast(ui.ctx(), db_cmd.clone(), format!("Copied {label} command"));
        ui.close();
    }

    let curl = curl_command(ip, port, service, banner);
    if ui.button("⚡ Copy curl command").clicked() {
        app.copy_text_with_toast(ui.ctx(), curl.clone(), "Copied curl command".to_owned());
        ui.close();
    }

    ui.separator();

    // 2. Копирование адресов и данных строки
    if ui
        .button(format!("Copy IP:Port ({proto})      ({cmd}+C)"))
        .clicked()
    {
        let text = format!("{}:{}", entry.ip_text, entry.port_text);
        app.copy_text_with_toast(ui.ctx(), text.clone(), format!("Copied {text}"));
        ui.close();
    }

    if ui.button("Copy IP").clicked() {
        let text = entry.ip_text.clone();
        app.copy_text_with_toast(ui.ctx(), text.clone(), format!("Copied IP {text}"));
        ui.close();
    }

    if ui.button("Copy Port").clicked() {
        let text = entry.port_text.clone();
        app.copy_text_with_toast(ui.ctx(), text.clone(), format!("Copied port {text}"));
        ui.close();
    }

    if let Some(b) = banner
        && ui.button("Copy banner").clicked()
    {
        app.copy_text_with_toast(
            ui.ctx(),
            b.to_owned(),
            format!("Copied banner for {}:{}", entry.ip_text, entry.port_text),
        );
        ui.close();
    }

    if ui.button("Copy as JSON").clicked() {
        let json_obj = serde_json::json!({
            "ip": entry.raw.ip,
            "port": entry.raw.port,
            "protocol": entry.raw.protocol.as_str(),
            "state": entry.raw.state.as_str(),
            "service": entry.raw.service.as_str(),
            "rtt_ms": (entry.raw.rtt.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
            "banner": entry.raw.banner,
        });
        let json_str = serde_json::to_string(&json_obj).unwrap_or_default();
        app.copy_text_with_toast(
            ui.ctx(),
            json_str,
            format!("Copied JSON for {}:{}", entry.ip_text, entry.port_text),
        );
        ui.close();
    }

    if ui.button("Copy row (TSV)").clicked() {
        let text = if let Some(b) = banner {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                entry.ip_text,
                entry.port_text,
                entry.protocol_text,
                entry.raw.service,
                entry.latency_text,
                b
            )
        } else {
            format!(
                "{}\t{}\t{}\t{}\t{}",
                entry.ip_text,
                entry.port_text,
                entry.protocol_text,
                entry.raw.service,
                entry.latency_text
            )
        };
        app.copy_text_with_toast(
            ui.ctx(),
            text,
            format!("Copied row {}:{}", entry.ip_text, entry.port_text),
        );
        ui.close();
    }

    ui.separator();

    // 3. Быстрая фильтрация и исследование хоста
    if ui
        .button(format!("🔍 Filter by this IP ({})", entry.ip_text))
        .clicked()
    {
        app.results_filter = entry.ip_text.clone();
        ui.close();
    }

    if ui
        .button(format!("🎯 Set as scan target ({})", entry.ip_text))
        .on_hover_text(
            "Puts this IP in the Targets field and sets ports to 1-1024 for deep scanning",
        )
        .clicked()
    {
        app.targets_input = entry.ip_text.clone();
        app.ports_input = "1-1024".to_owned();
        app.show_toast(format!("Targets set to {} (ports 1-1024)", entry.ip_text));
        ui.close();
    }
}

/// Отображает нижнюю панель быстрых действий для выделенной строки.
pub fn show_inspector_toolbar(app: &mut App, entry_idx: usize, ui: &mut Ui) {
    if entry_idx >= app.results.len() {
        return;
    }
    let entry = app.row(entry_idx).clone();
    let ip = entry.raw.ip;
    let port = entry.raw.port;
    let service = entry.raw.service.as_str();
    let banner = entry.raw.banner.as_deref();
    let web_url = web_url_for(ip, port, service, banner);

    ui.add_space(4.0);
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Selected:").strong().small());
                ui.label(
                    RichText::new(format!(
                        "{}:{} ({}, {})",
                        entry.ip_text, entry.port_text, entry.raw.service, entry.latency_text
                    ))
                    .small(),
                );

                ui.separator();

                if let Some(url) = &web_url
                    && ui
                        .small_button("🌐 Open Web")
                        .on_hover_text("Open URL in default web browser (Enter)")
                        .clicked()
                {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }

                if ui
                    .small_button("📋 Copy IP:Port")
                    .on_hover_text("Copy address and port to clipboard")
                    .clicked()
                {
                    let text = format!("{}:{}", entry.ip_text, entry.port_text);
                    app.copy_text_with_toast(ui.ctx(), text.clone(), format!("Copied {text}"));
                }

                if ui
                    .small_button("⚡ curl")
                    .on_hover_text("Copy curl probe command")
                    .clicked()
                {
                    let cmd = curl_command(ip, port, service, banner);
                    app.copy_text_with_toast(ui.ctx(), cmd, "Copied curl command".to_owned());
                }

                if let Some(ssh) = ssh_command(ip, port, service)
                    && ui
                        .small_button("💻 ssh")
                        .on_hover_text("Copy SSH command")
                        .clicked()
                {
                    app.copy_text_with_toast(ui.ctx(), ssh.clone(), format!("Copied {ssh}"));
                }

                if ui
                    .small_button("🔍 Filter Host")
                    .on_hover_text("Filter results table to only show ports for this host")
                    .clicked()
                {
                    app.results_filter = entry.ip_text.clone();
                }

                if ui
                    .small_button("🎯 Scan Host")
                    .on_hover_text("Load this IP as target with ports 1-1024")
                    .clicked()
                {
                    app.targets_input = entry.ip_text.clone();
                    app.ports_input = "1-1024".to_owned();
                    app.show_toast(format!("Targets set to {} (ports 1-1024)", entry.ip_text));
                }

                if ui.small_button("✕").on_hover_text("Deselect row").clicked() {
                    app.selected_result = None;
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn web_url_for_detects_http_and_https_ports() {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(
            web_url_for(ip, 80, "http", None),
            Some("http://127.0.0.1:80".to_owned())
        );
        assert_eq!(
            web_url_for(ip, 443, "https", None),
            Some("https://127.0.0.1:443".to_owned())
        );
        assert_eq!(
            web_url_for(ip, 8080, "unknown", None),
            Some("http://127.0.0.1:8080".to_owned())
        );
        assert_eq!(
            web_url_for(ip, 3000, "node", Some("HTTP/1.1 200 OK")),
            Some("http://127.0.0.1:3000".to_owned())
        );
        assert_eq!(web_url_for(ip, 22, "ssh", None), None);
    }

    #[test]
    fn web_url_for_ipv6_wraps_in_brackets() {
        let ip = IpAddr::V6(Ipv6Addr::LOCALHOST);
        assert_eq!(
            web_url_for(ip, 80, "http", None),
            Some("http://[::1]:80".to_owned())
        );
        assert_eq!(
            web_url_for(ip, 443, "https", None),
            Some("https://[::1]:443".to_owned())
        );
    }

    #[test]
    fn ssh_command_detects_port_22_and_custom_ports() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10));
        assert_eq!(
            ssh_command(ip, 22, "ssh"),
            Some("ssh 192.168.1.10".to_owned())
        );
        assert_eq!(
            ssh_command(ip, 2222, "ssh"),
            Some("ssh 192.168.1.10 -p 2222".to_owned())
        );
        assert_eq!(ssh_command(ip, 80, "http"), None);
    }

    #[test]
    fn database_cli_command_detects_mysql_postgres_redis() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));
        assert!(database_cli_command(ip, 3306, "mysql").is_some());
        assert!(database_cli_command(ip, 5432, "postgresql").is_some());
        assert!(database_cli_command(ip, 6379, "redis").is_some());
        assert!(database_cli_command(ip, 80, "http").is_none());
    }

    #[test]
    fn curl_command_generates_valid_curl_commands() {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(
            curl_command(ip, 80, "http", None),
            "curl -vk \"http://127.0.0.1:80\""
        );
        assert_eq!(
            curl_command(ip, 22, "ssh", None),
            "curl -v \"telnet://127.0.0.1:22\""
        );
    }

    #[test]
    fn inspector_toolbar_and_context_menu_render_without_panicking() {
        let mut app = App::default_state();
        let res = crate::scanner::ScanResult::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            80,
            "http",
            std::time::Duration::from_millis(5),
        );
        app.push_result(res);
        app.ensure_results_view();

        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show_inspector_toolbar(&mut app, 0, ui);
            let entry = app.row(0).clone();
            ui.menu_button("Test Menu", |ui| {
                show_row_context_menu(&mut app, &entry, ui);
            });
        });
        output.textures_delta.clear();
    }
}
