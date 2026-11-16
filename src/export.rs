use std::fmt::Write as _;
use std::path::Path;

use crate::scanner::ScanResult;

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
pub fn save_csv(path: &Path, results: &[ScanResult]) -> std::io::Result<()> {
    let mut out = String::from("ip,port,service,rtt_ms\n");
    for result in results {
        let _ = writeln!(
            out,
            "{},{},{},{:.2}",
            result.ip,
            result.port,
            result.service,
            result.rtt.as_secs_f64() * 1000.0
        );
    }
    std::fs::write(path, out)
}

/// Сохраняет результаты в JSON.
pub fn save_json(path: &Path, results: &[ScanResult]) -> std::io::Result<()> {
    let mut out = String::from("[\n");
    for (idx, result) in results.iter().enumerate() {
        let comma = if idx + 1 == results.len() { "" } else { "," };
        let _ = writeln!(
            out,
            "  {{\"ip\": \"{}\", \"port\": {}, \"service\": \"{}\", \"rtt_ms\": {:.2}}}{comma}",
            result.ip,
            result.port,
            result.service,
            result.rtt.as_secs_f64() * 1000.0
        );
    }
    out.push_str("]\n");
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::time::Duration;

    fn sample() -> Vec<ScanResult> {
        vec![ScanResult {
            ip: "10.0.0.1".parse::<IpAddr>().unwrap(),
            port: 80,
            service: "http",
            rtt: Duration::from_millis(12),
        }]
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
}
