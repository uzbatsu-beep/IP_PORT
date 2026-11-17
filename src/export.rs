use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::scanner::ScanResult;

/// Версия схемы форматов экспорта (CSV/JSON v1).
#[allow(dead_code)]
pub const EXPORT_FORMAT_VERSION: u32 = 1;

/// Возвращает текущую версию спецификации экспорта.
#[allow(dead_code)]
pub fn export_format_version() -> u32 {
    EXPORT_FORMAT_VERSION
}

/// Канонический заголовок CSV версии 1 (RFC 4180).
pub const CSV_HEADER_V1: &str = "ip,port,service,rtt_ms";

/// Генерирует имя файла экспорта с текущей временной меткой: scan_results_YYYYMMDD_HHMMSS.{extension}
pub fn default_export_filename(extension: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (now / 86400) as i64;
    let seconds_of_day = (now % 86400) as u32;
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;

    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!("scan_results_{year:04}{m:02}{d:02}_{hour:02}{minute:02}{second:02}.{extension}")
}

/// Открывает путь или содержащую папку в системном проводнике файлов.
pub fn reveal_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        if path.exists() {
            let _ = std::process::Command::new("explorer")
                .arg(format!("/select,{}", path.display()))
                .spawn();
        } else if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("explorer").arg(parent).spawn();
        }
    }
    #[cfg(target_os = "macos")]
    {
        if path.exists() {
            let _ = std::process::Command::new("open")
                .arg("-R")
                .arg(path)
                .spawn();
        } else if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("open").arg(parent).spawn();
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let dir = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
}

/// Сохраняет результаты в CSV.
pub fn save_csv(path: &Path, results: &[ScanResult]) -> io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut out = BufWriter::new(file);
    writeln!(out, "{CSV_HEADER_V1}")?;
    for result in results {
        write_csv_field(&mut out, &result.ip.to_string())?;
        write!(out, ",{},", result.port)?;
        write_csv_field(&mut out, result.service)?;
        writeln!(out, ",{:.2}", result.rtt.as_secs_f64() * 1000.0)?;
    }
    out.flush()
}

fn write_csv_field(out: &mut impl Write, value: &str) -> io::Result<()> {
    if value
        .bytes()
        .any(|byte| matches!(byte, b',' | b'"' | b'\r' | b'\n'))
    {
        out.write_all(b"\"")?;
        for character in value.chars() {
            if character == '"' {
                out.write_all(b"\"\"")?;
            } else {
                let mut buffer = [0; 4];
                out.write_all(character.encode_utf8(&mut buffer).as_bytes())?;
            }
        }
        out.write_all(b"\"")
    } else {
        out.write_all(value.as_bytes())
    }
}

/// Сохраняет результаты в JSON.
pub fn save_json(path: &Path, results: &[ScanResult]) -> io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut out = BufWriter::new(file);
    writeln!(out, "[")?;
    for (idx, result) in results.iter().enumerate() {
        let comma = if idx + 1 == results.len() { "" } else { "," };
        write!(out, "  {{\"ip\": ")?;
        write_json_string(&mut out, &result.ip.to_string())?;
        write!(out, ", \"port\": {}, \"protocol\": ", result.port)?;
        write_json_string(&mut out, result.protocol.as_str())?;
        write!(out, ", \"state\": ")?;
        write_json_string(&mut out, result.state.as_str())?;
        write!(out, ", \"service\": ")?;
        write_json_string(&mut out, result.service)?;
        write!(out, ", \"verified\": {}", result.service_verified)?;
        if let Some(banner) = &result.banner {
            write!(out, ", \"banner\": ")?;
            write_json_string(&mut out, banner)?;
        }
        writeln!(
            out,
            ", \"rtt_ms\": {:.2}}}{comma}",
            result.rtt.as_secs_f64() * 1000.0
        )?;
    }
    writeln!(out, "]")?;
    out.flush()
}

fn write_json_string(out: &mut impl Write, value: &str) -> io::Result<()> {
    out.write_all(b"\"")?;
    for character in value.chars() {
        match character {
            '"' => out.write_all(b"\\\"")?,
            '\\' => out.write_all(b"\\\\")?,
            '\u{0008}' => out.write_all(b"\\b")?,
            '\u{000C}' => out.write_all(b"\\f")?,
            '\n' => out.write_all(b"\\n")?,
            '\r' => out.write_all(b"\\r")?,
            '\t' => out.write_all(b"\\t")?,
            c if c <= '\u{001F}' => write!(out, "\\u{:04x}", c as u32)?,
            c => {
                let mut buffer = [0; 4];
                out.write_all(c.encode_utf8(&mut buffer).as_bytes())?;
            }
        }
    }
    out.write_all(b"\"")
}

