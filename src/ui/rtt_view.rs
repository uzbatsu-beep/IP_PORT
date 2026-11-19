use egui::{Color32, RichText, Ui};

use crate::app::App;
use crate::scanner::{PortState, ScanResult};

/// Статистические метрики задержки (RTT) для набора открытых портов.
#[derive(Clone, Debug, PartialEq)]
pub struct RttStats {
    pub count: usize,
    pub min_ms: f64,
    pub avg_ms: f64,
    pub median_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub buckets: [RttBucket; 5],
}

/// Корзина распределения времени отклика.
#[derive(Clone, Debug, PartialEq)]
pub struct RttBucket {
    pub label: &'static str,
    pub count: usize,
    pub percentage: f32,
    pub color: Color32,
}

struct Thresholds {
    t1: f64,
    t2: f64,
    t3: f64,
    t4: f64,
    labels: [&'static str; 5],
}

impl Thresholds {
    fn for_max(max_ms: f64) -> Self {
        if max_ms <= 3.0 {
            // Localhost / Loopback / IPC
            Self {
                t1: 0.2,
                t2: 0.5,
                t3: 1.0,
                t4: 2.0,
                labels: ["< 0.2 ms", "0.2–0.5 ms", "0.5–1 ms", "1–2 ms", "> 2 ms"],
            }
        } else if max_ms <= 15.0 {
            // Fast Gigabit LAN / Direct Wi-Fi
            Self {
                t1: 1.0,
                t2: 2.5,
                t3: 5.0,
                t4: 10.0,
                labels: ["< 1 ms", "1–2.5 ms", "2.5–5 ms", "5–10 ms", "> 10 ms"],
            }
        } else if max_ms <= 60.0 {
            // Local network / Metro WAN
            Self {
                t1: 5.0,
                t2: 15.0,
                t3: 30.0,
                t4: 45.0,
                labels: ["< 5 ms", "5–15 ms", "15–30 ms", "30–45 ms", "> 45 ms"],
            }
        } else {
            // Standard WAN / Internet
            Self {
                t1: 20.0,
                t2: 50.0,
                t3: 100.0,
                t4: 250.0,
                labels: ["< 20 ms", "20–50 ms", "50–100 ms", "100–250 ms", "> 250 ms"],
            }
        }
    }
}

const BUCKET_COLORS: [Color32; 5] = [
    Color32::from_rgb(46, 204, 113), // Emerald green (fastest)
    Color32::from_rgb(100, 221, 23), // Lime green (fast)
    Color32::from_rgb(255, 193, 7),  // Amber/Gold (normal)
    Color32::from_rgb(255, 152, 0),  // Orange (elevated)
    Color32::from_rgb(239, 83, 80),  // Coral red (slow)
];

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Вычисляет статистику задержек и распределение по корзинам из списка результатов.
pub fn calculate_rtt_stats(results: &[ScanResult]) -> Option<RttStats> {
    // Open|Filtered means no UDP response; the timeout is not a measured RTT.
    let mut latencies: Vec<f64> = results
        .iter()
        .filter(|r| r.state == PortState::Open)
        .map(|r| r.rtt.as_secs_f64() * 1000.0)
        .collect();
    if latencies.is_empty() {
        return None;
    }
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let count = latencies.len();
    let min_ms = latencies[0];
    let max_ms = latencies[count - 1];
    let sum: f64 = latencies.iter().sum();
    let avg_ms = sum / count as f64;

    let median_ms = if count % 2 == 1 {
        latencies[count / 2]
    } else {
        (latencies[count / 2 - 1] + latencies[count / 2]) / 2.0
    };

    let p90_ms = percentile(&latencies, 0.90);
    let p95_ms = percentile(&latencies, 0.95);

    let thresholds = Thresholds::for_max(max_ms);
    let mut counts = [0usize; 5];

    for &rtt in &latencies {
        if rtt < thresholds.t1 {
            counts[0] += 1;
        } else if rtt < thresholds.t2 {
            counts[1] += 1;
        } else if rtt < thresholds.t3 {
            counts[2] += 1;
        } else if rtt < thresholds.t4 {
            counts[3] += 1;
        } else {
            counts[4] += 1;
        }
    }

    let total_f32 = count as f32;
    let buckets = [
        RttBucket {
            label: thresholds.labels[0],
            count: counts[0],
            percentage: (counts[0] as f32 / total_f32) * 100.0,
            color: BUCKET_COLORS[0],
        },
        RttBucket {
            label: thresholds.labels[1],
            count: counts[1],
            percentage: (counts[1] as f32 / total_f32) * 100.0,
            color: BUCKET_COLORS[1],
        },
        RttBucket {
            label: thresholds.labels[2],
            count: counts[2],
            percentage: (counts[2] as f32 / total_f32) * 100.0,
            color: BUCKET_COLORS[2],
        },
        RttBucket {
            label: thresholds.labels[3],
            count: counts[3],
            percentage: (counts[3] as f32 / total_f32) * 100.0,
            color: BUCKET_COLORS[3],
        },
        RttBucket {
            label: thresholds.labels[4],
            count: counts[4],
            percentage: (counts[4] as f32 / total_f32) * 100.0,
            color: BUCKET_COLORS[4],
        },
    ];

    Some(RttStats {
        count,
        min_ms,
        avg_ms,
        median_ms,
        p90_ms,
        p95_ms,
        max_ms,
        buckets,
    })
}

/// Отображает блок визуализации задержки (RTT Distribution).
pub fn show_rtt_panel(_app: &mut App, stats: &RttStats, ui: &mut Ui) {
    ui.scope(|ui| {
        egui::Frame::group(ui.style())
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .corner_radius(6.0)
            .show(ui, |ui| {
                // Строка заголовка и кратких метрик
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("📊 Latency (RTT)").strong().small());
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Min: {:.2} ms", stats.min_ms))
                            .small()
                            .color(BUCKET_COLORS[0]),
                    );
                    ui.label(
                        RichText::new(format!("Avg: {:.2} ms", stats.avg_ms))
                            .small()
                            .weak(),
                    );
                    ui.label(
                        RichText::new(format!("Median: {:.2} ms", stats.median_ms))
                            .small()
                            .strong(),
                    );
                    ui.label(
                        RichText::new(format!("p95: {:.2} ms", stats.p95_ms))
                            .small()
                            .weak(),
                    );
                    ui.label(
                        RichText::new(format!("Max: {:.2} ms", stats.max_ms))
                            .small()
                            .color(BUCKET_COLORS[4]),
                    );
                });

                ui.add_space(4.0);

                // Горизонтальный составной прогресс-бар распределения
                let bar_height = 6.0;
                let available_width = ui.available_width().max(100.0);
                let (rect, _response) = ui.allocate_exact_size(
                    egui::vec2(available_width, bar_height),
                    egui::Sense::hover(),
                );
                let painter = ui.painter();

                // Фоновая подложка
                painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);

