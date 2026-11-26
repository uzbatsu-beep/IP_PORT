//! Deterministic, bounded, synthetic live scan QA. No real network probes.
use super::*;
use serde_json::{Value, json};

const QUEUE: usize = 4096;
const FRAME_LIMIT: usize = 512;

struct Harness {
    app: App,
    ctx: egui::Context,
    tx: Option<mpsc::Sender<ScanEvent>>,
    terminal: Option<std::sync::mpsc::Sender<ScanEvent>>,
    cancel: CancellationToken,
    next: usize,
    sent: usize,
    consumed: usize,
    cleared: usize,
    samples: Vec<Value>,
    max_queue: usize,
    max_shapes: usize,
    cancellation_ms: Option<f64>,
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn observation(index: usize) -> ScanResult {
    let ip = IpAddr::V4(Ipv4Addr::from(0x0a00_0001 + (index / 10) as u32));
    let port = (80 + index % 10) as u16;
    let service = if index.is_multiple_of(10) {
        "http"
    } else {
        "ssh"
    };
    ScanResult::new(ip, port, service, Duration::from_micros((index % 5000 + 100) as u64))
}

impl Harness {
    fn new(initial: usize, mode: ResultsViewMode) -> Self {
        let mut app = App::default_state();
        app.workspace.section = ui::navigation::Section::Devices;
        app.view_mode = mode;
        for index in 0..initial {
            app.push_result(observation(index));
        }
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let (tx, rx) = mpsc::channel(QUEUE);
        let (terminal, terminal_rx) = std::sync::mpsc::channel();
        let cancel = CancellationToken::new();
        app.scan = Some(ScanHandle::from_channels(rx, terminal_rx, cancel.clone()));
        app.scan_started_at = Some(Instant::now());
        app.set_status(StatusLevel::Running, "Synthetic scan".to_owned());
        Self {
            app,
            ctx,
            tx: Some(tx),
            terminal: Some(terminal),
            cancel,
            next: initial,
            sent: 0,
            consumed: 0,
            cleared: 0,
            samples: Vec::new(),
            max_queue: 0,
            max_shapes: 0,
            cancellation_ms: None,
        }
    }

    fn depth(&self) -> usize {
        self.tx.as_ref().map_or(self.sent - self.consumed, |tx| QUEUE - tx.capacity())
    }

    fn send(&mut self, count: usize) {
        let tx = self.tx.as_ref().unwrap();
        assert!(count <= tx.capacity());
        for _ in 0..count {
            tx.try_send(ScanEvent::Open(observation(self.next))).unwrap();
            self.next += 1;
            self.sent += 1;
        }
    }

    fn frame(&mut self) {
        assert!(self.samples.len() < FRAME_LIMIT, "live scan stalled");
        let before = self.depth();
        let revision = self.app.results.revision();
        let metrics_before = self.app.projection.metrics();
        self.app.live_probe = Some(LiveFrameProbe::default());
        let start = Instant::now();
        let output = run_headless_frame(&mut self.app, &self.ctx, egui::vec2(1180.0, 760.0), 1.0, vec![]);
        let frame_ms = ms(start.elapsed());
        let probe = self.app.live_probe.take().unwrap();
        let data_events = before.min(MAX_EVENTS_PER_FRAME);
        self.consumed += data_events;
        assert!(probe.events <= MAX_EVENTS_PER_FRAME);
        let after = self.depth();
        assert_eq!(before - after, data_events);
        // Terminal event is on a separate channel and is not a data observation.
        assert!(probe.events == data_events || probe.events == data_events + 1);
        self.max_queue = self.max_queue.max(before);
        self.max_shapes = self.max_shapes.max(output.shapes.len());
        assert!(output.shapes.len() < 5000);
        assert!(self.app.results.len() <= MAX_STORED_RESULTS);
        assert!(self.app.projection.metrics().view_rebuilds <= metrics_before.view_rebuilds + 1);
        let lookup_start = Instant::now();
        let selected = std::hint::black_box(self.app.selected_endpoint_index());
        let selection_ms = ms(lookup_start.elapsed());
        self.samples.push(json!({
            "frame_ms": frame_ms,
            "event_ms": probe.event_ms,
            "enrichment_ms": probe.enrichment_ms,
            "projection_ms": probe.projection_ms,
            "selection_lookup_ms": selection_ms,
            "selected_index": selected,
            "queue_before": before,
            "queue_after": after,
            "data_events": data_events,
            "processed_events": probe.events,
            "stored": self.app.results.len(),
            "dropped": self.app.results.dropped(),
            "revision_changed": self.app.results.revision() != revision,
            "projection_metrics": self.app.projection.metrics(),
        }));
    }