/// Экранирует спецсимволы HTML для безопасного вывода в отчёте.
pub fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Сохраняет результаты в красивый интерактивный автономный HTML-отчёт.
pub fn save_html(path: &Path, results: &[ScanResult]) -> io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut out = BufWriter::new(file);

    let total = results.len();
    let mut unique_ips: Vec<std::net::IpAddr> = results.iter().map(|r| r.ip).collect();
    unique_ips.sort();
    unique_ips.dedup();
    let unique_hosts_count = unique_ips.len();

    let mut latencies: Vec<f64> = results
        .iter()
        .map(|r| r.rtt.as_secs_f64() * 1000.0)
        .collect();
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let min_rtt = latencies.first().copied().unwrap_or(0.0);
    let max_rtt = latencies.last().copied().unwrap_or(0.0);
    let avg_rtt = if total > 0 {
        latencies.iter().sum::<f64>() / total as f64
    } else {
        0.0
    };

    let tcp_count = results
        .iter()
        .filter(|r| r.protocol == crate::scanner::ScanProtocol::Tcp)
        .count();
    let udp_count = results
        .iter()
        .filter(|r| r.protocol == crate::scanner::ScanProtocol::Udp)
        .count();

    writeln!(out, "<!DOCTYPE html>")?;
    writeln!(out, "<html lang=\"en\">")?;
    writeln!(out, "<head>")?;
    writeln!(out, "  <meta charset=\"utf-8\">")?;
    writeln!(
        out,
        "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">"
    )?;
    writeln!(out, "  <title>IP / PORT Scanner Report</title>")?;
    writeln!(out, "  <style>")?;
    writeln!(out, "    :root {{")?;
    writeln!(
        out,
        "      --bg: #121316; --card: #1a1c23; --card-hover: #222530; --border: #2d3139;"
    )?;
    writeln!(
        out,
        "      --text: #e2e8f0; --muted: #94a3b8; --accent: #10b981; --accent-light: #34d399;"
    )?;
    writeln!(
        out,
        "      --warn: #f59e0b; --danger: #ef4444; --code-bg: #0d0e11;"
    )?;
    writeln!(
        out,
        "      --font: system-ui, -apple-system, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;"
    )?;
    writeln!(out, "    }}")?;
    writeln!(
        out,
        "    * {{ box-sizing: border-box; margin: 0; padding: 0; }}"
    )?;
    writeln!(
        out,
        "    body {{ background: var(--bg); color: var(--text); font-family: var(--font); padding: 24px; line-height: 1.5; }}"
    )?;
    writeln!(
        out,
        "    .container {{ max-width: 1200px; margin: 0 auto; }}"
    )?;
    writeln!(
        out,
        "    header {{ display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid var(--border); padding-bottom: 16px; margin-bottom: 24px; }}"
    )?;
    writeln!(
        out,
        "    h1 {{ font-size: 1.5rem; font-weight: 700; color: #fff; }}"
    )?;
    writeln!(
        out,
        "    .subtitle {{ font-size: 0.875rem; color: var(--muted); }}"
    )?;
    writeln!(
        out,
        "    .cards {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 16px; margin-bottom: 24px; }}"
    )?;
    writeln!(
        out,
        "    .card {{ background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 16px; }}"
    )?;
    writeln!(
        out,
        "    .card-title {{ font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--muted); margin-bottom: 4px; }}"
    )?;
    writeln!(
        out,
        "    .card-val {{ font-size: 1.5rem; font-weight: 700; color: #fff; }}"
    )?;
    writeln!(
        out,
        "    .toolbar {{ display: flex; gap: 12px; margin-bottom: 16px; flex-wrap: wrap; align-items: center; }}"
    )?;
    writeln!(
        out,
        "    .search {{ flex: 1; min-width: 260px; background: var(--card); border: 1px solid var(--border); border-radius: 6px; padding: 8px 12px; color: var(--text); font-size: 0.875rem; outline: none; }}"
    )?;
    writeln!(out, "    .search:focus {{ border-color: var(--accent); }}")?;
    writeln!(
        out,
        "    .btn-group {{ display: flex; gap: 4px; background: var(--card); border: 1px solid var(--border); border-radius: 6px; padding: 2px; }}"
    )?;
    writeln!(
        out,
        "    .filter-btn {{ background: transparent; border: none; border-radius: 4px; padding: 6px 12px; color: var(--muted); font-size: 0.75rem; font-weight: 600; cursor: pointer; }}"
    )?;
    writeln!(
        out,
        "    .filter-btn.active, .filter-btn:hover {{ background: #2a2e3d; color: #fff; }}"
    )?;
    writeln!(
        out,
        "    table {{ width: 100%; border-collapse: collapse; background: var(--card); border: 1px solid var(--border); border-radius: 8px; overflow: hidden; }}"
    )?;
    writeln!(
        out,
        "    th {{ background: #161820; text-align: left; padding: 12px; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--muted); border-bottom: 1px solid var(--border); cursor: pointer; user-select: none; }}"
    )?;
    writeln!(out, "    th:hover {{ color: #fff; }}")?;
    writeln!(
        out,
        "    td {{ padding: 10px 12px; font-size: 0.875rem; border-bottom: 1px solid var(--border); }}"
    )?;
    writeln!(out, "    tr:hover td {{ background: var(--card-hover); }}")?;
    writeln!(
        out,
        "    .mono {{ font-family: monospace; font-size: 0.85rem; }}"
    )?;
    writeln!(
        out,
        "    .badge {{ display: inline-block; padding: 2px 6px; border-radius: 4px; font-size: 0.75rem; font-weight: 600; }}"
    )?;
    writeln!(
        out,
        "    .badge-open {{ background: rgba(16, 185, 129, 0.2); color: #34d399; }}"
    )?;
    writeln!(
        out,
        "    .badge-filtered {{ background: rgba(245, 158, 11, 0.2); color: #fbbf24; }}"
    )?;
    writeln!(
        out,
        "    .badge-verified {{ color: #10b981; font-weight: bold; margin-right: 4px; }}"
    )?;
    writeln!(
        out,
        "    .banner {{ color: var(--muted); font-size: 0.8rem; margin-top: 2px; }}"
    )?;
    writeln!(
        out,
        "    footer {{ text-align: center; color: var(--muted); font-size: 0.75rem; margin-top: 32px; }}"
    )?;
    writeln!(out, "  </style>")?;
    writeln!(out, "</head>")?;
    writeln!(out, "<body>")?;
    writeln!(out, "  <div class=\"container\">")?;
    writeln!(out, "    <header>")?;
    writeln!(out, "      <div>")?;
    writeln!(out, "        <h1>🔍 IP / PORT Scanner Report</h1>")?;
    writeln!(
        out,
        "        <div class=\"subtitle\">Generated by IP / PORT Scanner v0.2.0</div>"
    )?;
    writeln!(out, "      </div>")?;
    writeln!(out, "      <div class=\"subtitle\">{} results</div>", total)?;
    writeln!(out, "    </header>")?;

    // Сводные карточки
    writeln!(out, "    <div class=\"cards\">")?;
    writeln!(
        out,
        "      <div class=\"card\"><div class=\"card-title\">Total Open Ports</div><div class=\"card-val\">{}</div></div>",
        total
    )?;
    writeln!(
        out,
        "      <div class=\"card\"><div class=\"card-title\">Unique Hosts</div><div class=\"card-val\">{}</div></div>",
        unique_hosts_count
    )?;
    writeln!(
        out,
        "      <div class=\"card\"><div class=\"card-title\">Protocols</div><div class=\"card-val\">{} TCP / {} UDP</div></div>",
        tcp_count, udp_count
    )?;
    writeln!(
        out,
        "      <div class=\"card\"><div class=\"card-title\">Latency (Min / Avg / Max)</div><div class=\"card-val\">{:.1} / {:.1} / {:.1} ms</div></div>",
        min_rtt, avg_rtt, max_rtt
    )?;
    writeln!(out, "    </div>")?;

    // Панель фильтров
    writeln!(out, "    <div class=\"toolbar\">")?;
    writeln!(
        out,
        "      <input type=\"text\" id=\"search\" class=\"search\" placeholder=\"Search IP, port, service, banner...\" oninput=\"filterRows()\">"
    )?;
    writeln!(out, "      <div class=\"btn-group\">")?;
    writeln!(
        out,
        "        <button class=\"filter-btn active\" id=\"btn-all\" onclick=\"setProto('all')\">All ({total})</button>"
    )?;
    writeln!(
        out,
        "        <button class=\"filter-btn\" id=\"btn-tcp\" onclick=\"setProto('tcp')\">TCP ({tcp_count})</button>"
    )?;
    writeln!(
        out,
        "        <button class=\"filter-btn\" id=\"btn-udp\" onclick=\"setProto('udp')\">UDP ({udp_count})</button>"
    )?;
    writeln!(out, "      </div>")?;
    writeln!(out, "    </div>")?;

    // Таблица результатов
    writeln!(out, "    <table id=\"results\">")?;
    writeln!(out, "      <thead>")?;
    writeln!(out, "        <tr>")?;
    writeln!(out, "          <th onclick=\"sortTable(0)\">Address ↕</th>")?;
    writeln!(out, "          <th onclick=\"sortTable(1)\">Port ↕</th>")?;
    writeln!(
        out,
        "          <th onclick=\"sortTable(2)\">Protocol ↕</th>"
    )?;
    writeln!(out, "          <th onclick=\"sortTable(3)\">State ↕</th>")?;
    writeln!(out, "          <th onclick=\"sortTable(4)\">Service ↕</th>")?;
    writeln!(out, "          <th onclick=\"sortTable(5)\">Latency ↕</th>")?;
    writeln!(out, "        </tr>")?;
    writeln!(out, "      </thead>")?;
    writeln!(out, "      <tbody>")?;

    for res in results {
        let ip_esc = escape_html(&res.ip.to_string());
        let port = res.port;
        let proto = res.protocol.as_str();
        let state = res.state.as_str();
        let state_class = match res.state {
            crate::scanner::PortState::Open => "badge-open",
            crate::scanner::PortState::OpenFiltered => "badge-filtered",
        };
        let svc_esc = escape_html(res.service);
        let verified_mark = if res.service_verified {
            "<span class=\"badge-verified\" title=\"Service confirmed by banner\">✔</span>"
        } else {
            ""
        };
        let banner_html = if let Some(b) = &res.banner {
            format!("<div class=\"banner\">{}</div>", escape_html(b))
        } else {
            String::new()
        };
        let rtt_ms = res.rtt.as_secs_f64() * 1000.0;

        writeln!(
            out,
            "        <tr data-proto=\"{}\">",
            proto.to_ascii_lowercase()
        )?;
        writeln!(out, "          <td class=\"mono\">{}</td>", ip_esc)?;
        writeln!(out, "          <td class=\"mono\">{}</td>", port)?;
        writeln!(out, "          <td>{}</td>", proto)?;
        writeln!(
            out,
            "          <td><span class=\"badge {}\">{}</span></td>",
            state_class, state
        )?;
        writeln!(
            out,
            "          <td>{}{}{}</td>",
            verified_mark, svc_esc, banner_html
        )?;
        writeln!(out, "          <td class=\"mono\">{:.2} ms</td>", rtt_ms)?;
        writeln!(out, "        </tr>")?;
    }

    writeln!(out, "      </tbody>")?;
    writeln!(out, "    </table>")?;
    writeln!(
        out,
        "    <footer>Generated by IP / PORT Scanner · Standalone Offline Report</footer>"
    )?;

    // Встроенный JavaScript для фильтрации и сортировки
    writeln!(out, "    <script>")?;
    writeln!(out, "      let currentProto = 'all';")?;
    writeln!(out, "      function setProto(p) {{")?;
    writeln!(out, "        currentProto = p;")?;
    writeln!(
        out,
        "        document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));"
    )?;
    writeln!(
        out,
        "        document.getElementById('btn-' + p).classList.add('active');"
    )?;
    writeln!(out, "        filterRows();")?;
    writeln!(out, "      }}")?;
    writeln!(out, "      function filterRows() {{")?;
    writeln!(
        out,
        "        const q = document.getElementById('search').value.toLowerCase();"
    )?;
    writeln!(
        out,
        "        const rows = document.querySelectorAll('#results tbody tr');"
    )?;
    writeln!(out, "        rows.forEach(r => {{")?;
    writeln!(out, "          const p = r.getAttribute('data-proto');")?;
    writeln!(
        out,
        "          const protoMatch = (currentProto === 'all' || p === currentProto);"
    )?;
    writeln!(
        out,
        "          const textMatch = !q || r.innerText.toLowerCase().includes(q);"
    )?;
    writeln!(
        out,
        "          r.style.display = (protoMatch && textMatch) ? '' : 'none';"
    )?;
    writeln!(out, "        }});")?;
    writeln!(out, "      }}")?;
    writeln!(out, "      let sortAsc = true, lastCol = -1;")?;
    writeln!(out, "      function sortTable(n) {{")?;
    writeln!(
        out,
        "        const table = document.getElementById('results');"
    )?;
    writeln!(out, "        const tbody = table.querySelector('tbody');")?;
    writeln!(
        out,
        "        const rows = Array.from(tbody.querySelectorAll('tr'));"
    )?;
    writeln!(out, "        sortAsc = (lastCol === n) ? !sortAsc : true;")?;
    writeln!(out, "        lastCol = n;")?;
    writeln!(out, "        rows.sort((a, b) => {{")?;
    writeln!(out, "          let valA = a.children[n].innerText.trim();")?;
    writeln!(out, "          let valB = b.children[n].innerText.trim();")?;
    writeln!(
        out,
        "          let numA = parseFloat(valA), numB = parseFloat(valB);"
    )?;
    writeln!(
        out,
        "          if (!isNaN(numA) && !isNaN(numB)) return sortAsc ? numA - numB : numB - numA;"
    )?;
    writeln!(
        out,
        "          return sortAsc ? valA.localeCompare(valB) : valB.localeCompare(valA);"
    )?;
    writeln!(out, "        }});")?;
    writeln!(out, "        rows.forEach(r => tbody.appendChild(r));")?;
    writeln!(out, "      }}")?;
    writeln!(out, "    </script>")?;
    writeln!(out, "  </div>")?;
    writeln!(out, "</body>")?;
    writeln!(out, "</html>")?;

    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::time::Duration;

    fn sample() -> Vec<ScanResult> {
        vec![ScanResult::new(
            "10.0.0.1".parse::<IpAddr>().unwrap(),
            80,
            "http",
            Duration::from_millis(12),
        )]
    }

    #[test]
    fn writes_csv_and_json() {
        let dir = std::env::temp_dir().join("ip_port_scanner_export_test");
        std::fs::create_dir_all(&dir).unwrap();

        let csv = dir.join("out.csv");
        let json = dir.join("out.json");
        save_csv(&csv, &sample()).unwrap();
        save_json(&json, &sample()).unwrap();

        let csv_text = std::fs::read_to_string(&csv).unwrap();
        let json_text = std::fs::read_to_string(&json).unwrap();

        assert!(csv_text.starts_with("ip,port,service,rtt_ms\n"));
        assert!(csv_text.contains("10.0.0.1,80,http,12.00"));
        assert!(json_text.contains("\"port\": 80"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn escapes_csv_and_json_text_fields() {
        let results = vec![ScanResult::new(
            "10.0.0.1".parse().unwrap(),
            8080,
            "custom,\"service\"\nnext\t\0",
            Duration::from_millis(7),
        )];
        let dir = std::env::temp_dir().join("ip_port_scanner_export_escaping_test");
        std::fs::create_dir_all(&dir).unwrap();
        let csv = dir.join("escaped.csv");
        let json = dir.join("escaped.json");

        save_csv(&csv, &results).unwrap();
        save_json(&json, &results).unwrap();

        let csv_text = std::fs::read_to_string(&csv).unwrap();
        let json_text = std::fs::read_to_string(&json).unwrap();
        assert!(csv_text.contains("\"custom,\"\"service\"\"\nnext\t\0\""));
        assert!(json_text.contains("custom,\\\"service\\\"\\nnext\\t\\u0000"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reports_file_system_errors() {
        let missing = std::env::temp_dir()
            .join("ip_port_scanner_missing_export_directory")
            .join("out.csv");
        let _ = std::fs::remove_dir_all(missing.parent().unwrap());
        assert!(save_csv(&missing, &sample()).is_err());
        assert!(save_json(&missing, &sample()).is_err());
    }

    #[test]
    fn generates_timestamped_default_export_filename() {
        let name_csv = default_export_filename("csv");
        let name_json = default_export_filename("json");

        assert!(name_csv.starts_with("scan_results_"));
        assert!(name_csv.ends_with(".csv"));
        assert!(name_json.starts_with("scan_results_"));
        assert!(name_json.ends_with(".json"));

        // Format: scan_results_YYYYMMDD_HHMMSS.ext (13 + 8 + 1 + 6 + 4 = 32 chars)
        assert_eq!(name_csv.len(), 32);
        let timestamp_part = &name_csv["scan_results_".len()..name_csv.len() - ".csv".len()];
        assert_eq!(timestamp_part.len(), 15); // YYYYMMDD_HHMMSS
        assert_eq!(&timestamp_part[8..9], "_");
        assert!(
            timestamp_part
                .chars()
                .filter(|c| *c != '_')
                .all(|c| c.is_ascii_digit())
        );
    }

    #[test]
    fn verifies_csv_and_json_format_v1_compatibility() {
        let dir = std::env::temp_dir().join("ip_port_scanner_format_compat_test");
        std::fs::create_dir_all(&dir).unwrap();
        let csv_path = dir.join("compat.csv");
        let json_path = dir.join("compat.json");

        let items = vec![
            ScanResult::with_banner(
                "192.168.1.100".parse().unwrap(),
                80,
                "http",
                Duration::from_micros(12500),
                Some("nginx/1.18".to_owned()),
                true,
            ),
            ScanResult::new(
                "::1".parse().unwrap(),
                8080,
                "http-alt, \"proxy\"",
                Duration::from_micros(520),
            ),
        ];

        save_csv(&csv_path, &items).unwrap();
        save_json(&json_path, &items).unwrap();

        assert_eq!(EXPORT_FORMAT_VERSION, 1);

        // 1. Validate CSV header and column format
        let csv_text = std::fs::read_to_string(&csv_path).unwrap();
        let mut lines = csv_text.lines();
        assert_eq!(lines.next().unwrap(), CSV_HEADER_V1);
        let first_row = lines.next().unwrap();
        assert_eq!(first_row, "192.168.1.100,80,http,12.50");
        let second_row = lines.next().unwrap();
        assert_eq!(second_row, "::1,8080,\"http-alt, \"\"proxy\"\"\",0.52");

        // 2. Validate JSON structure, types and field names with serde_json
        let json_text = std::fs::read_to_string(&json_path).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_text).unwrap();
        assert_eq!(parsed.len(), 2);

        // Record 1
        assert_eq!(parsed[0]["ip"], "192.168.1.100");
        assert_eq!(parsed[0]["port"], 80);
        assert_eq!(parsed[0]["protocol"], "TCP");
        assert_eq!(parsed[0]["state"], "Open");
        assert_eq!(parsed[0]["service"], "http");
        assert_eq!(parsed[0]["rtt_ms"], 12.50);
        assert_eq!(parsed[0]["verified"], true);
        assert_eq!(parsed[0]["banner"], "nginx/1.18");

        // Record 2 (IPv6, escaped quotes and comma)
        assert_eq!(parsed[1]["ip"], "::1");
        assert_eq!(parsed[1]["port"], 8080);
        assert_eq!(parsed[1]["protocol"], "TCP");
        assert_eq!(parsed[1]["state"], "Open");
        assert_eq!(parsed[1]["service"], "http-alt, \"proxy\"");
        assert_eq!(parsed[1]["rtt_ms"], 0.52);
        assert_eq!(parsed[1]["verified"], false);
        assert_eq!(parsed[1].get("banner"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_html_report_and_escapes_html() {
        let dir = std::env::temp_dir().join("ip_port_scanner_html_test");
        std::fs::create_dir_all(&dir).unwrap();
        let html_path = dir.join("report.html");

        let mut item = ScanResult::new(
            "192.168.1.100".parse::<IpAddr>().unwrap(),
            8080,
            "<custom & dangerous>",
            Duration::from_millis(25),
        );
        item.banner = Some("Server: Test <script>alert(1)</script>".to_owned());
        item.service_verified = true;

        save_html(&html_path, &[item]).unwrap();
        let content = std::fs::read_to_string(&html_path).unwrap();

        assert!(content.contains("<!DOCTYPE html>"));
        assert!(content.contains("192.168.1.100"));
        assert!(content.contains("8080"));
        assert!(content.contains("&lt;custom &amp; dangerous&gt;"));
        assert!(content.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!content.contains("<script>alert(1)</script>"));
        assert!(content.contains("Total Open Ports"));

        let _ = std::fs::remove_dir_all(dir);
    }
}