                let mut current_x = rect.min.x;
                for bucket in &stats.buckets {
                    if bucket.count == 0 {
                        continue;
                    }
                    let seg_width = (available_width * (bucket.percentage / 100.0)).max(1.0);
                    let seg_rect = egui::Rect::from_min_max(
                        egui::pos2(current_x, rect.min.y),
                        egui::pos2((current_x + seg_width).min(rect.max.x), rect.max.y),
                    );
                    painter.rect_filled(seg_rect, 0.0, bucket.color);
                    current_x += seg_width;
                }

                ui.add_space(6.0);

                // Столбчатая мини-гистограмма по 5 корзинам
                let max_bucket_count = stats
                    .buckets
                    .iter()
                    .map(|b| b.count)
                    .max()
                    .unwrap_or(1)
                    .max(1);

                ui.columns(5, |columns| {
                    for (i, col) in columns.iter_mut().enumerate() {
                        let bucket = &stats.buckets[i];
                        col.vertical_centered(|ui| {
                            // Подпись количества сверху
                            ui.label(RichText::new(format!("{}", bucket.count)).small().color(
                                if bucket.count > 0 {
                                    ui.visuals().text_color()
                                } else {
                                    ui.visuals().weak_text_color()
                                },
                            ));

                            // Отрисовка вертикального столбика
                            let col_width = 24.0f32.min(ui.available_width());
                            let max_col_height = 36.0f32;
                            let bar_h = if bucket.count > 0 {
                                ((bucket.count as f32 / max_bucket_count as f32) * max_col_height)
                                    .max(4.0)
                            } else {
                                2.0
                            };

                            let (bar_rect, bar_resp) = ui.allocate_exact_size(
                                egui::vec2(col_width, max_col_height),
                                egui::Sense::hover(),
                            );
                            let bar_painter = ui.painter();

                            // Отрисовываем столбик снизу вверх
                            let filled_rect = egui::Rect::from_min_max(
                                egui::pos2(bar_rect.min.x, bar_rect.max.y - bar_h),
                                egui::pos2(bar_rect.max.x, bar_rect.max.y),
                            );
                            let fill_color = if bucket.count > 0 {
                                bucket.color
                            } else {
                                ui.visuals().extreme_bg_color
                            };
                            bar_painter.rect_filled(filled_rect, 2.0, fill_color);

                            // Всплывающая подсказка с деталями
                            let tooltip_text = format!(
                                "Range: {}\nCount: {} port{}\nShare: {:.1}% of total",
                                bucket.label,
                                bucket.count,
                                if bucket.count == 1 { "" } else { "s" },
                                bucket.percentage
                            );
                            bar_resp.on_hover_text(tooltip_text);

                            // Подпись диапазона снизу
                            ui.label(RichText::new(bucket.label).small().weak());
                        });
                    }
                });
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::time::Duration;