    fn finish(&mut self, cancelled: bool) {
        let start = Instant::now();
        if cancelled {
            self.app.stop_scan();
            assert!(self.cancel.is_cancelled());
            assert!(self.app.is_stopping());
        }
        // Send terminal before closing the data channel: queued hits must be drained first.
        self.terminal.take().unwrap().send(ScanEvent::Finished {
            done: self.sent,
            cancelled,
            counts: ProbeCounts { open: self.sent, ..ProbeCounts::default() },
        }).unwrap();
        drop(self.tx.take());
        while self.app.is_running() {
            self.frame();
        }
        assert_eq!(self.consumed, self.sent);
        assert_eq!(self.app.done_probes, self.sent);
        assert_eq!(self.app.probe_counts.open, self.sent);
        assert!(!self.app.is_stopping());
        if cancelled {
            self.cancellation_ms = Some(ms(start.elapsed()));
        }
    }

    fn report(&self, id: &str, initial: usize) -> Value {
        assert_eq!(self.consumed, self.sent);
        assert_eq!(initial + self.sent, self.app.results.len() + self.app.results.dropped() + self.cleared);
        let mut times: Vec<f64> = self.samples.iter().map(|s| s["frame_ms"].as_f64().unwrap()).collect();
        let elapsed = times.iter().sum::<f64>();
        let mean = elapsed / times.len() as f64;
        times.sort_by(f64::total_cmp);
        let percentile = |p: usize| times[(times.len() * p).div_ceil(100) - 1];
        json!({
            "id": id, "initial": initial, "sent": self.sent, "consumed": self.consumed,
            "stored": self.app.results.len(), "dropped": self.app.results.dropped(),
            "cleared": self.cleared, "max_queue": self.max_queue, "max_shapes": self.max_shapes,
            "mean_ms": mean, "p50_ms": percentile(50), "p95_ms": percentile(95),
            "p99_ms": percentile(99), "max_ms": times.last().unwrap(),
            "events_per_active_frame_second": if elapsed > 0.0 { Some(self.sent as f64 * 1000.0 / elapsed) } else { None },
            "within_target": mean < 16.0 && percentile(95) < 25.0,
            "cancellation_ms": self.cancellation_ms, "rss_bytes": process_rss_bytes(),
            "samples": self.samples,
            "projection_metrics": self.app.projection.metrics(),
        })
    }
}

fn metadata(h: &mut Harness, ip: IpAddr) {
    // Inject worker output through the actual App drain path, not directly into HostInfo.
    let (requests, _rx) = std::sync::mpsc::sync_channel(512);
    let (updates, rx) = std::sync::mpsc::channel();
    h.app.host_info_worker = Some(HostInfoWorker::from_test_channels(requests, rx));
    h.app.host_info_pending.insert(ip);
    updates.send(HostInfoEvent::Started(ip)).unwrap();
    updates.send(HostInfoEvent::Finished(ip, HostInfo {
        hostname: Some("live-fixture.example".to_owned()),
        mac: Some("00:11:22:33:44:55".to_owned()),
        ptr_status: PtrStatus::Resolved,
        ptr_duration_ms: Some(1),
        ..HostInfo::default()
    })).unwrap();
    h.frame();
    assert_eq!(h.app.host_info().get(&ip).unwrap().hostname.as_deref(), Some("live-fixture.example"));
    assert_eq!(h.app.host_info().get(&ip).unwrap().mac.as_deref(), Some("00:11:22:33:44:55"));
    h.app.host_info_worker = None;
    h.app.host_info_pending.clear();
    h.app.host_info_backlog.clear();
}

#[test]
fn live_performance_matrix_optimized() {
    let mut reports = Vec::new();

    // LIVE-01: continuous batches at low, medium and full frame budget.
    let mut h = Harness::new(0, ResultsViewMode::GroupedByHost);
    for batch in [64, 256, 1024] {
        for _ in 0..20 {
            h.send(batch);
            h.frame();
        }
    }
    h.finish(false);
    reports.push(h.report("LIVE-01", 0));

    // LIVE-02: burst at actual queue capacity, followed by budget-limited drainage.
    let mut h = Harness::new(20_000, ResultsViewMode::Flat);
    h.send(QUEUE);
    assert_eq!(h.tx.as_ref().unwrap().capacity(), 0);
    assert!(h.tx.as_ref().unwrap().try_send(ScanEvent::Open(observation(h.next))).is_err());
    h.finish(false);
    assert_eq!(h.max_queue, QUEUE);
    reports.push(h.report("LIVE-02", 20_000));

    // LIVE-03: filter with known deterministic matching count during arrival.
    let mut h = Harness::new(20_000, ResultsViewMode::Flat);
    for query in ["http", "missing", "ssh", ""] {
        h.app.projection.settings.results_filter = query.to_owned();
        h.send(256);
        h.frame();
        let expected = h.app.results.iter().filter(|r| r.service.contains(query)).count();
        assert_eq!(h.app.visible_indices().len(), expected);
    }
    h.finish(false);
    reports.push(h.report("LIVE-03", 20_000));

    // LIVE-04: stable ordering by port in both directions during arrival.
    let mut h = Harness::new(50_000, ResultsViewMode::GroupedByHost);
    h.app.projection.settings.sort_column = SortColumn::Port;
    for ascending in [true, false, true, false] {
        h.app.projection.settings.sort_ascending = ascending;
        h.send(256);
        h.frame();
        assert!(h.app.visible_indices().windows(2).all(|w| {
            let a = h.app.row(w[0]).raw.port;
            let b = h.app.row(w[1]).raw.port;
            if ascending { a <= b } else { a >= b }
        }));
    }
    h.finish(false);
    reports.push(h.report("LIVE-04", 50_000));

    // LIVE-05: real enrichment event handling, deterministic injected resolver output.
    let mut h = Harness::new(20_000, ResultsViewMode::GroupedByHost);
    let ip = h.app.row(0).raw.ip;
    h.app.projection.settings.results_filter = "live-fixture".to_owned();
    h.frame();
    assert!(h.app.visible_indices().is_empty());
    h.send(256);
    metadata(&mut h, ip);
    assert_eq!(h.app.visible_indices().len(), 10);
    assert_eq!(h.app.host_info_stats.completed, 1);
    h.finish(false);
    reports.push(h.report("LIVE-05", 20_000));

    // LIVE-06: stable identity, hidden filter, collapsed group, mode switches.
    let mut h = Harness::new(50_000, ResultsViewMode::GroupedByHost);
    h.app.select_endpoint(0);
    let id = h.app.results.id_at(0).unwrap();
    let ip = h.app.selected_host().unwrap();
    for query in ["missing", "", "ssh", ""] {
        h.app.projection.settings.results_filter = query.to_owned();
        h.app.toggle_host(ip);
        h.send(1024);
        h.frame();
        assert_eq!(h.app.selected_endpoint_index(), Some(0));
        assert_eq!(h.app.results.id_at(0), Some(id));
    }
    h.app.view_mode = ResultsViewMode::Flat;
    h.finish(false);
    assert_eq!(h.app.selected_endpoint_index(), Some(0));
    reports.push(h.report("LIVE-06", 50_000));

    // LIVE-07: capacity boundary and exact rejected count.
    let mut h = Harness::new(99_000, ResultsViewMode::Flat);
    h.send(QUEUE);
    h.finish(false);
    assert_eq!(h.app.results.len(), MAX_STORED_RESULTS);
    assert_eq!(h.app.results.dropped(), 3096);
    reports.push(h.report("LIVE-07", 99_000));

    // LIVE-08: cancellation with queued observations and terminal ordering.
    let mut h = Harness::new(20_000, ResultsViewMode::GroupedByHost);
    h.app.select_endpoint(0);
    h.send(QUEUE);
    h.finish(true);
    assert_eq!(h.app.selected_endpoint_index(), Some(0));
    reports.push(h.report("LIVE-08", 20_000));

    // LIVE-09: clear while scan is active, then new independent scan.
    let mut h = Harness::new(0, ResultsViewMode::GroupedByHost);
    h.send(256);
    h.frame();
    h.app.select_endpoint(0);
    let old = h.app.results.id_at(0).unwrap();
    h.cleared = h.app.results.len();
    h.app.clear_results();
    assert!(h.app.is_running());
    assert!(h.app.selected_host().is_none());
    h.send(256);
    h.frame();
    assert!(h.app.results.resolve(old).is_none());
    h.finish(false);
    let report = h.report("LIVE-09", 0);
    // Exercise the actual successful new-scan transition on the original App.
    let (tx, rx) = mpsc::channel(QUEUE);
    let (terminal, terminal_rx) = std::sync::mpsc::channel();
    let cancel = CancellationToken::new();
    let previous = h.app.results.id_at(0).unwrap();
    h.app.start_scan_with(|_| Ok(ScanHandle::from_channels(rx, terminal_rx, cancel.clone())));
    assert!(h.app.is_running());
    assert!(h.app.results.is_empty());
    assert!(h.app.results.resolve(previous).is_none());
    tx.try_send(ScanEvent::Open(observation(999))).unwrap();
    drop(tx);
    terminal.send(ScanEvent::Finished { done: 1, cancelled: false, counts: ProbeCounts { open: 1, ..ProbeCounts::default() } }).unwrap();
    let _ = run_headless_frame(&mut h.app, &h.ctx, egui::vec2(1180.0, 760.0), 1.0, vec![]);
    assert_eq!(h.app.results.len(), 1);
    assert!(!h.app.is_running());
    assert!(h.app.results.resolve(previous).is_none());
    reports.push(report);

    // LIVE-10: actual background writer takes a snapshot before later arrivals.
    let mut h = Harness::new(1000, ResultsViewMode::Flat);
    h.app.projection.settings.results_filter = "missing".to_owned();
    let path = std::env::temp_dir().join(format!("ip-port-live-export-{}.json", std::process::id()));
    h.app.export_to_path(path.clone(), "json");
    h.send(256);
    h.frame();
    h.finish(false);
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.app.export_task.is_some() {
        h.frame();
        assert!(Instant::now() < deadline, "export stalled");
        std::thread::sleep(Duration::from_millis(1));
    }
    let data: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(data.as_array().unwrap().len(), 1000);
    assert_eq!(h.app.results.len(), 1256);
    assert_eq!(h.app.last_export_path.as_ref(), Some(&path));
    reports.push(h.report("LIVE-10", 1000));

    // LIVE-11: combined sustained load, changing filters/sort, mode, metadata and selection.
    let mut h = Harness::new(80_000, ResultsViewMode::GroupedByHost);
    h.app.select_endpoint(0);
    let ip = h.app.selected_host().unwrap();
    for i in 0..24 {
        h.app.view_mode = if i % 2 == 0 { ResultsViewMode::GroupedByHost } else { ResultsViewMode::Flat };
        h.app.projection.settings.sort_ascending = i % 3 == 0;
        h.app.projection.settings.results_filter = if i % 3 == 0 { "http" } else { "" }.to_owned();
        h.send(1024);
        if i == 3 {
            metadata(&mut h, ip);
        } else {
            h.frame();
        }
        assert_eq!(h.app.selected_endpoint_index(), Some(0));
    }
    h.finish(true);
    assert_eq!(h.app.results.len(), MAX_STORED_RESULTS);
    reports.push(h.report("LIVE-11", 80_000));

    let report = json!({
        "schema_version": 1, "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "commit": std::env::var("GITHUB_SHA").ok(), "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "functional_gate_passed": true, "timing_gate_required": false, "scenarios": reports,
    });
    if let Some(path) = std::env::var_os("IP_PORT_LIVE_REPORT") {
        let file = std::fs::File::create(path).expect("create live report");
        serde_json::to_writer_pretty(file, &report).expect("write live report");
    }
    println!("{report}");
}
