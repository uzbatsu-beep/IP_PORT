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
}