    fn sample_scan_result(rtt_ms: u64) -> ScanResult {
        ScanResult::new(
            IpAddr::from([127, 0, 0, 1]),
            80,
            "http",
            Duration::from_millis(rtt_ms),
        )
    }

    #[test]
    fn calculate_rtt_stats_returns_none_for_empty() {
        assert_eq!(calculate_rtt_stats(&[]), None);
    }

    #[test]
    fn ignores_unanswered_udp_probes_in_latency_statistics() {
        let confirmed = sample_scan_result(12);
        let unanswered = ScanResult::udp(
            IpAddr::from([127, 0, 0, 1]),
            161,
            "snmp",
            Duration::from_millis(800),
            crate::scanner::PortState::OpenFiltered,
            false,
        );
        assert!(calculate_rtt_stats(&[unanswered.clone()]).is_none());
        let stats = calculate_rtt_stats(&[confirmed, unanswered]).unwrap();
        assert_eq!(stats.count, 1);
        assert!((stats.avg_ms - 12.0).abs() < 1e-4);
    }

    #[test]
    fn calculate_rtt_stats_single_item() {
        let results = [sample_scan_result(42)];
        let stats = calculate_rtt_stats(&results).unwrap();
        assert_eq!(stats.count, 1);
        assert!((stats.min_ms - 42.0).abs() < 1e-4);
        assert!((stats.max_ms - 42.0).abs() < 1e-4);
        assert!((stats.avg_ms - 42.0).abs() < 1e-4);
        assert!((stats.median_ms - 42.0).abs() < 1e-4);
        assert!((stats.p95_ms - 42.0).abs() < 1e-4);
        let total_bucket_count: usize = stats.buckets.iter().map(|b| b.count).sum();
        assert_eq!(total_bucket_count, 1);
    }

    #[test]
    fn calculate_rtt_stats_calculates_percentiles_and_buckets() {
        let results = [
            sample_scan_result(10),
            sample_scan_result(20),
            sample_scan_result(30),
            sample_scan_result(40),
            sample_scan_result(100),
        ];
        let stats = calculate_rtt_stats(&results).unwrap();
        assert_eq!(stats.count, 5);
        assert!((stats.min_ms - 10.0).abs() < 1e-4);
        assert!((stats.max_ms - 100.0).abs() < 1e-4);
        assert!((stats.avg_ms - 40.0).abs() < 1e-4);
        assert!((stats.median_ms - 30.0).abs() < 1e-4);

        let total_bucket_count: usize = stats.buckets.iter().map(|b| b.count).sum();
        assert_eq!(total_bucket_count, 5);

        let total_percentage: f32 = stats.buckets.iter().map(|b| b.percentage).sum();
        assert!((total_percentage - 100.0).abs() < 0.1);
    }

    #[test]
    fn adaptive_thresholds_for_submillisecond_and_lan() {
        let sub_ms = [
            ScanResult::new(
                IpAddr::from([127, 0, 0, 1]),
                80,
                "http",
                Duration::from_micros(150),
            ),
            ScanResult::new(
                IpAddr::from([127, 0, 0, 1]),
                443,
                "https",
                Duration::from_micros(800),
            ),
        ];
        let stats = calculate_rtt_stats(&sub_ms).unwrap();
        assert_eq!(stats.buckets[0].label, "< 0.2 ms");
        assert_eq!(stats.buckets[0].count, 1);
        assert_eq!(stats.buckets[2].count, 1); // 0.5–1 ms
    }

    #[test]
    fn rtt_panel_renders_in_egui_context_without_panicking() {
        let mut app = App::default_state();
        let results = [
            sample_scan_result(5),
            sample_scan_result(15),
            sample_scan_result(50),
            sample_scan_result(120),
            sample_scan_result(450),
        ];
        let stats = calculate_rtt_stats(&results).unwrap();

        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show_rtt_panel(&mut app, &stats, ui);
        });
        output.textures_delta.clear();
    }
}
