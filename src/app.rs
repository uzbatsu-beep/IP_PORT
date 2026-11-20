use std::collections::{HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::host_info::{HostInfo, HostInfoEvent, HostInfoWorker, PtrStatus};
use crate::oui_registry::OuiRegistry;
use crate::oui_store::{ImportedOui, RestoredOui};
use crate::scanner::{
    self, PortState, ProbeCounts, ScanConfig, ScanEvent, ScanHandle, ScanProtocol, ScanResult,
};
use crate::scanner::{scope::Exclusions, targets};
use crate::ui;

/// Максимальное число событий сканера, обрабатываемых за один кадр UI.
pub const MAX_EVENTS_PER_FRAME: usize = 1024;
pub const MAX_STORED_RESULTS: usize = 100_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortColumn {
    #[default]
    Address,
    Port,
    Service,
    Latency,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultsViewMode {
    #[default]
    Flat,
    GroupedByHost,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HostGroup {
    pub ip: IpAddr,
    pub ip_text: String,
    pub indices: Vec<usize>,
    pub ports_count: usize,
    pub services_summary: String,
    pub min_rtt: Duration,
    pub has_measured_rtt: bool,
}

impl HostGroup {
    pub fn build(ip: IpAddr, indices: Vec<usize>, rows: &[ResultRow]) -> Self {
        let ip_text = ip.to_string();
        let ports_count = indices.len();
        let confirmed_rtt = indices
            .iter()
            .filter(|&&idx| rows[idx].raw.state == PortState::Open)
            .map(|&idx| rows[idx].raw.rtt)
            .min();
        let min_rtt = confirmed_rtt.unwrap_or_default();

        let mut distinct_services: Vec<&str> = Vec::new();
        for &idx in &indices {
            let svc = rows[idx].raw.service.as_str();
            if svc != "unknown" && !distinct_services.contains(&svc) {
                distinct_services.push(svc);
            }
        }

        let services_summary = if distinct_services.is_empty() {
            "unknown".to_owned()
        } else if distinct_services.len() <= 3 {
            distinct_services.join(", ")
        } else {
            format!(
                "{}, +{} more",
                distinct_services[..3].join(", "),
                distinct_services.len() - 3
            )
        };

        Self {
            ip,
            ip_text,
            indices,
            ports_count,
            services_summary,
            min_rtt,
            has_measured_rtt: confirmed_rtt.is_some(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusLevel {
    #[default]
    Idle,
    Running,
    Stopping,
    Success,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopePreview {
    pub targets: usize,
    pub hostnames: usize,
    pub excluded: usize,
    pub ports: usize,
    pub probes: usize,
}

#[derive(Clone, Debug)]
pub struct ResultRow {
    pub raw: ScanResult,
    pub ip_text: String,
    pub port_text: String,
    pub protocol_text: &'static str,
    pub state_text: &'static str,
    pub latency_text: String,
}

impl ResultRow {
    fn from_result(raw: ScanResult) -> Self {
        let ip_text = raw.ip.to_string();
        let port_text = raw.port.to_string();
        let protocol_text = raw.protocol.as_str();
        let state_text = raw.state.as_str();
        let latency_text = if raw.state == PortState::OpenFiltered {
            "—".to_owned()
        } else {
            format!("{:.2} ms", raw.rtt.as_secs_f64() * 1000.0)
        };
        Self {
            raw,
            ip_text,
            port_text,
            protocol_text,
            state_text,
            latency_text,
        }
    }

    fn matches_lowercase_query(&self, query_lower: &str) -> bool {
        query_lower.is_empty()
            || contains_ascii_case_insensitive(&self.ip_text, query_lower)
            || self.port_text.contains(query_lower)
            || contains_ascii_case_insensitive(&self.raw.service, query_lower)
            || contains_ascii_case_insensitive(self.protocol_text, query_lower)
            || contains_ascii_case_insensitive(self.state_text, query_lower)
            || self
                .raw
                .banner
                .as_ref()
                .is_some_and(|banner| contains_ascii_case_insensitive(banner, query_lower))
    }
}

struct InputCache {
    targets_raw: String,
    targets: Result<crate::scanner::TargetList, String>,
    ports_raw: String,
    ports: Result<Vec<u16>, String>,
    exclusions_raw: String,
    exclusions: Result<Exclusions, String>,
    bind_addr_raw: String,
    bind_addr: Result<Option<IpAddr>, String>,
    preview: Result<ScopePreview, String>,
}

impl InputCache {
    fn new(
        targets_input: &str,
        ports_input: &str,
        bind_addr_input: &str,
        exclusions_input: &str,
    ) -> Self {
        let targets = targets::parse_targets(targets_input);
        let ports = targets::parse_ports(ports_input);
        let bind_addr = Self::parse_bind_addr(bind_addr_input);
        let exclusions = Exclusions::parse(exclusions_input);
        let preview = Self::compute_preview(&targets, &ports, &bind_addr, &exclusions);
        Self {
            targets_raw: targets_input.to_owned(),
            targets,
            ports_raw: ports_input.to_owned(),
            ports,
            exclusions_raw: exclusions_input.to_owned(),
            exclusions,
            bind_addr_raw: bind_addr_input.to_owned(),
            bind_addr,
            preview,
        }
    }

    fn parse_bind_addr(raw: &str) -> Result<Option<IpAddr>, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("default")
            || trimmed.eq_ignore_ascii_case("any")
        {
            Ok(None)
        } else {
            trimmed
                .parse::<IpAddr>()
                .map(Some)
                .map_err(|e| format!("Bind address: {e}"))
        }
    }

    fn sync(
        &mut self,
        targets_input: &str,
        ports_input: &str,
        bind_addr_input: &str,
        exclusions_input: &str,
    ) {
        let mut changed = false;
        if self.targets_raw != targets_input {
            self.targets_raw.clear();
            self.targets_raw.push_str(targets_input);
            self.targets = targets::parse_targets(targets_input);
            changed = true;
        }
        if self.ports_raw != ports_input {
            self.ports_raw.clear();
            self.ports_raw.push_str(ports_input);
            self.ports = targets::parse_ports(ports_input);
            changed = true;
        }
        if self.exclusions_raw != exclusions_input {
            self.exclusions_raw.clear();
            self.exclusions_raw.push_str(exclusions_input);
            self.exclusions = Exclusions::parse(exclusions_input);
            changed = true;
        }
        if self.bind_addr_raw != bind_addr_input {
            self.bind_addr_raw.clear();
            self.bind_addr_raw.push_str(bind_addr_input);
            self.bind_addr = Self::parse_bind_addr(bind_addr_input);
            changed = true;
        }
        if changed {
            self.preview = Self::compute_preview(
                &self.targets,
                &self.ports,
                &self.bind_addr,
                &self.exclusions,
            );
        }
    }

    fn compute_preview(
        targets: &Result<crate::scanner::TargetList, String>,
        ports: &Result<Vec<u16>, String>,
        bind_addr: &Result<Option<IpAddr>, String>,
        exclusions: &Result<Exclusions, String>,
    ) -> Result<ScopePreview, String> {
        if let Err(err) = bind_addr {
            return Err(err.clone());
        }
        let exclusions = exclusions
            .as_ref()
            .map_err(|err| format!("Exclusions: {err}"))?;
        let (targets_len, hostnames_len, excluded) = match targets {
            Ok(list) => {
                let excluded = list.ips.iter().filter(|ip| exclusions.contains(ip)).count();
                (
                    list.estimated_count() - excluded,
                    list.hostnames.len(),
                    excluded,
                )
            }
            Err(err) => return Err(format!("Targets: {err}")),
        };
        if targets_len == 0 {
            return Err("All targets are excluded from scan scope".to_owned());
        }
        let ports_len = match ports {
            Ok(list) => list.len(),
            Err(err) => return Err(format!("Ports: {err}")),
        };
        let probes = targets_len
            .checked_mul(ports_len)
            .ok_or_else(|| "Scan scope is too large for this platform".to_owned())?;
        Ok(ScopePreview {
            targets: targets_len,
            hostnames: hostnames_len,
            excluded,
            ports: ports_len,
            probes,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub message: String,
    pub created_at: Instant,
    pub duration: Duration,
}

struct ExportTask {
    receiver: std::sync::mpsc::Receiver<(PathBuf, usize, Result<(), String>)>,
}

enum OuiTaskResult {
    Imported { filename: String, data: ImportedOui },
    Restored(RestoredOui),
}

struct OuiTask {
    receiver: std::sync::mpsc::Receiver<Result<OuiTaskResult, String>>,
}

pub type ScanSpawner = fn(ScanConfig) -> Result<ScanHandle, String>;

#[derive(Clone, Debug, Default)]
pub struct HostInfoStats {
    pub completed: usize,
    pub ptr_resolved: usize,
    pub ptr_no_name: usize,
    pub ptr_failed: usize,
    pub mac_found: usize,
    pub neighbor_failed: usize,
    pub ptr_over_limit: usize,
    pub last_ptr_error: Option<String>,
    pub last_neighbor_error: Option<String>,
}

pub struct App {
    pub targets_input: String,
    pub ports_input: String,
    pub bind_addr_input: String,
    pub exclusions_input: String,
    pub resolve_ptr: bool,
    pub read_neighbor_cache: bool,
    pub ptr_soft_deadline_ms: u64,
    pub oui_registry: OuiRegistry,
    pub oui_status: String,
    oui_task: Option<OuiTask>,
    pub host_info: HashMap<IpAddr, HostInfo>,
    host_info_pending: HashSet<IpAddr>,
    host_info_backlog: VecDeque<IpAddr>,
    host_info_inflight: HashMap<IpAddr, Instant>,
    host_info_overdue: HashSet<IpAddr>,
    pub host_info_stats: HostInfoStats,
    host_info_worker: Option<HostInfoWorker>,
    pub interfaces: Vec<crate::scanner::NetworkInterfaceInfo>,
    pub timeout_ms: u64,
    pub concurrency: usize,
    pub banner_grabbing: bool,
    pub banner_timeout_ms: u64,
    pub protocol: ScanProtocol,

    pub status: String,
    pub status_level: StatusLevel,
    pub results: Vec<ScanResult>,
    pub results_filter: String,
    pub sort_column: SortColumn,
    pub sort_ascending: bool,
    pub selected_result: Option<usize>,
    pub view_mode: ResultsViewMode,

    pub target_count: usize,
    pub port_count: usize,
    pub total_probes: usize,
    pub done_probes: usize,
    pub probe_counts: ProbeCounts,
    pub dropped_open_results: usize,
    pub toasts: Vec<Toast>,
    scan_started_at: Option<Instant>,

    rows: Vec<ResultRow>,
    visible_indices: Vec<usize>,
    host_groups: Vec<HostGroup>,
    cached_rtt_stats: Option<crate::ui::rtt_view::RttStats>,
    rtt_stats_dirty: bool,
    cached_filter: String,
    cached_sort_column: SortColumn,
    cached_sort_ascending: bool,
    view_dirty: bool,
    view_rebuilds: usize,

    input_cache: InputCache,
    scan: Option<ScanHandle>,
    spawner: ScanSpawner,
    export_task: Option<ExportTask>,
    last_export_dir: Option<PathBuf>,
    pub last_export_path: Option<PathBuf>,
    pub scroll_to_selected: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        ui::theme::apply(&cc.egui_ctx);
        let mut app = Self::default_state();
        app.start_oui_restore();
        app
    }

    pub fn default_state() -> Self {
        let targets_input = "127.0.0.1".to_owned();
        let ports_input = "22,80,443".to_owned();
        let bind_addr_input = String::new();
        let exclusions_input = String::new();
        let input_cache = InputCache::new(
            &targets_input,
            &ports_input,
            &bind_addr_input,
            &exclusions_input,
        );

        Self {
            targets_input,
            ports_input,
            bind_addr_input,
            exclusions_input,
            resolve_ptr: false,
            read_neighbor_cache: false,
            ptr_soft_deadline_ms: 3_000,
            oui_registry: OuiRegistry::default(),
            oui_status: "Built-in OUI list".to_owned(),
            oui_task: None,
            host_info: HashMap::new(),
            host_info_pending: HashSet::new(),
            host_info_backlog: VecDeque::new(),
            host_info_inflight: HashMap::new(),
            host_info_overdue: HashSet::new(),
            host_info_stats: HostInfoStats::default(),
            host_info_worker: None,
            interfaces: crate::scanner::interfaces::list_interfaces(),
            timeout_ms: 800,
            concurrency: 256,
            banner_grabbing: false,
            banner_timeout_ms: 1000,
            protocol: ScanProtocol::Tcp,
            status: String::new(),
            status_level: StatusLevel::Idle,
            results: Vec::new(),
            results_filter: String::new(),
            sort_column: SortColumn::Address,
            sort_ascending: true,
            selected_result: None,
            view_mode: ResultsViewMode::Flat,
            target_count: 0,
            port_count: 0,
            total_probes: 0,
            done_probes: 0,
            probe_counts: ProbeCounts::default(),
            dropped_open_results: 0,
            toasts: Vec::new(),
            scan_started_at: None,
            rows: Vec::new(),
            visible_indices: Vec::new(),
            host_groups: Vec::new(),
            cached_rtt_stats: None,
            rtt_stats_dirty: true,
            cached_filter: String::new(),
            cached_sort_column: SortColumn::Address,
            cached_sort_ascending: true,
            view_dirty: false,
            view_rebuilds: 0,
            input_cache,
            scan: None,
            spawner: scanner::spawn,
            export_task: None,
            last_export_dir: None,
            last_export_path: None,
            scroll_to_selected: false,
        }
    }

    #[allow(dead_code)]
    pub fn set_spawner(&mut self, spawner: ScanSpawner) {
        self.spawner = spawner;
    }

    pub fn is_running(&self) -> bool {
        self.scan.is_some()
    }

    pub fn is_stopping(&self) -> bool {
        self.scan.as_ref().is_some_and(ScanHandle::is_cancelled)
    }

    pub fn refresh_interfaces(&mut self) {
        self.interfaces = crate::scanner::interfaces::list_interfaces();
    }

    pub fn refresh_input_preview(&mut self) -> Result<ScopePreview, &str> {
        self.input_cache.sync(
            &self.targets_input,
            &self.ports_input,
            &self.bind_addr_input,
            &self.exclusions_input,
        );
        self.input_cache
            .preview
            .as_ref()
            .copied()
            .map_err(String::as_str)
    }

    pub fn can_start_scan(&mut self) -> bool {
        !self.is_running() && self.refresh_input_preview().is_ok()
    }

    pub fn start_scan(&mut self) {
        self.start_scan_with(self.spawner);
    }

    pub fn start_scan_with<F>(&mut self, spawn_fn: F)
    where
        F: FnOnce(ScanConfig) -> Result<ScanHandle, String>,
    {
        if self.is_running() {
            return;
        }

        self.input_cache.sync(
            &self.targets_input,
            &self.ports_input,
            &self.bind_addr_input,
            &self.exclusions_input,
        );
        let preview = match &self.input_cache.preview {
            Ok(preview) => *preview,
            Err(err) => {
                self.set_status(StatusLevel::Error, err.clone());
                return;
            }
        };
        let targets = match &self.input_cache.targets {
            Ok(targets) => targets.clone(),
            Err(err) => {
                self.set_status(StatusLevel::Error, format!("Targets: {err}"));
                return;
            }
        };
        let ports = match &self.input_cache.ports {
            Ok(ports) => ports.clone(),
            Err(err) => {
                self.set_status(StatusLevel::Error, format!("Ports: {err}"));
                return;
            }
        };
        let bind_addr = match &self.input_cache.bind_addr {
            Ok(bind) => *bind,
            Err(err) => {
                self.set_status(StatusLevel::Error, err.clone());
                return;
            }
        };

        let config = ScanConfig {
            targets,
            ports,
            timeout: Duration::from_millis(self.timeout_ms),
            concurrency: self.concurrency,
            banner_grabbing: self.banner_grabbing,
            banner_timeout: Duration::from_millis(self.banner_timeout_ms),
            protocol: self.protocol,
            bind_addr,
            exclusions: self
                .input_cache
                .exclusions
                .as_ref()
                .expect("validated exclusions")
                .clone(),
        };
        let handle = match spawn_fn(config) {
            Ok(handle) => handle,
            Err(err) => {
                self.set_status(StatusLevel::Error, format!("Error: {err}"));
                return;
            }
        };

        self.clear_results_storage();
        self.start_host_enrichment();
        self.selected_result = None;
        self.target_count = preview.targets;
        self.port_count = preview.ports;
        self.total_probes = preview.probes;
        self.done_probes = 0;
        self.probe_counts = ProbeCounts::default();
        self.dropped_open_results = 0;
        self.scan_started_at = Some(Instant::now());
        self.set_status(
            StatusLevel::Running,
            format!(
                "Scanning {} target{} × {} port{} ({} probes)",
                preview.targets,
                if preview.targets == 1 { "" } else { "s" },
                preview.ports,
                if preview.ports == 1 { "" } else { "s" },
                self.total_probes
            ),
        );

        self.scan = Some(handle);
    }

    pub fn stop_scan(&mut self) {
        if let Some(handle) = &self.scan
            && !handle.is_cancelled()
        {
            handle.cancel();
            self.set_status(StatusLevel::Stopping, "Stopping scan...".to_owned());
        }
    }

    /// Очищает накопленные на текущий момент открытые порты.
    /// Если сканирование ещё идёт, новые открытые порты продолжат поступать.
    pub fn clear_results(&mut self) {
        if self.results.is_empty() {
            return;
        }
        self.clear_results_storage();
        if self.is_running() {
            self.start_host_enrichment();
        }
        if !self.is_running() {
            self.set_status(StatusLevel::Idle, "Results cleared".to_owned());
        }
    }

    pub fn push_result(&mut self, result: ScanResult) {
        if self.results.len() >= MAX_STORED_RESULTS {
            self.dropped_open_results += 1;
            return;
        }
        if !self.host_info.contains_key(&result.ip)
            && !self.host_info_pending.contains(&result.ip)
            && self.host_info_worker.is_some()
        {
            self.host_info_pending.insert(result.ip);
            self.host_info_backlog.push_back(result.ip);
            self.flush_host_info_backlog();
        }
        self.rows.push(ResultRow::from_result(result.clone()));
        self.results.push(result);
        self.view_dirty = true;
        self.rtt_stats_dirty = true;
    }

    fn start_host_enrichment(&mut self) {
        if !self.resolve_ptr && !self.read_neighbor_cache {
            return;
        }
        match HostInfoWorker::start(self.resolve_ptr, self.read_neighbor_cache) {
            Ok(worker) => self.host_info_worker = Some(worker),
            Err(err) => self.show_toast(format!("Host enrichment disabled: {err}")),
        }
    }

    /// Retry requests rejected by the bounded worker queue; no discovered host
    /// is silently dropped when more than 512 unique IPs arrive in a burst.
    fn flush_host_info_backlog(&mut self) {
        for _ in 0..64 {
            let Some(&ip) = self.host_info_backlog.front() else {
                break;
            };
            let next = self
                .host_info_worker
                .as_ref()
                .map(|worker| worker.submit(ip));
            match next {
                Some(Ok(())) => {
                    self.host_info_backlog.pop_front();
                }
                Some(Err(std::sync::mpsc::TrySendError::Full(_))) => break,
                Some(Err(std::sync::mpsc::TrySendError::Disconnected(_))) => {
                    self.host_info_worker = None;
                    self.host_info_backlog.clear();
                    self.host_info_pending.clear();
                    self.show_toast("Host identification worker disconnected".to_owned());
                    break;
                }
                None => break,
            }
        }
    }

    pub fn host_info_backlog_len(&self) -> usize {
        self.host_info_backlog.len()
    }

    pub fn host_info_pending_len(&self) -> usize {
        self.host_info_pending.len()
    }

    pub fn ptr_currently_overdue(&self) -> usize {
        self.host_info_overdue.len()
    }

    pub fn is_loading_oui(&self) -> bool {
        self.oui_task.is_some()
    }

    fn start_oui_restore(&mut self) {
        let Some(dir) = crate::oui_store::default_dir() else {
            self.oui_status = "Built-in OUI: no local application data directory".to_owned();
            return;
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let task = std::thread::Builder::new()
            .name("ieee-oui-restore".to_owned())
            .spawn(move || {
                let restored = crate::oui_store::restore(&dir);
                let _ = sender.send(Ok(OuiTaskResult::Restored(restored)));
            });
        match task {
            Ok(_) => {
                self.oui_task = Some(OuiTask { receiver });
                self.oui_status = "Restoring IEEE OUI cache...".to_owned();
            }
            Err(error) => self.show_toast(format!("IEEE OUI restore unavailable: {error}")),
        }
    }

    pub fn load_oui_file(&mut self, path: &std::path::Path) {
        if self.oui_task.is_some() {
            self.show_toast("IEEE OUI import already in progress".to_owned());
            return;
        }
        let Some(cache_dir) = crate::oui_store::default_dir() else {
            self.show_toast("IEEE OUI cache directory unavailable".to_owned());
            return;
        };
        let source = path.to_path_buf();
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("IEEE CSV")
            .to_owned();
        let (sender, receiver) = std::sync::mpsc::channel();
        let task = std::thread::Builder::new()
            .name("ieee-oui-import".to_owned())
            .spawn(move || {
                let result = crate::oui_store::import_and_cache(&source, &cache_dir)
                    .map(|data| OuiTaskResult::Imported { filename, data });
                let _ = sender.send(result);
            });
        match task {
            Ok(_) => {
                self.oui_task = Some(OuiTask { receiver });
                self.oui_status = format!("Importing IEEE CSV from {}...", path.display());
            }
            Err(error) => self.show_toast(format!("Cannot start IEEE OUI import: {error}")),
        }
    }

    pub fn reset_oui_registry(&mut self) {
        if self.oui_task.is_some() {
            self.show_toast("Wait for IEEE OUI import to finish before resetting".to_owned());
            return;
        }
        let remove_result = crate::oui_store::default_dir()
            .map(|dir| crate::oui_store::clear_cache(&dir));
        self.oui_registry.reset();
        self.refresh_oui_vendors();
        match remove_result {
            Some(Err(error)) => {
                self.oui_status = format!("Built-in OUI (cache removal failed: {error})");
                self.show_toast(self.oui_status.clone());
            }
            _ => {
                self.oui_status = "Built-in OUI list (saved imports removed)".to_owned();
                self.show_toast("IEEE OUI registry reset".to_owned());
            }
        }
    }

    fn poll_oui_task(&mut self, ctx: &egui::Context) {
        let outcome = self.oui_task.as_ref().map(|task| task.receiver.try_recv());
        match outcome {
            Some(Ok(Ok(OuiTaskResult::Imported { filename, data }))) => {
                self.oui_task = None;
                self.oui_registry.replace_kind_from(data.kind, data.registry);
                self.refresh_oui_vendors();
                self.oui_status = format!(
                    "Imported and saved {} {} entries from {filename}",
                    data.count,
                    data.kind.label(),
                );
                self.show_toast(self.oui_status.clone());
            }
            Some(Ok(Ok(OuiTaskResult::Restored(restored)))) => {
                self.oui_task = None;
                self.oui_registry = restored.registry;
                self.refresh_oui_vendors();
                let count = restored.loaded;
                if !restored.warnings.is_empty() {
                    self.oui_status = format!(
                        "Restored {count} IEEE entries; {} cache warning(s)",
                        restored.warnings.len(),
                    );
                    self.show_toast(format!(
                        "IEEE OUI cache: {}",
                        restored.warnings.join("; ")
                    ));
                } else if count > 0 {
                    self.oui_status = format!("Restored {count} saved IEEE OUI entries");
                } else {
                    self.oui_status = "Built-in OUI list".to_owned();
                }
            }
            Some(Ok(Err(error))) => {
                self.oui_task = None;
                self.oui_status = format!("IEEE OUI import failed: {error}");
                self.show_toast(self.oui_status.clone());
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.oui_task = None;
                self.oui_status = "IEEE OUI worker disconnected".to_owned();
                self.show_toast(self.oui_status.clone());
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            None => {}
        }
    }

    fn refresh_oui_vendors(&mut self) {
        for info in self.host_info.values_mut() {
            info.vendor = info
                .mac
                .as_deref()
                .and_then(|mac| self.oui_registry.lookup(mac));
        }
        self.view_dirty = true;
    }

    fn drain_host_enrichment(&mut self, ctx: &egui::Context) {
        for _ in 0..128 {
            let next = self
                .host_info_worker
                .as_ref()
                .and_then(HostInfoWorker::try_recv);
            let Some(event) = next else { break };
            match event {
                HostInfoEvent::Started(ip) => {
                    if self.resolve_ptr {
                        self.host_info_inflight.insert(ip, Instant::now());
                    }
                }
                HostInfoEvent::Finished(ip, mut info) => {
                    self.host_info_pending.remove(&ip);
                    self.host_info_inflight.remove(&ip);
                    let was_overdue = self.host_info_overdue.remove(&ip);
                    if self.resolve_ptr
                        && (was_overdue
                            || info
                                .ptr_duration_ms
                                .is_some_and(|ms| ms >= self.ptr_soft_deadline_ms))
                        && !was_overdue
                    {
                        self.host_info_stats.ptr_over_limit += 1;
                    }
                    info.vendor = info
                        .mac
                        .as_deref()
                        .and_then(|mac| self.oui_registry.lookup(mac));
                    self.host_info_stats.completed += 1;
                    match &info.ptr_status {
                        PtrStatus::Disabled => {}
                        PtrStatus::Resolved => self.host_info_stats.ptr_resolved += 1,
                        PtrStatus::NoName => self.host_info_stats.ptr_no_name += 1,
                        PtrStatus::Failed(message) => {
                            self.host_info_stats.ptr_failed += 1;
                            self.host_info_stats.last_ptr_error = Some(format!("{ip}: {message}"));
                        }
                    }
                    if info.mac.is_some() {
                        self.host_info_stats.mac_found += 1;
                    }
                    if let Some(message) = &info.neighbor_error {
                        self.host_info_stats.neighbor_failed += 1;
                        self.host_info_stats.last_neighbor_error = Some(message.clone());
                    }
                    self.host_info.insert(ip, info);
                    self.view_dirty = true;
                }
            }
        }
        // A soft deadline is an alert only. getnameinfo may continue blocking
        // on a worker; do not report it as forcibly cancelled or requeue it.
        for (&ip, &started) in &self.host_info_inflight {
            if started.elapsed() >= Duration::from_millis(self.ptr_soft_deadline_ms)
                && self.host_info_overdue.insert(ip)
            {
                self.host_info_stats.ptr_over_limit += 1;
            }
        }
        self.flush_host_info_backlog();
        if self.host_info_worker.is_some() {
            if self.host_info_pending.is_empty()
                && self.host_info_backlog.is_empty()
                && !self.is_running()
            {
                self.host_info_worker = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(125));
            }
        }
    }

    pub fn ensure_results_view(&mut self) {
        let filter_changed = self.cached_filter != self.results_filter;
        let sort_changed = self.cached_sort_column != self.sort_column
            || self.cached_sort_ascending != self.sort_ascending;

        if !self.view_dirty && !filter_changed && !sort_changed {
            return;
        }

        let query_lower = self.results_filter.trim().to_ascii_lowercase();
        self.visible_indices.clear();
        for (idx, row) in self.rows.iter().enumerate() {
            let info_matches =
                self.host_info.get(&row.raw.ip).is_some_and(|info| {
                    info.hostname
                        .as_deref()
                        .is_some_and(|value| contains_ascii_case_insensitive(value, &query_lower))
                        || info.mac.as_deref().is_some_and(|value| {
                            contains_ascii_case_insensitive(value, &query_lower)
                        })
                        || info.vendor.as_deref().is_some_and(|value| {
                            contains_ascii_case_insensitive(value, &query_lower)
                        })
                });
            if row.matches_lowercase_query(&query_lower) || info_matches {
                self.visible_indices.push(idx);
            }
        }

        let rows = &self.rows;
        let sort_column = self.sort_column;
        let sort_ascending = self.sort_ascending;
        self.visible_indices.sort_by(|&a_idx, &b_idx| {
            let a = &rows[a_idx].raw;
            let b = &rows[b_idx].raw;
            let ordering = match sort_column {
                SortColumn::Address => a.ip.cmp(&b.ip).then_with(|| a.port.cmp(&b.port)),
                SortColumn::Port => a.port.cmp(&b.port).then_with(|| a.ip.cmp(&b.ip)),
                SortColumn::Service => a
                    .service
                    .cmp(&b.service)
                    .then_with(|| a.ip.cmp(&b.ip))
                    .then_with(|| a.port.cmp(&b.port)),
                SortColumn::Latency => a
                    .rtt
                    .cmp(&b.rtt)
                    .then_with(|| a.ip.cmp(&b.ip))
                    .then_with(|| a.port.cmp(&b.port)),
            };

            if sort_ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });

        self.cached_filter.clear();
        self.cached_filter.push_str(&self.results_filter);
        self.cached_sort_column = self.sort_column;
        self.cached_sort_ascending = self.sort_ascending;
        self.view_dirty = false;
        self.view_rebuilds += 1;

        // Keep display order while using O(1) average lookup for each unique host.
        let mut groups_map: Vec<(IpAddr, Vec<usize>)> = Vec::new();
        let mut host_indices = std::collections::HashMap::<IpAddr, usize>::new();
        for &idx in &self.visible_indices {
            let ip = self.rows[idx].raw.ip;
            if let Some(&group_idx) = host_indices.get(&ip) {
                groups_map[group_idx].1.push(idx);
            } else {
                host_indices.insert(ip, groups_map.len());
                groups_map.push((ip, vec![idx]));
            }
        }
        self.host_groups = groups_map
            .into_iter()
            .map(|(ip, indices)| HostGroup::build(ip, indices, &self.rows))
            .collect();
    }

    /// Cache latency distribution until scan results change, not once per UI frame.
    pub fn current_rtt_stats(&mut self) -> Option<crate::ui::rtt_view::RttStats> {
        if self.rtt_stats_dirty {
            self.cached_rtt_stats = crate::ui::rtt_view::calculate_rtt_stats(&self.results);
            self.rtt_stats_dirty = false;
        }
        self.cached_rtt_stats.clone()
    }

    pub fn visible_indices(&self) -> &[usize] {
        &self.visible_indices
    }

    pub fn host_groups(&self) -> &[HostGroup] {
        &self.host_groups
    }

    pub fn row(&self, index: usize) -> &ResultRow {
        &self.rows[index]
    }

    #[cfg(test)]
    pub fn view_rebuilds(&self) -> usize {
        self.view_rebuilds
    }

    pub fn progress_fraction(&self) -> f32 {
        if self.total_probes == 0 {
            return 0.0;
        }
        (self.done_probes as f32 / self.total_probes as f32).clamp(0.0, 1.0)
    }

    pub fn progress_text(&self) -> String {
        if self.total_probes == 0 {
            return "Idle".to_owned();
        }
        if let Some(started_at) = self.scan_started_at {
            let elapsed = started_at.elapsed().as_secs_f64();
            let rate = if elapsed > 0.0 {
                self.done_probes as f64 / elapsed
            } else {
                0.0
            };
            let remaining = self.total_probes.saturating_sub(self.done_probes);
            let eta = if self.is_running() && rate > 0.0 {
                format!(" · ETA {:.1}s", remaining as f64 / rate)
            } else {
                String::new()
            };
            format!(
                "{}/{} probes · {:.1}s · {:.0} probes/s{eta}",
                self.done_probes, self.total_probes, elapsed, rate
            )
        } else {
            format!("{}/{} probes", self.done_probes, self.total_probes)
        }
    }

    pub fn export_csv(&mut self) {
        self.export("csv");
    }

    pub fn export_json(&mut self) {
        self.export("json");
    }

    pub fn export_html(&mut self) {
        self.export("html");
    }

    fn export(&mut self, extension: &str) {
        if self.export_task.is_some() {
            return;
        }

        let file_name = crate::export::default_export_filename(extension);
        let mut dialog = rfd::FileDialog::new()
            .set_file_name(&file_name)
            .add_filter(extension.to_uppercase(), &[extension]);
        if let Some(dir) = &self.last_export_dir {
            dialog = dialog.set_directory(dir);
        }

        let Some(path) = dialog.save_file() else {
            return;
        };
        self.last_export_dir = path.parent().map(PathBuf::from);

        let snapshot = self.results.clone();
        let host_metadata = self.host_info.clone();
        let snapshot_count = snapshot.len();
        let (sender, receiver) = std::sync::mpsc::channel();
        let path_for_task = path.clone();
        let extension = extension.to_owned();
        let task = std::thread::Builder::new()
            .name("export-writer".to_owned())
            .spawn(move || {
                let result = match extension.as_str() {
                    "csv" => crate::export::save_csv(&path_for_task, &snapshot),
                    "json" => {
                        crate::export::save_json_enriched(&path_for_task, &snapshot, &host_metadata)
                    }
                    "html" if host_metadata.is_empty() => {
                        crate::export::save_html(&path_for_task, &snapshot)
                    }
                    "html" => {
                        crate::export::save_html_enriched(&path_for_task, &snapshot, &host_metadata)
                    }
                    _ => crate::export::save_json(&path_for_task, &snapshot),
                }
                .map_err(|err| err.to_string());
                let _ = sender.send((path_for_task, snapshot_count, result));
            });

        match task {
            Ok(_) => {
                self.export_task = Some(ExportTask { receiver });
                if !self.is_running() {
                    self.set_status(StatusLevel::Running, "Exporting results...".to_owned());
                }
            }
            Err(err) => {
                self.set_status(StatusLevel::Error, format!("Export failed: {err}"));
            }
        }
    }

    fn poll_export_task(&mut self, ctx: &egui::Context) {
        let outcome = self
            .export_task
            .as_ref()
            .map(|task| task.receiver.try_recv());
        match outcome {
            Some(Ok((path, count, result))) => {
                self.export_task = None;
                match result {
                    Ok(()) => {
                        self.last_export_path = Some(path.clone());
                        let file_name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("export file");
                        self.show_toast(format!("Exported {count} results to {file_name}"));
                        let level = if self.is_stopping() {
                            StatusLevel::Stopping
                        } else if self.is_running() {
                            StatusLevel::Running
                        } else {
                            StatusLevel::Success
                        };
                        self.set_status(
                            level,
                            format!("Saved {count} results to {}", path.display()),
                        );
                    }
                    Err(err) => self.report_export_failure(err),
                }
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.export_task = None;
                self.report_export_failure("Export worker stopped unexpectedly".to_owned());
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            None => {}
        }
    }

    fn report_export_failure(&mut self, error: String) {
        if self.is_running() {
            self.show_toast(format!("Export failed: {error}"));
        } else {
            self.set_status(StatusLevel::Error, format!("Export failed: {error}"));
        }
    }

    pub fn is_exporting(&self) -> bool {
        self.export_task.is_some()
    }

    pub fn save_profile(&mut self, path: &std::path::Path) {
        let bind_addr = if self.bind_addr_input.trim().is_empty() {
            None
        } else {
            Some(self.bind_addr_input.trim().to_owned())
        };
        let mut profile = crate::profile::ScanProfile::new(
            self.targets_input.clone(),
            self.ports_input.clone(),
            self.timeout_ms,
            self.concurrency,
            self.banner_grabbing,
            self.banner_timeout_ms,
            self.protocol,
            bind_addr,
        );
        profile.exclusions = self.exclusions_input.clone();
        profile.resolve_ptr = self.resolve_ptr;
        profile.read_neighbor_cache = self.read_neighbor_cache;
        profile.ptr_soft_deadline_ms = self.ptr_soft_deadline_ms;
        match crate::profile::save(path, &profile) {
            Ok(()) => {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("profile.json");
                self.show_toast(format!("Saved profile {file_name}"));
                self.set_status(
                    StatusLevel::Success,
                    format!("Saved profile to {}", path.display()),
                );
            }
            Err(error) => {
                self.set_status(StatusLevel::Error, format!("Profile save failed: {error}"))
            }
        }
    }

    pub fn load_profile(&mut self, path: &std::path::Path) {
        match crate::profile::load(path) {
            Ok(profile) => {
                self.targets_input = profile.targets;
                self.ports_input = profile.ports;
                self.timeout_ms = profile.timeout_ms;
                self.concurrency = profile.concurrency;
                self.banner_grabbing = profile.banner_grabbing;
                self.banner_timeout_ms = profile.banner_timeout_ms;
                self.protocol = profile.protocol;
                self.exclusions_input = profile.exclusions;
                self.resolve_ptr = profile.resolve_ptr;
                self.read_neighbor_cache = profile.read_neighbor_cache;
                self.ptr_soft_deadline_ms = profile.ptr_soft_deadline_ms;
                if let Some(bind) = profile.bind_addr {
                    self.bind_addr_input = bind;
                } else {
                    self.bind_addr_input.clear();
                }
                self.input_cache = InputCache::new(
                    &self.targets_input,
                    &self.ports_input,
                    &self.bind_addr_input,
                    &self.exclusions_input,
                );
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("profile.json");
                self.show_toast(format!("Loaded profile {file_name}"));
                self.set_status(
                    StatusLevel::Success,
                    format!("Loaded profile from {}", path.display()),
                );
            }
            Err(error) => {
                self.set_status(StatusLevel::Error, format!("Profile load failed: {error}"));
            }
        }
    }

    pub(crate) fn clear_results_storage(&mut self) {
        self.results.clear();
        self.rows.clear();
        self.visible_indices.clear();
        self.host_groups.clear();
        self.host_info.clear();
        self.host_info_pending.clear();
        self.host_info_backlog.clear();
        self.host_info_inflight.clear();
        self.host_info_overdue.clear();
        self.host_info_stats = HostInfoStats::default();
        self.host_info_worker = None;
        self.cached_rtt_stats = None;
        self.rtt_stats_dirty = true;
        self.selected_result = None;
        self.view_dirty = false;
        self.dropped_open_results = 0;
    }

    pub fn set_status(&mut self, level: StatusLevel, status: String) {
        self.status_level = level;
        self.status = status;
    }

    pub fn show_toast(&mut self, message: impl Into<String>) {
        self.toasts.push(Toast {
            message: message.into(),
            created_at: Instant::now(),
            duration: Duration::from_millis(2200),
        });
    }

    pub fn copy_text_with_toast(&mut self, ctx: &egui::Context, text: String, toast_msg: String) {
        ctx.copy_text(text);
        self.show_toast(toast_msg);
    }

    pub fn render_toasts(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts
            .retain(|toast| now.duration_since(toast.created_at) < toast.duration);

        if self.toasts.is_empty() {
            return;
        }

        ctx.request_repaint_after(Duration::from_millis(40));

        let screen_rect = ctx
            .input(|i| i.raw.screen_rect)
            .unwrap_or(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1180.0, 760.0),
            ));
        egui::Area::new(egui::Id::unique("app_toasts"))
            .fixed_pos(egui::pos2(
                (screen_rect.max.x - 280.0).max(10.0),
                (screen_rect.max.y - 65.0).max(10.0),
            ))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                for toast in &self.toasts {
                    let elapsed = now.duration_since(toast.created_at).as_secs_f32();
                    let total = toast.duration.as_secs_f32();
                    let alpha = if elapsed > total - 0.4 {
                        ((total - elapsed) / 0.4).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };

                    egui::Frame::popup(ui.style())
                        .fill(egui::Color32::from_rgba_premultiplied(
                            0x25,
                            0x2a,
                            0x33,
                            (alpha * 245.0) as u8,
                        ))
                        .stroke(egui::Stroke::new(
                            1.0,
                            ui::theme::ACCENT.gamma_multiply(alpha),
                        ))
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .corner_radius(egui::CornerRadius::same(6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("✔")
                                        .color(ui::theme::SUCCESS.gamma_multiply(alpha))
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new(&toast.message)
                                        .color(egui::Color32::from_rgba_premultiplied(
                                            0xd8,
                                            0xdd,
                                            0xe6,
                                            (alpha * 255.0) as u8,
                                        ))
                                        .small(),
                                );
                            });
                        });
                }
            });
    }

    pub(crate) fn drain_events_with_budget(&mut self, max_events: usize) -> usize {
        if max_events == 0 {
            return 0;
        }

        let mut processed = 0;
        let mut finished = false;
        let mut pending_open = Vec::new();
        let mut finished_state = None;
        let mut failed_message = None;

        if let Some(handle) = &mut self.scan {
            while processed < max_events {
                let Some(event) = handle.try_recv() else {
                    break;
                };
                processed += 1;

                match event {
                    ScanEvent::Started { total } => {
                        self.total_probes = total;
                        self.done_probes = 0;
                    }
                    ScanEvent::Open(result) => pending_open.push(result),
                    ScanEvent::Progress {
                        done,
                        total,
                        counts,
                    } => {
                        self.done_probes = self.done_probes.max(done);
                        self.total_probes = total;
                        self.probe_counts = self.probe_counts.componentwise_max(counts);
                    }
                    ScanEvent::Finished {
                        done,
                        cancelled,
                        counts,
                    } => {
                        finished_state = Some((done, cancelled, counts));
                        finished = true;
                        break;
                    }
                    ScanEvent::Failed { message } => {
                        failed_message = Some(message);
                        finished = true;
                        break;
                    }
                }
            }
        }

        for result in pending_open {
            self.push_result(result);
        }

        if let Some((done, cancelled, counts)) = finished_state {
            self.done_probes = self.done_probes.max(done);
            self.probe_counts = self.probe_counts.componentwise_max(counts);
            let elapsed = self
                .scan_started_at
                .take()
                .map(|started_at| started_at.elapsed().as_secs_f64())
                .unwrap_or_default();
            let stored_note = if self.dropped_open_results > 0 {
                format!(
                    " · {} open results not stored (limit)",
                    self.dropped_open_results
                )
            } else {
                String::new()
            };
            if cancelled {
                self.set_status(
                    StatusLevel::Idle,
                    format!(
                        "Cancelled after {} probes in {:.1}s · {} open · {} refused · {} timed out · {} errors{stored_note}",
                        self.done_probes,
                        elapsed,
                        self.probe_counts.open,
                        self.probe_counts.refused,
                        self.probe_counts.timed_out,
                        self.probe_counts.errors
                    ),
                );
            } else {
                self.set_status(
                    StatusLevel::Success,
                    format!(
                        "Done in {:.1}s · {} probes · {} open · {} refused · {} timed out · {} errors{stored_note}",
                        elapsed,
                        self.done_probes,
                        self.probe_counts.open,
                        self.probe_counts.refused,
                        self.probe_counts.timed_out,
                        self.probe_counts.errors
                    ),
                );
            }
        } else if let Some(message) = failed_message {
            self.scan_started_at = None;
            self.set_status(StatusLevel::Error, format!("Scan failed: {message}"));
        }

        if finished {
            self.scan = None;
        }

        processed
    }
}

fn contains_ascii_case_insensitive(haystack: &str, needle_lower: &str) -> bool {
    let needle = needle_lower.as_bytes();
    if needle.is_empty() {
        return true;
    }
    let hay = haystack.as_bytes();
    if needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle.iter())
            .all(|(h, n)| h.to_ascii_lowercase() == *n)
    })
}

impl App {
    pub fn update_ui(&mut self, ui: &mut egui::Ui) {
        self.poll_export_task(ui.ctx());
        self.poll_oui_task(ui.ctx());
        let processed = self.drain_events_with_budget(MAX_EVENTS_PER_FRAME);
        self.drain_host_enrichment(ui.ctx());

        let is_cmd = |m: &egui::Modifiers| m.command || m.ctrl || m.mac_cmd;
        let command = ui.input(|i| is_cmd(&i.modifiers));
        let text_input_focused = ui.ctx().egui_wants_keyboard_input();
        let start_requested = ui.input(|i| is_cmd(&i.modifiers) && i.key_pressed(egui::Key::Enter));
        let stop_requested = !text_input_focused && ui.input(|i| i.key_pressed(egui::Key::Escape));
        let copy_requested = !text_input_focused
            && ui.input(|i| is_cmd(&i.modifiers) && i.key_pressed(egui::Key::C));
        let export_csv_requested = !text_input_focused
            && ui.input(|i| {
                is_cmd(&i.modifiers) && i.key_pressed(egui::Key::E) && !i.modifiers.shift
            });
        let export_json_requested = !text_input_focused
            && ui.input(|i| {
                is_cmd(&i.modifiers) && i.modifiers.shift && i.key_pressed(egui::Key::E)
            });

        if start_requested && self.can_start_scan() {
            self.start_scan();
        }
        if stop_requested && self.is_running() {
            self.stop_scan();
        }
        if export_csv_requested && !self.results.is_empty() {
            self.export_csv();
        }
        if export_json_requested && !self.results.is_empty() {
            self.export_json();
        }
        if copy_requested
            && let Some(index) = self.selected_result
            && let Some(result) = self.results.get(index)
        {
            let text = format!("{}:{}", result.ip, result.port);
            self.copy_text_with_toast(ui.ctx(), text.clone(), format!("Copied {text}"));
        }
        if copy_requested && self.selected_result.is_none() && !self.results.is_empty() {
            let all_results = self
                .results
                .iter()
                .map(|result| format!("{}:{}", result.ip, result.port))
                .collect::<Vec<_>>()
                .join("\n");
            let count = self.results.len();
            self.copy_text_with_toast(
                ui.ctx(),
                all_results,
                format!("Copied {count} result{}", if count == 1 { "" } else { "s" }),
            );
        }

        let arrow_down = !text_input_focused && ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
        let arrow_up = !text_input_focused && ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
        let home_pressed = !text_input_focused && ui.input(|i| i.key_pressed(egui::Key::Home));
        let end_pressed = !text_input_focused && ui.input(|i| i.key_pressed(egui::Key::End));

        self.ensure_results_view();
        if arrow_down && !self.visible_indices.is_empty() {
            let next_index = match self.selected_result {
                None => self.visible_indices[0],
                Some(current) => {
                    let pos = self
                        .visible_indices
                        .iter()
                        .position(|&idx| idx == current)
                        .unwrap_or(0);
                    let next_pos = (pos + 1).min(self.visible_indices.len() - 1);
                    self.visible_indices[next_pos]
                }
            };
            self.selected_result = Some(next_index);
            self.scroll_to_selected = true;
        }
        if arrow_up && !self.visible_indices.is_empty() {
            let prev_index = match self.selected_result {
                None => *self.visible_indices.last().unwrap(),
                Some(current) => {
                    let pos = self
                        .visible_indices
                        .iter()
                        .position(|&idx| idx == current)
                        .unwrap_or(0);
                    let prev_pos = pos.saturating_sub(1);
                    self.visible_indices[prev_pos]
                }
            };
            self.selected_result = Some(prev_index);
            self.scroll_to_selected = true;
        }
        if home_pressed && !self.visible_indices.is_empty() {
            self.selected_result = Some(self.visible_indices[0]);
            self.scroll_to_selected = true;
        }
        if end_pressed && !self.visible_indices.is_empty() {
            self.selected_result = self.visible_indices.last().copied();
            self.scroll_to_selected = true;
        }

        let row_action_requested = !text_input_focused
            && !command
            && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
        if row_action_requested
            && let Some(index) = self.selected_result
            && let Some(result) = self.results.get(index)
        {
            if let Some(url) = crate::ui::quick_actions::web_url_for(
                result.ip,
                result.port,
                &result.service,
                result.banner.as_deref(),
            ) {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                self.show_toast(format!("Opened {}:{} in browser", result.ip, result.port));
            } else {
                let text = format!("{}:{}", result.ip, result.port);
                self.copy_text_with_toast(ui.ctx(), text.clone(), format!("Copied {text}"));
            }
        }

        if command
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::F))
        {
            ui.memory_mut(|memory| memory.request_focus(egui::Id::unique("results_filter")));
        }

        if self.is_running() {
            let delay = if processed >= MAX_EVENTS_PER_FRAME {
                Duration::ZERO
            } else {
                Duration::from_millis(16)
            };
            ui.ctx().request_repaint_after(delay);
        }

        egui::Panel::left("controls")
            .resizable(true)
            .default_size(320.0)
            .size_range(280.0..=460.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui::sidebar::show(self, ui);
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui::results::show(self, ui);
        });

        self.render_toasts(ui.ctx());
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.update_ui(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn attach_mock_scan(
        app: &mut App,
    ) -> (
        mpsc::Sender<ScanEvent>,
        std::sync::mpsc::Sender<ScanEvent>,
        CancellationToken,
    ) {
        let (tx, rx) = mpsc::channel(64);
        let (terminal_tx, terminal_rx) = std::sync::mpsc::channel();
        let cancel = CancellationToken::new();
        app.scan = Some(ScanHandle::from_channels(rx, terminal_rx, cancel.clone()));
        (tx, terminal_tx, cancel)
    }

    fn mock_infinite_scan(_config: ScanConfig) -> Result<ScanHandle, String> {
        let (_tx, rx) = mpsc::channel(64);
        let (_terminal_tx, terminal_rx) = std::sync::mpsc::channel();
        let cancel = CancellationToken::new();
        Ok(ScanHandle::from_channels(rx, terminal_rx, cancel))
    }

    #[test]
    fn ptr_soft_deadline_warns_once_without_cancelling_worker() {
        let mut app = App::default_state();
        app.resolve_ptr = true;
        app.ptr_soft_deadline_ms = 500;
        let ip = IpAddr::from([192, 0, 2, 15]);
        app.host_info_inflight
            .insert(ip, Instant::now() - Duration::from_secs(2));
        let ctx = egui::Context::default();
        app.drain_host_enrichment(&ctx);
        app.drain_host_enrichment(&ctx);
        assert_eq!(app.host_info_stats.ptr_over_limit, 1);
        assert_eq!(app.ptr_currently_overdue(), 1);
        // A warning is not cancellation; the pending lookup remains accounted for.
        assert!(app.host_info_inflight.contains_key(&ip));
        app.clear_results_storage();
        assert_eq!(app.ptr_currently_overdue(), 0);
    }

    #[test]
    fn host_enrichment_retries_full_queue_without_losing_hosts() {
        // Local worker with PTR and cache access disabled: deterministic and
        // does not contact any network, but exercises > 512 queued addresses.
        let mut app = App::default_state();
        app.host_info_worker = Some(HostInfoWorker::start(false, false).unwrap());
        let total = 850usize;
        for n in 0..total {
            let ip = IpAddr::from([10, 42, (n / 256) as u8, (n % 256) as u8]);
            app.push_result(ScanResult::new(ip, 80, "test", Duration::from_millis(1)));
        }
        assert_eq!(app.host_info_pending_len(), total);
        let context = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.host_info_stats.completed != total {
            app.drain_host_enrichment(&context);
            assert!(Instant::now() < deadline, "host enrichment queue stalled");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.host_info.len(), total);
        assert_eq!(app.host_info_pending_len(), 0);
        assert_eq!(app.host_info_backlog_len(), 0);
        assert_eq!(app.host_info_stats.ptr_failed, 0);
        app.clear_results_storage();
        assert_eq!(app.host_info_stats.completed, 0);
        assert_eq!(app.host_info_backlog_len(), 0);
    }

    fn sample_result(host: u8, port: u16, service: &'static str, rtt_ms: u64) -> ScanResult {
        ScanResult::new(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, host)),
            port,
            service,
            Duration::from_millis(rtt_ms),
        )
    }

    #[test]
    fn loading_profile_updates_inputs_and_invalid_profile_is_atomic() {
        let path = std::env::temp_dir().join("ip_port_scanner_app_profile_test.json");
        let mut app = App::default_state();
        app.targets_input = "192.168.1.1".to_owned();
        app.ports_input = "22".to_owned();
        app.timeout_ms = 1200;
        app.concurrency = 64;

        let profile = crate::profile::ScanProfile::new(
            "127.0.0.1".to_owned(),
            "80,443".to_owned(),
            500,
            12,
            true,
            1500,
            crate::scanner::ScanProtocol::Udp,
            None,
        );
        crate::profile::save(&path, &profile).unwrap();
        app.load_profile(&path);
        assert_eq!(app.targets_input, "127.0.0.1");
        assert_eq!(app.ports_input, "80,443");
        assert_eq!(app.timeout_ms, 500);
        assert_eq!(app.concurrency, 12);
        assert!(app.banner_grabbing);
        assert_eq!(app.banner_timeout_ms, 1500);
        assert_eq!(app.protocol, crate::scanner::ScanProtocol::Udp);
        assert_eq!(app.refresh_input_preview().unwrap().ports, 2);

        std::fs::write(
            &path,
            r#"{"version":1,"targets":"http://invalid","ports":"80","timeout_ms":500,"concurrency":12}"#,
        )
        .unwrap();
        app.load_profile(&path);
        assert_eq!(app.targets_input, "127.0.0.1");
        assert_eq!(app.ports_input, "80,443");
        assert_eq!(app.status_level, StatusLevel::Error);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn monotonic_progress_and_frame_budget_in_ui_state() {
        let mut app = App::default_state();
        let (tx, terminal_tx, _cancel) = attach_mock_scan(&mut app);

        tx.blocking_send(ScanEvent::Started { total: 200 }).unwrap();
        tx.blocking_send(ScanEvent::Progress {
            done: 150,
            total: 200,
            counts: ProbeCounts {
                open: 1,
                open_filtered: 0,
                refused: 149,
                timed_out: 0,
                unreachable: 0,
                errors: 0,
            },
        })
        .unwrap();
        tx.blocking_send(ScanEvent::Progress {
            done: 100,
            total: 200,
            counts: ProbeCounts {
                open: 1,
                open_filtered: 0,
                refused: 99,
                timed_out: 0,
                unreachable: 0,
                errors: 0,
            },
        })
        .unwrap();
        tx.blocking_send(ScanEvent::Open(sample_result(1, 80, "http", 5)))
            .unwrap();
        drop(tx);
        terminal_tx
            .send(ScanEvent::Finished {
                done: 200,
                cancelled: false,
                counts: ProbeCounts {
                    open: 1,
                    open_filtered: 0,
                    refused: 199,
                    timed_out: 0,
                    unreachable: 0,
                    errors: 0,
                },
            })
            .unwrap();

        assert_eq!(app.drain_events_with_budget(2), 2);
        assert_eq!(app.done_probes, 150);
        assert_eq!(app.probe_counts.refused, 149);
        assert!(app.is_running());

        assert_eq!(app.drain_events_with_budget(2), 2);
        assert_eq!(app.done_probes, 150);
        assert_eq!(app.probe_counts.refused, 149);
        assert_eq!(app.results.len(), 1);
        assert!(app.is_running());

        assert_eq!(app.drain_events_with_budget(2), 1);
        assert_eq!(app.done_probes, 200);
        assert_eq!(app.probe_counts.done(), 200);
        assert_eq!(app.probe_counts.open, 1);
        assert_eq!(app.probe_counts.refused, 199);
        assert!(!app.is_running());
        assert_eq!(app.status_level, StatusLevel::Success);
        assert!(app.status.contains("199 refused"));
    }

    #[test]
    fn failed_event_sets_error_status_and_finishes_scan() {
        let mut app = App::default_state();
        let (tx, terminal_tx, _cancel) = attach_mock_scan(&mut app);
        drop(tx);
        terminal_tx
            .send(ScanEvent::Failed {
                message: "runtime failed".to_owned(),
            })
            .unwrap();

        app.drain_events_with_budget(MAX_EVENTS_PER_FRAME);
        assert!(!app.is_running());
        assert_eq!(app.status_level, StatusLevel::Error);
        assert!(app.status.contains("runtime failed"));
    }

    #[test]
    fn result_storage_stops_at_limit_and_counts_dropped_rows() {
        let mut app = App::default_state();
        for index in 0..MAX_STORED_RESULTS {
            app.push_result(sample_result(
                1,
                (index % u16::MAX as usize + 1) as u16,
                "unknown",
                1,
            ));
        }
        app.push_result(sample_result(1, 1, "unknown", 1));

        assert_eq!(app.results.len(), MAX_STORED_RESULTS);
        assert_eq!(app.rows.len(), MAX_STORED_RESULTS);
        assert_eq!(app.dropped_open_results, 1);

        app.clear_results();
        assert_eq!(app.dropped_open_results, 0);
    }

    #[test]
    fn results_view_caches_until_data_filter_or_sort_changes() {
        let mut app = App::default_state();
        app.push_result(sample_result(2, 443, "https", 20));
        app.push_result(sample_result(1, 80, "http", 5));
        app.push_result(sample_result(1, 22, "ssh", 12));

        app.ensure_results_view();
        assert_eq!(app.view_rebuilds(), 1);
        let ordered_ports: Vec<u16> = app
            .visible_indices()
            .iter()
            .map(|&idx| app.row(idx).raw.port)
            .collect();
        assert_eq!(ordered_ports, vec![22, 80, 443]);

        // Idle frames must reuse the cached view without sorting or filtering again.
        app.ensure_results_view();
        app.ensure_results_view();
        assert_eq!(app.view_rebuilds(), 1);

        app.results_filter = "HTTP".to_owned();
        app.ensure_results_view();
        assert_eq!(app.view_rebuilds(), 2);
        assert_eq!(app.visible_indices().len(), 2);

        app.sort_column = SortColumn::Latency;
        app.sort_ascending = false;
        app.ensure_results_view();
        assert_eq!(app.view_rebuilds(), 3);
        let first = app.row(app.visible_indices()[0]).raw.port;
        assert_eq!(first, 443);
    }

    #[test]
    fn unanswered_udp_does_not_pollute_cached_or_host_latency() {
        let mut app = App::default_state();
        let unanswered = ScanResult::udp(
            IpAddr::from([10, 0, 0, 10]),
            161,
            PortState::OpenFiltered,
            "snmp",
            Duration::from_millis(800),
            false,
        );
        app.push_result(unanswered);
        assert!(app.current_rtt_stats().is_none());
        app.ensure_results_view();
        assert_eq!(app.host_groups().len(), 1);
        assert!(!app.host_groups()[0].has_measured_rtt);

        app.push_result(ScanResult::new(
            IpAddr::from([10, 0, 0, 10]),
            80,
            "http",
            Duration::from_millis(12),
        ));
        let stats = app.current_rtt_stats().unwrap();
        assert_eq!(stats.count, 1);
        assert!((stats.min_ms - 12.0).abs() < 1e-4);
        app.ensure_results_view();
        assert_eq!(app.host_groups()[0].min_rtt, Duration::from_millis(12));
        assert!(app.host_groups()[0].has_measured_rtt);

        app.clear_results_storage();
        assert!(app.current_rtt_stats().is_none());
    }

    #[test]
    fn host_groups_aggregation_and_view_mode_toggle() {
        let mut app = App::default_state();
        assert_eq!(app.view_mode, ResultsViewMode::Flat);

        // Host 1: 10.0.0.1 with ports 80, 443, 8080
        app.push_result(sample_result(1, 80, "http", 10));
        app.push_result(sample_result(1, 443, "https", 15));
        app.push_result(sample_result(1, 8080, "http-alt", 20));

        // Host 2: 10.0.0.2 with port 22
        app.push_result(sample_result(2, 22, "ssh", 5));

        app.ensure_results_view();
        assert_eq!(app.host_groups().len(), 2);

        let g0 = &app.host_groups()[0];
        assert_eq!(g0.ip_text, "10.0.0.1");
        assert_eq!(g0.ports_count, 3);
        assert_eq!(g0.min_rtt, Duration::from_millis(10));
        assert!(g0.services_summary.contains("http"));

        let g1 = &app.host_groups()[1];
        assert_eq!(g1.ip_text, "10.0.0.2");
        assert_eq!(g1.ports_count, 1);
        assert_eq!(g1.min_rtt, Duration::from_millis(5));
        assert_eq!(g1.services_summary, "ssh");

        // Filter by 10.0.0.2
        app.results_filter = "10.0.0.2".to_owned();
        app.ensure_results_view();
        assert_eq!(app.host_groups().len(), 1);
        assert_eq!(app.host_groups()[0].ip_text, "10.0.0.2");

        // Switch to GroupedByHost
        app.results_filter.clear();
        app.view_mode = ResultsViewMode::GroupedByHost;
        app.ensure_results_view();

        // Render headless frame in GroupedByHost mode
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.update_ui(ui);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn clear_during_running_scan_preserves_stream_for_new_hits() {
        let mut app = App::default_state();
        let (tx, terminal_tx, cancel) = attach_mock_scan(&mut app);

        tx.blocking_send(ScanEvent::Started { total: 2 }).unwrap();
        tx.blocking_send(ScanEvent::Open(sample_result(1, 22, "ssh", 3)))
            .unwrap();
        app.drain_events_with_budget(10);
        assert_eq!(app.results.len(), 1);

        app.clear_results();
        assert!(app.results.is_empty());
        assert!(app.is_running());

        tx.blocking_send(ScanEvent::Open(sample_result(1, 80, "http", 4)))
            .unwrap();
        app.stop_scan();
        assert!(cancel.is_cancelled());
        assert!(app.is_stopping());
        drop(tx);
        terminal_tx
            .send(ScanEvent::Finished {
                done: 2,
                cancelled: true,
                counts: ProbeCounts {
                    open: 1,
                    open_filtered: 0,
                    refused: 1,
                    timed_out: 0,
                    unreachable: 0,
                    errors: 0,
                },
            })
            .unwrap();

        app.drain_events_with_budget(10);
        assert_eq!(app.results.len(), 1);
        assert_eq!(app.results[0].port, 80);
    }

    #[test]
    fn progress_text_includes_elapsed_rate_and_eta_while_running() {
        let mut app = App::default_state();
        let (_tx, _terminal_tx, _cancel) = attach_mock_scan(&mut app);
        app.scan_started_at = Some(Instant::now() - Duration::from_secs(2));
        app.total_probes = 100;
        app.done_probes = 50;

        let progress = app.progress_text();
        assert!(progress.contains("50/100 probes"));
        assert!(progress.contains("2.0s"));
        assert!(progress.contains("probes/s"));
        assert!(progress.contains("ETA"));
    }

    #[test]
    fn input_preview_reports_planned_scope_and_validation_errors() {
        let mut app = App::default_state();
        assert!(app.can_start_scan());
        assert_eq!(
            app.refresh_input_preview().unwrap(),
            ScopePreview {
                targets: 1,
                hostnames: 0,
                excluded: 0,
                ports: 3,
                probes: 3
            }
        );

        // Hostname in targets preview
        app.targets_input = "localhost\n127.0.0.1".to_owned();
        assert_eq!(
            app.refresh_input_preview().unwrap(),
            ScopePreview {
                targets: 2,
                hostnames: 1,
                excluded: 0,
                ports: 3,
                probes: 6
            }
        );

        app.ports_input = "0".to_owned();
        assert!(!app.can_start_scan());
        assert!(app.refresh_input_preview().unwrap_err().contains("Ports:"));

        app.start_scan();
        assert_eq!(app.status_level, StatusLevel::Error);
        assert!(!app.is_running());
    }

    #[test]
    fn exclusions_update_preview_and_reject_empty_scope() {
        let mut app = App::default_state();
        app.targets_input = "10.0.0.1-10.0.0.3".to_owned();
        app.ports_input = "80,443".to_owned();
        app.exclusions_input = "10.0.0.2".to_owned();
        let scope = app.refresh_input_preview().unwrap();
        assert_eq!(scope.excluded, 1);
        assert_eq!(scope.targets, 2);
        assert_eq!(scope.probes, 4);

        app.exclusions_input = "10.0.0.0/24".to_owned();
        assert!(!app.can_start_scan());
        assert!(
            app.refresh_input_preview()
                .unwrap_err()
                .contains("excluded")
        );

        app.exclusions_input = "not-an-ip".to_owned();
        assert!(
            app.refresh_input_preview()
                .unwrap_err()
                .contains("Exclusions:")
        );
    }

    #[test]
    fn bind_addr_validation_and_config_propagation() {
        let mut app = App::default_state();
        assert!(app.can_start_scan());

        // Valid IPv4 bind address
        app.bind_addr_input = "192.168.1.10".to_owned();
        assert!(app.can_start_scan());
        assert!(app.refresh_input_preview().is_ok());

        // Valid IPv6 bind address
        app.bind_addr_input = "::1".to_owned();
        assert!(app.can_start_scan());

        // Default / any keyword
        app.bind_addr_input = "default".to_owned();
        assert!(app.can_start_scan());

        app.bind_addr_input = "  any  ".to_owned();
        assert!(app.can_start_scan());

        // Invalid IP address syntax
        app.bind_addr_input = "999.999.999.999".to_owned();
        assert!(!app.can_start_scan());
        assert!(
            app.refresh_input_preview()
                .unwrap_err()
                .contains("Bind address:")
        );

        app.start_scan();
        assert_eq!(app.status_level, StatusLevel::Error);
        assert!(!app.is_running());

        // Valid bind address correctly propagated to ScanConfig
        app.bind_addr_input = "127.0.0.1".to_owned();
        let captured_bind = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_clone = captured_bind.clone();
        app.start_scan_with(move |cfg| {
            *captured_clone.lock().unwrap() = cfg.bind_addr;
            mock_infinite_scan(cfg)
        });
        assert_eq!(
            *captured_bind.lock().unwrap(),
            Some(IpAddr::from([127, 0, 0, 1]))
        );
    }

    #[test]
    fn app_handles_spawn_failure_gracefully() {
        let mut app = App::default_state();
        assert!(app.can_start_scan());

        app.start_scan_with(|_config| {
            Err("Failed to start scanner thread: synthetic OS error".to_owned())
        });

        assert_eq!(app.status_level, StatusLevel::Error);
        assert!(app.status.contains("Failed to start scanner thread"));
        assert!(app.status.contains("synthetic OS error"));
        assert!(!app.is_running());
    }

    #[test]
    fn cached_frame_with_20k_results_avoids_rebuilding() {
        let mut app = App::default_state();
        for idx in 0..20_000u32 {
            let ip = IpAddr::V4(Ipv4Addr::from(0x0a00_0001 + (idx / 100)));
            let port = ((idx % 100) + 1) as u16;
            app.push_result(ScanResult::new(
                ip,
                port,
                crate::scanner::probe::service_name(port),
                Duration::from_micros((idx % 5_000) as u64 + 100),
            ));
        }

        app.results_filter = "http".to_owned();
        app.sort_column = SortColumn::Latency;
        app.ensure_results_view();
        assert_eq!(app.view_rebuilds(), 1);
        assert!(!app.visible_indices().is_empty());

        for _ in 0..60 {
            app.ensure_results_view();
        }
        assert_eq!(app.view_rebuilds(), 1);
    }

    fn run_headless_frame(
        app: &mut App,
        ctx: &egui::Context,
        size: egui::Vec2,
        pixels_per_point: f32,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.set_pixels_per_point(pixels_per_point);
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw_input, |ui| {
            app.update_ui(ui);
        });
        output.textures_delta.clear();
        output
    }

    #[test]
    fn port_presets_interaction_and_protocol_switching() {
        let mut app = App::default_state();
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);

        // 1. Initial default is TCP, ports 22,80,443
        assert_eq!(app.protocol, ScanProtocol::Tcp);
        let preview = app.refresh_input_preview().unwrap();
        assert_eq!(preview.ports, 3);

        // 2. Apply Top 100 preset
        let top100 = crate::scanner::presets::TCP_PRESETS
            .iter()
            .find(|p| p.name == "Top 100")
            .unwrap();
        app.ports_input = top100.ports.to_owned();
        let preview_top100 = app.refresh_input_preview().unwrap();
        assert_eq!(preview_top100.ports, 100);

        // 3. Switch to UDP protocol
        app.protocol = ScanProtocol::Udp;
        let udp_presets = crate::scanner::presets::presets_for_protocol(app.protocol);
        assert_eq!(
            udp_presets.len(),
            crate::scanner::presets::UDP_PRESETS.len()
        );
        assert!(udp_presets.iter().any(|p| p.name == "Common UDP"));

        // 4. Merge/append another preset (Common UDP + Discovery)
        let common_udp = udp_presets.iter().find(|p| p.name == "Common UDP").unwrap();
        let discovery = udp_presets.iter().find(|p| p.name == "Discovery").unwrap();
        let merged = crate::scanner::presets::apply_preset(common_udp.ports, discovery.ports, true);
        app.ports_input = merged;
        let preview_udp = app.refresh_input_preview().unwrap();
        assert!(preview_udp.ports >= 15);

        // 5. Render a frame to ensure no panic in sidebar
        run_headless_frame(&mut app, &ctx, egui::vec2(1180.0, 760.0), 1.0, vec![]);
    }

    #[test]
    fn keyboard_navigation_and_shortcuts_test() {
        let mut app = App::default_state();
        app.set_spawner(mock_infinite_scan);
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);

        // 1. Ctrl+Enter starts scan
        assert!(!app.is_running());
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(app.is_running());

        // 2. Escape while running requests stop
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
                egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert!(app.is_stopping());

        // 3. Ctrl+C with selected row copies selected IP:Port
        app.scan = None;
        app.clear_results_storage();
        app.push_result(sample_result(1, 80, "http", 10));
        app.push_result(sample_result(2, 443, "https", 25));
        app.selected_result = Some(0);

        let output1 = run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::C,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(output1.platform_output.commands.iter().any(|cmd| matches!(
            cmd,
            egui::OutputCommand::CopyText(text) if text == "10.0.0.1:80"
        )));
        assert!(app.toasts.last().unwrap().message.contains("10.0.0.1:80"));

        // 4. Ctrl+C without selection copies all IP:Port entries
        app.selected_result = None;
        let output2 = run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::C,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(output2.platform_output.commands.iter().any(|cmd| matches!(
            cmd,
            egui::OutputCommand::CopyText(text) if text == "10.0.0.1:80\n10.0.0.2:443"
        )));
        assert!(app.toasts.last().unwrap().message.contains("2 results"));

        // 5. Ctrl+F requests focus for filter
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::F,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(ctx.memory(|m| m.has_focus(egui::Id::unique("results_filter"))));

        // 6. Escape while filter has focus clears filter
        app.results_filter = "test-query".to_owned();
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
                egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert!(app.results_filter.is_empty());

        // 7. Arrow keys navigate results table
        app.selected_result = None;
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
                egui::Event::Key {
                    key: egui::Key::ArrowDown,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert_eq!(app.selected_result, Some(0));

        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
                egui::Event::Key {
                    key: egui::Key::ArrowDown,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert_eq!(app.selected_result, Some(1));

        // 8. ArrowUp moves back
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
                egui::Event::Key {
                    key: egui::Key::ArrowUp,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert_eq!(app.selected_result, Some(0));

        // 9. End key selects last item
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
                egui::Event::Key {
                    key: egui::Key::End,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert_eq!(app.selected_result, Some(1));

        // 10. Home key selects first item
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
                egui::Event::Key {
                    key: egui::Key::Home,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert_eq!(app.selected_result, Some(0));

        // 11. Enter on web port (item 0 is port 80) triggers browser open
        let output_web = run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert!(
            output_web
                .platform_output
                .commands
                .iter()
                .any(|cmd| matches!(
                    cmd,
                    egui::OutputCommand::OpenUrl(open_url) if open_url.url == "http://10.0.0.1:80"
                ))
        );
        assert!(
            app.toasts
                .last()
                .unwrap()
                .message
                .contains("Opened 10.0.0.1:80")
        );
    }

    #[test]
    fn toast_notifications_render_and_expire() {
        let mut app = App::default_state();
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);

        app.show_toast("Copied 127.0.0.1:80");
        assert_eq!(app.toasts.len(), 1);

        let output = run_headless_frame(&mut app, &ctx, egui::vec2(1180.0, 760.0), 1.0, Vec::new());
        assert!(!output.shapes.is_empty());
        assert_eq!(app.toasts.len(), 1);

        // Advance time to expire toast
        app.toasts[0].created_at = Instant::now() - Duration::from_secs(5);
        let _ = run_headless_frame(&mut app, &ctx, egui::vec2(1180.0, 760.0), 1.0, Vec::new());
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn window_resizing_and_dpi_scaling_test() {
        let mut app = App::default_state();
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);

        for idx in 0..20_000u32 {
            let ip = IpAddr::V4(Ipv4Addr::from(0x0a00_0001 + (idx / 100)));
            let port = ((idx % 100) + 1) as u16;
            app.push_result(ScanResult::new(
                ip,
                port,
                crate::scanner::probe::service_name(port),
                Duration::from_micros((idx % 5_000) as u64 + 100),
            ));
        }

        let resolutions = [
            egui::vec2(880.0, 560.0),   // Minimum size
            egui::vec2(1180.0, 760.0),  // Default initial size
            egui::vec2(1920.0, 1080.0), // Full HD
            egui::vec2(3840.0, 2160.0), // 4K UHD
        ];

        let dpi_scales = [1.0, 1.25, 1.5, 2.0];

        for &res in &resolutions {
            for &dpi in &dpi_scales {
                let output = run_headless_frame(&mut app, &ctx, res, dpi, Vec::new());
                assert!(
                    output.shapes.len() > 10,
                    "Expected rendered shapes at size {res:?} and DPI {dpi}"
                );
            }
        }
    }

    #[test]
    fn theme_wcag_contrast_conformance_test() {
        fn srgb_to_linear(channel: u8) -> f64 {
            let c = channel as f64 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }

        fn relative_luminance(c: egui::Color32) -> f64 {
            0.2126 * srgb_to_linear(c.r())
                + 0.7152 * srgb_to_linear(c.g())
                + 0.0722 * srgb_to_linear(c.b())
        }

        fn contrast_ratio(c1: egui::Color32, c2: egui::Color32) -> f64 {
            let l1 = relative_luminance(c1);
            let l2 = relative_luminance(c2);
            let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
            (lighter + 0.05) / (darker + 0.05)
        }

        let panel_fill = egui::Color32::from_rgb(0x16, 0x18, 0x1d);
        let window_fill = egui::Color32::from_rgb(0x1c, 0x1f, 0x26);
        let text_color = egui::Color32::from_rgb(0xd8, 0xdd, 0xe6);

        let ratio_panel = contrast_ratio(text_color, panel_fill);
        let ratio_window = contrast_ratio(text_color, window_fill);
        let ratio_accent = contrast_ratio(crate::ui::theme::ACCENT, panel_fill);
        let ratio_error = contrast_ratio(crate::ui::theme::ERROR, panel_fill);

        // WCAG AAA requires >= 7.0 for normal text
        assert!(
            ratio_panel >= 7.0,
            "Text contrast on panel: {ratio_panel:.2} (requires >= 7.0 for WCAG AAA)"
        );
        assert!(
            ratio_window >= 7.0,
            "Text contrast on window: {ratio_window:.2} (requires >= 7.0 for WCAG AAA)"
        );
        // WCAG AA requires >= 4.5 for UI graphics / alerts
        assert!(
            ratio_accent >= 4.5,
            "Accent contrast on panel: {ratio_accent:.2} (requires >= 4.5 for WCAG AA)"
        );
        assert!(
            ratio_error >= 4.5,
            "Error contrast on panel: {ratio_error:.2} (requires >= 4.5 for WCAG AA)"
        );
    }

    #[test]
    fn stress_benchmark_events_sorting_and_rendering() {
        let mut app = App::default_state();
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);

        const COUNT: u32 = 20_000;

        // 1. Measure batch event ingestion
        let (tx, rx) = mpsc::channel(COUNT as usize + 64);
        let (terminal_tx, terminal_rx) = std::sync::mpsc::channel();
        let cancel = CancellationToken::new();
        app.scan = Some(ScanHandle::from_channels(rx, terminal_rx, cancel));

        let t_ingest_start = Instant::now();
        for idx in 0..COUNT {
            let ip = IpAddr::V4(Ipv4Addr::from(0x0a00_0001 + (idx / 100)));
            let port = ((idx % 100) + 1) as u16;
            let _ = tx.blocking_send(ScanEvent::Open(ScanResult::new(
                ip,
                port,
                crate::scanner::probe::service_name(port),
                Duration::from_micros((idx % 5_000) as u64 + 100),
            )));
        }
        drop(tx);
        let _ = terminal_tx.send(ScanEvent::Finished {
            done: COUNT as usize,
            cancelled: false,
            counts: ProbeCounts {
                open: COUNT as usize,
                open_filtered: 0,
                refused: 0,
                timed_out: 0,
                unreachable: 0,
                errors: 0,
            },
        });

        let mut drained_total = 0;
        loop {
            let count = app.drain_events_with_budget(MAX_EVENTS_PER_FRAME);
            drained_total += count;
            if count == 0 {
                break;
            }
        }
        let ingest_duration = t_ingest_start.elapsed();
        assert_eq!(drained_total, COUNT as usize + 1);
        assert_eq!(app.results.len(), COUNT as usize);

        // 2. Measure view build and sorting
        let t_sort_start = Instant::now();
        app.ensure_results_view();
        let initial_build_duration = t_sort_start.elapsed();

        let t_sort_latency = Instant::now();
        app.sort_column = SortColumn::Latency;
        app.sort_ascending = false;
        app.ensure_results_view();
        let sort_latency_duration = t_sort_latency.elapsed();

        // 3. Measure search / filtering across 20 000 items
        let t_filter_start = Instant::now();
        app.results_filter = "http".to_owned();
        app.ensure_results_view();
        let filter_duration = t_filter_start.elapsed();
        let filtered_count = app.visible_indices().len();
        assert!(filtered_count > 0 && filtered_count < COUNT as usize);

        app.results_filter.clear();
        app.ensure_results_view();

        // 4. Measure 60 full UI frame render cycles
        let mut frame_durations = Vec::with_capacity(60);
        for _ in 0..60 {
            let t_frame_start = Instant::now();
            let _ = run_headless_frame(&mut app, &ctx, egui::vec2(1180.0, 760.0), 1.0, Vec::new());
            frame_durations.push(t_frame_start.elapsed());
        }

        let min_frame = *frame_durations.iter().min().unwrap();
        let max_frame = *frame_durations.iter().max().unwrap();
        let total_frame_time: Duration = frame_durations.iter().sum();
        let avg_frame = total_frame_time / frame_durations.len() as u32;

        frame_durations.sort();
        let median_frame = frame_durations[frame_durations.len() / 2];
        let p95_frame = frame_durations[(frame_durations.len() as f64 * 0.95) as usize];
        let fps = 1.0 / avg_frame.as_secs_f64();

        println!("\n================ LOAD & STRESS BENCHMARK REPORT ================");
        println!("Platform / Specs: AMD Ryzen 5 5600X 6C/12T, 32GB RAM, Windows 10 x64");
        println!("Dataset: {} synthetic open port results", COUNT);
        println!(
            "- Event Ingestion (20k events via mpsc + budget): {:.2} ms ({:.0} events/sec)",
            ingest_duration.as_secs_f64() * 1000.0,
            COUNT as f64 / ingest_duration.as_secs_f64()
        );
        println!(
            "- Initial View Build & Sort (Address): {:.2} ms",
            initial_build_duration.as_secs_f64() * 1000.0
        );
        println!(
            "- View Re-Sort (Latency desc): {:.2} ms",
            sort_latency_duration.as_secs_f64() * 1000.0
        );
        println!(
            "- Substring Filter ('http', {} matches): {:.2} ms",
            filtered_count,
            filter_duration.as_secs_f64() * 1000.0
        );
        println!("- 60 Full egui UI Frames Rendering:");
        println!(
            "    Min Frame:    {:.3} ms",
            min_frame.as_secs_f64() * 1000.0
        );
        println!(
            "    Avg Frame:    {:.3} ms",
            avg_frame.as_secs_f64() * 1000.0
        );
        println!(
            "    Median Frame: {:.3} ms",
            median_frame.as_secs_f64() * 1000.0
        );
        println!(
            "    P95 Frame:    {:.3} ms",
            p95_frame.as_secs_f64() * 1000.0
        );
        println!(
            "    Max Frame:    {:.3} ms",
            max_frame.as_secs_f64() * 1000.0
        );
        println!(
            "    Effective UI FPS: {:.0} FPS (target: >= 60 FPS / <= 16.6 ms)",
            fps
        );
        println!("================================================================\n");

        // UI frame threshold: must be well under 16.6 ms (60 FPS).
        assert!(
            avg_frame < Duration::from_millis(16),
            "Average frame time {:?} exceeds 60 FPS budget (16.6ms)",
            avg_frame
        );
        assert!(
            p95_frame < Duration::from_millis(25),
            "P95 frame time {:?} exceeds interactive threshold",
            p95_frame
        );
    }

    #[test]
    fn linux_and_macos_hotkeys_and_contrast_verification_test() {
        let mut app = App::default_state();
        app.set_spawner(mock_infinite_scan);
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);

        // 1. Verify platform-specific modifier key label
        #[cfg(target_os = "macos")]
        assert_eq!(crate::ui::cmd_key_name(), "Cmd");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(crate::ui::cmd_key_name(), "Ctrl");

        // 2. Linux environment simulation (ctrl: true, mac_cmd: false, command: false)
        // 2a. Ctrl+Enter starts scan
        assert!(!app.is_running());
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    ctrl: true,
                    mac_cmd: false,
                    command: false,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        mac_cmd: false,
                        command: false,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(app.is_running(), "Linux Ctrl+Enter should start scan");

        // 2b. Escape stops scan
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::default()),
                egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert!(app.is_stopping(), "Escape should stop scan");

        // 2c. Linux Ctrl+C copies result and triggers toast
        app.scan = None;
        app.clear_results_storage();
        app.toasts.clear();
        app.push_result(sample_result(10, 8080, "http-proxy", 15));
        app.selected_result = Some(0);

        let out_linux_c = run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    ctrl: true,
                    mac_cmd: false,
                    command: false,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::C,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        mac_cmd: false,
                        command: false,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(
            out_linux_c
                .platform_output
                .commands
                .iter()
                .any(|cmd| matches!(
                    cmd,
                    egui::OutputCommand::CopyText(t) if t == "10.0.0.10:8080"
                ))
        );
        assert_eq!(app.toasts.len(), 1);
        assert!(app.toasts[0].message.contains("10.0.0.10:8080"));

        // 2d. Linux Ctrl+F requests focus on filter
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    ctrl: true,
                    mac_cmd: false,
                    command: false,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::F,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        mac_cmd: false,
                        command: false,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(ctx.memory(|m| m.has_focus(egui::Id::unique("results_filter"))));

        // 3. macOS environment simulation (mac_cmd: true, ctrl: false, command: false)
        // 3a. Cmd+Enter starts scan
        app.scan = None;
        app.toasts.clear();
        assert!(!app.is_running());
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    mac_cmd: true,
                    ctrl: false,
                    command: false,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        mac_cmd: true,
                        ctrl: false,
                        command: false,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(app.is_running(), "macOS Cmd+Enter should start scan");

        // 3b. Cmd+C copies result and triggers toast
        app.scan = None;
        app.clear_results_storage();
        app.toasts.clear();
        app.push_result(sample_result(10, 8080, "http-proxy", 15));
        app.selected_result = Some(0);
        let out_mac_c = run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    mac_cmd: true,
                    ctrl: false,
                    command: false,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::C,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        mac_cmd: true,
                        ctrl: false,
                        command: false,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(
            out_mac_c
                .platform_output
                .commands
                .iter()
                .any(|cmd| matches!(
                    cmd,
                    egui::OutputCommand::CopyText(t) if t == "10.0.0.10:8080"
                ))
        );
        assert_eq!(app.toasts.len(), 1);

        // 3c. macOS Cmd+F requests focus on filter
        run_headless_frame(
            &mut app,
            &ctx,
            egui::vec2(1180.0, 760.0),
            1.0,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers {
                    mac_cmd: true,
                    ctrl: false,
                    command: false,
                    ..Default::default()
                }),
                egui::Event::Key {
                    key: egui::Key::F,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        mac_cmd: true,
                        ctrl: false,
                        command: false,
                        ..Default::default()
                    },
                },
            ],
        );
        assert!(ctx.memory(|m| m.has_focus(egui::Id::unique("results_filter"))));

        // 4. Cross-platform Display Profiles & Gamma Verification (sRGB / Display P3 / Apple Linear)
        fn luminance_with_gamma(c: egui::Color32, gamma: f64) -> f64 {
            let to_linear = |v: u8| (v as f64 / 255.0).powf(gamma);
            0.2126 * to_linear(c.r()) + 0.7152 * to_linear(c.g()) + 0.0722 * to_linear(c.b())
        }

        fn contrast_with_gamma(c1: egui::Color32, c2: egui::Color32, gamma: f64) -> f64 {
            let l1 = luminance_with_gamma(c1, gamma);
            let l2 = luminance_with_gamma(c2, gamma);
            let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
            (lighter + 0.05) / (darker + 0.05)
        }

        let panel_fill = egui::Color32::from_rgb(0x16, 0x18, 0x1d);
        let window_fill = egui::Color32::from_rgb(0x1c, 0x1f, 0x26);
        let text_color = egui::Color32::from_rgb(0xd8, 0xdd, 0xe6);

        // Gamma 2.2: Standard Linux / X11 / Wayland / sRGB / macOS Display P3
        // Gamma 1.8: Legacy Apple Macintosh / Quartz gamma profile
        for &gamma in &[1.8, 2.2] {
            let ratio_panel = contrast_with_gamma(text_color, panel_fill, gamma);
            let ratio_window = contrast_with_gamma(text_color, window_fill, gamma);
            let ratio_accent = contrast_with_gamma(crate::ui::theme::ACCENT, panel_fill, gamma);
            let ratio_error = contrast_with_gamma(crate::ui::theme::ERROR, panel_fill, gamma);

            assert!(
                ratio_panel >= 7.0,
                "Text/Panel contrast {ratio_panel:.2} under gamma {gamma} must meet WCAG AAA (>= 7.0)"
            );
            assert!(
                ratio_window >= 7.0,
                "Text/Window contrast {ratio_window:.2} under gamma {gamma} must meet WCAG AAA (>= 7.0)"
            );
            assert!(
                ratio_accent >= 4.5,
                "Accent/Panel contrast {ratio_accent:.2} under gamma {gamma} must meet WCAG AA (>= 4.5)"
            );
            assert!(
                ratio_error >= 4.5,
                "Error/Panel contrast {ratio_error:.2} under gamma {gamma} must meet WCAG AA (>= 4.5)"
            );
        }
    }

    #[test]
    fn export_large_dataset_during_active_scan_and_io_completion_test() {
        let mut app = App::default_state();
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);

        const COUNT: usize = 20_000;
        for idx in 0..COUNT {
            app.push_result(sample_result(
                (idx % 250 + 1) as u8,
                (idx % 1000 + 1) as u16,
                "service",
                (idx % 50 + 1) as u64,
            ));
        }
        assert_eq!(app.results.len(), COUNT);

        // Simulate an active running scan
        let (_tx, rx) = mpsc::channel(64);
        let (_terminal_tx, terminal_rx) = std::sync::mpsc::channel();
        let cancel = CancellationToken::new();
        app.scan = Some(ScanHandle::from_channels(rx, terminal_rx, cancel));
        app.status_level = StatusLevel::Running;
        app.status = "Scanning...".to_owned();
        assert!(app.is_running());

        let temp_dir = std::env::temp_dir().join("ip_port_scanner_large_export_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let csv_path = temp_dir.join("large_results.csv");
        let json_path = temp_dir.join("large_results.json");

        // 1. Export CSV in background thread (simulating export_csv task spawn)
        let snapshot = app.results.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let path_clone = csv_path.clone();
        let t_start = Instant::now();
        let thread_handle = std::thread::Builder::new()
            .name("export-writer".to_owned())
            .spawn(move || {
                let res =
                    crate::export::save_csv(&path_clone, &snapshot).map_err(|err| err.to_string());
                let _ = sender.send((path_clone, COUNT, res));
            })
            .unwrap();
        app.export_task = Some(ExportTask { receiver });

        // While export is running in background, push new results into app (verifying snapshot isolation)
        for idx in 0..500 {
            app.push_result(sample_result(1, (1000 + idx) as u16, "new-service", 5));
        }
        assert_eq!(app.results.len(), COUNT + 500);

        // Wait for thread to finish and poll task in UI loop
        thread_handle.join().unwrap();
        let write_elapsed = t_start.elapsed();
        app.poll_export_task(&ctx);

        assert!(!app.is_exporting());
        assert!(app.is_running());
        assert_eq!(app.status_level, StatusLevel::Running);
        assert_eq!(app.last_export_path, Some(csv_path.clone()));
        assert!(
            app.toasts
                .iter()
                .any(|t| t.message.contains("Exported 20000 results"))
        );

        // Verify CSV on disk
        let csv_content = std::fs::read_to_string(&csv_path).unwrap();
        let line_count = csv_content.lines().count();
        assert_eq!(line_count, COUNT + 1);

        // 2. Export JSON in background thread
        let json_snapshot = app.results.clone();
        let total_json = json_snapshot.len();
        let (sender_json, receiver_json) = std::sync::mpsc::channel();
        let json_path_clone = json_path.clone();
        let thread_json = std::thread::Builder::new()
            .name("export-writer".to_owned())
            .spawn(move || {
                let res = crate::export::save_json(&json_path_clone, &json_snapshot)
                    .map_err(|err| err.to_string());
                let _ = sender_json.send((json_path_clone, total_json, res));
            })
            .unwrap();
        app.export_task = Some(ExportTask {
            receiver: receiver_json,
        });

        thread_json.join().unwrap();
        app.poll_export_task(&ctx);
        assert!(!app.is_exporting());

        // Verify JSON on disk
        let json_content = std::fs::read_to_string(&json_path).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_content).unwrap();
        assert_eq!(parsed.len(), total_json);

        println!(
            "Large CSV export of 20,000 rows completed in {:.2} ms",
            write_elapsed.as_secs_f64() * 1000.0
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
