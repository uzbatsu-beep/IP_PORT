pub mod interfaces;
pub mod presets;
pub mod scope;
pub mod probe;
pub mod services;
pub mod targets;
pub mod udp;

pub use interfaces::NetworkInterfaceInfo;
pub use targets::TargetList;
pub use udp::{PortState, ScanProtocol};

use std::future::Future;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Параметры одного запуска сканирования.
#[derive(Clone, Debug)]
pub struct ScanConfig {
    pub targets: TargetList,
    pub ports: Vec<u16>,
    pub timeout: Duration,
    pub concurrency: usize,
    pub banner_grabbing: bool,
    pub banner_timeout: Duration,
    pub protocol: ScanProtocol,
    pub bind_addr: Option<IpAddr>,
    pub exclusions: scope::Exclusions,
}

/// Найденный открытый порт.
#[derive(Clone, Debug, PartialEq)]
pub struct ScanResult {
    pub ip: IpAddr,
    pub port: u16,
    pub protocol: ScanProtocol,
    pub state: PortState,
    pub service: &'static str,
    pub rtt: Duration,
    pub banner: Option<String>,
    pub service_verified: bool,
}

#[allow(dead_code)]
impl ScanResult {
    pub fn new(ip: IpAddr, port: u16, service: &'static str, rtt: Duration) -> Self {
        Self {
            ip,
            port,
            protocol: ScanProtocol::Tcp,
            state: PortState::Open,
            service,
            rtt,
            banner: None,
            service_verified: false,
        }
    }

    pub fn with_banner(
        ip: IpAddr,
        port: u16,
        service: &'static str,
        rtt: Duration,
        banner: Option<String>,
        service_verified: bool,
    ) -> Self {
        Self {
            ip,
            port,
            protocol: ScanProtocol::Tcp,
            state: PortState::Open,
            service,
            rtt,
            banner,
            service_verified,
        }
    }

    pub fn udp(
        ip: IpAddr,
        port: u16,
        state: PortState,
        service: &'static str,
        rtt: Duration,
        verified: bool,
    ) -> Self {
        Self {
            ip,
            port,
            protocol: ScanProtocol::Udp,
            state,
            service,
            rtt,
            banner: None,
            service_verified: verified,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProbeCounts {
    pub open: usize,
    pub open_filtered: usize,
    pub refused: usize,
    pub timed_out: usize,
    pub unreachable: usize,
    pub errors: usize,
}

impl ProbeCounts {
    pub fn done(self) -> usize {
        self.open
            + self.open_filtered
            + self.refused
            + self.timed_out
            + self.unreachable
            + self.errors
    }

    pub fn componentwise_max(self, other: Self) -> Self {
        Self {
            open: self.open.max(other.open),
            open_filtered: self.open_filtered.max(other.open_filtered),
            refused: self.refused.max(other.refused),
            timed_out: self.timed_out.max(other.timed_out),
            unreachable: self.unreachable.max(other.unreachable),
            errors: self.errors.max(other.errors),
        }
    }
}

struct AtomicProbeCounts(Mutex<ProbeCounts>);

impl AtomicProbeCounts {
    fn new() -> Self {
        Self(Mutex::new(ProbeCounts::default()))
    }

    fn record(&self, outcome: &probe::ProbeOutcome) -> ProbeCounts {
        let mut counts = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let counter = match outcome {
            probe::ProbeOutcome::Open { .. } => &mut counts.open,
            probe::ProbeOutcome::OpenFiltered { .. } => &mut counts.open_filtered,
            probe::ProbeOutcome::Refused => &mut counts.refused,
            probe::ProbeOutcome::TimedOut => &mut counts.timed_out,
            probe::ProbeOutcome::Unreachable => &mut counts.unreachable,
            probe::ProbeOutcome::Error => &mut counts.errors,
        };
        *counter = counter.saturating_add(1);
        *counts
    }

    fn snapshot(&self) -> ProbeCounts {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// События, которые движок отправляет в UI.
#[derive(Clone, Debug)]
pub enum ScanEvent {
    Started {
        total: usize,
    },
    Open(ScanResult),
    Progress {
        done: usize,
        total: usize,
        counts: ProbeCounts,
    },
    Finished {
        done: usize,
        cancelled: bool,
        counts: ProbeCounts,
    },
    Failed {
        message: String,
    },
}

/// Управляющий дескриптор активного сканирования.
pub struct ScanHandle {
    rx: mpsc::Receiver<ScanEvent>,
    terminal_rx: std::sync::mpsc::Receiver<ScanEvent>,
    cancel: CancellationToken,
}

impl ScanHandle {
    #[cfg(test)]
    pub(crate) fn from_channels(
        rx: mpsc::Receiver<ScanEvent>,
        terminal_rx: std::sync::mpsc::Receiver<ScanEvent>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            rx,
            terminal_rx,
            cancel,
        }
    }

    pub fn try_recv(&mut self) -> Option<ScanEvent> {
        match self.rx.try_recv() {
            Ok(event) => Some(event),
            Err(mpsc::error::TryRecvError::Empty) => None,
            Err(mpsc::error::TryRecvError::Disconnected) => self.terminal_rx.try_recv().ok(),
        }
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

impl Drop for ScanHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl ScanConfig {
    pub fn total_probes(&self) -> Result<usize, String> {
        if self.targets.is_empty() || self.ports.is_empty() {
            return Err("At least one target and port are required".to_owned());
        }
        let remaining_explicit = self.targets.ips.iter()
            .filter(|ip| !self.exclusions.contains(ip))
            .count();
        let targets = remaining_explicit.saturating_add(self.targets.hostnames.len());
        if targets == 0 {
            return Err("Every target is excluded from the scan scope".to_owned());
        }
        checked_total(targets, self.ports.len())
    }
}

fn checked_total(targets: usize, ports: usize) -> Result<usize, String> {
    targets
        .checked_mul(ports)
        .ok_or_else(|| "Scan is too large for this platform".to_owned())
}

fn job_at(targets: &[IpAddr], ports: &[u16], index: usize) -> Option<(IpAddr, u16)> {
    if ports.is_empty() {
        return None;
    }
    Some((
        *targets.get(index / ports.len())?,
        ports[index % ports.len()],
    ))
}

/// Запускает сканирование в отдельном потоке с собственным Tokio-рантаймом.
pub fn spawn(config: ScanConfig) -> Result<ScanHandle, String> {
    let workers = config.concurrency.clamp(2, 16);
    let banner_config = if config.banner_grabbing {
        Some(probe::BannerConfig {
            timeout: config.banner_timeout,
            max_bytes: probe::MAX_BANNER_BYTES,
        })
    } else {
        None
    };
    let protocol = config.protocol;
    let bind_addr = config.bind_addr;
    spawn_with(
        config,
        move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(workers)
                .enable_all()
                .build()
                .map_err(|err| format!("Failed to start scanner runtime: {err}"))
        },
        move |ip, port, timeout| async move {
            match protocol {
                ScanProtocol::Tcp => {
                    probe::probe(ip, port, timeout, banner_config, bind_addr).await
                }
                ScanProtocol::Udp => probe::probe_udp(ip, port, timeout, bind_addr).await,
            }
        },
    )
}

fn spawn_with<R, P, Fut>(
    config: ScanConfig,
    build_runtime: R,
    probe_fn: P,
) -> Result<ScanHandle, String>
where
    R: FnOnce() -> Result<tokio::runtime::Runtime, String> + Send + 'static,
    P: Fn(IpAddr, u16, Duration) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = probe::ProbeOutcome> + Send + 'static,
{
    spawn_with_spawner(config, build_runtime, probe_fn, |task| {
        std::thread::Builder::new()
            .name("port-scanner".to_owned())
            .spawn(task)
            .map(|_| ())
    })
}

fn spawn_with_spawner<R, P, Fut, S>(
    config: ScanConfig,
    build_runtime: R,
    probe_fn: P,
    spawn_thread: S,
) -> Result<ScanHandle, String>
where
    R: FnOnce() -> Result<tokio::runtime::Runtime, String> + Send + 'static,
    P: Fn(IpAddr, u16, Duration) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = probe::ProbeOutcome> + Send + 'static,
    S: FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
{
    let total = config.total_probes()?;
    let (tx, rx) = mpsc::channel(4096);
    let (terminal_tx, terminal_rx) = std::sync::mpsc::channel();
    let cancel = CancellationToken::new();

    let thread_cancel = cancel.clone();
    spawn_thread(Box::new(move || {
        // Keep a sender alive until the terminal event is available: the UI drains
        // queued data before reading the separate, non-blocking terminal channel.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let runtime = build_runtime()?;
            runtime.block_on(run_with_probe(
                config,
                total,
                tx.clone(),
                thread_cancel,
                probe_fn,
            ))
        }));
        let terminal = match outcome {
            Ok(Ok(event)) => event,
            Ok(Err(message)) => ScanEvent::Failed { message },
            Err(_) => ScanEvent::Failed {
                message: "Scanner thread panicked".to_owned(),
            },
        };
        let _ = terminal_tx.send(terminal);
        drop(tx);
    }))
    .map_err(|err| format!("Failed to start scanner thread: {err}"))?;

    Ok(ScanHandle {
        rx,
        terminal_rx,
        cancel,
    })
}

#[cfg(test)]
async fn run(
    config: ScanConfig,
    total: usize,
    tx: mpsc::Sender<ScanEvent>,
    cancel: CancellationToken,
) -> Result<ScanEvent, String> {
    let banner_config = if config.banner_grabbing {
        Some(probe::BannerConfig {
            timeout: config.banner_timeout,
            max_bytes: probe::MAX_BANNER_BYTES,
        })
    } else {
        None
    };
    let protocol = config.protocol;
    let bind_addr = config.bind_addr;
    run_with_probe(
        config,
        total,
        tx,
        cancel,
        move |ip, port, timeout| async move {
            match protocol {
                ScanProtocol::Tcp => {
                    probe::probe(ip, port, timeout, banner_config, bind_addr).await
                }
                ScanProtocol::Udp => probe::probe_udp(ip, port, timeout, bind_addr).await,
            }
        },
    )
    .await
}

async fn run_with_probe<P, Fut>(
    config: ScanConfig,
    _estimated_total: usize,
    tx: mpsc::Sender<ScanEvent>,
    cancel: CancellationToken,
    probe_fn: P,
) -> Result<ScanEvent, String>
where
    P: Fn(IpAddr, u16, Duration) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = probe::ProbeOutcome> + Send + 'static,
{
    let mut resolved_targets = match targets::resolve_targets_with_cancel(
        &config.targets,
        targets::DEFAULT_DNS_TIMEOUT,
        &cancel,
    )
    .await?
    {
        Some(ips) => ips,
        None => {
            return Ok(ScanEvent::Finished {
                done: 0,
                cancelled: true,
                counts: ProbeCounts::default(),
            });
        }
    };

    // Apply scope blacklist after DNS A/AAAA resolution as well as direct IP input.
    resolved_targets.retain(|ip| !config.exclusions.contains(ip));
    if resolved_targets.is_empty() {
        return Err("All resolved targets match exclusion rules".to_owned());
    }
    let total = checked_total(resolved_targets.len(), config.ports.len())?;

    if tx.send(ScanEvent::Started { total }).await.is_err() {
        return Err("Scanner event receiver disconnected".to_owned());
    }

    let targets = Arc::new(resolved_targets);
    let ports = Arc::new(config.ports);
    let probe_fn = Arc::new(probe_fn);
    let next = Arc::new(AtomicUsize::new(0));
    let counts = Arc::new(AtomicProbeCounts::new());
    let workers = config.concurrency.clamp(1, 1024).min(total.max(1));

    let mut set = JoinSet::new();
    for _ in 0..workers {
        let targets = targets.clone();
        let ports = ports.clone();
        let probe_fn = probe_fn.clone();
        let next = next.clone();
        let counts = counts.clone();
        let cancel = cancel.clone();
        let tx = tx.clone();
        let timeout = config.timeout;

        set.spawn(async move {
            loop {
                if cancel.is_cancelled() {
                    break;
                }
                let idx = {
                    let mut curr = next.load(Ordering::Relaxed);
                    loop {
                        if curr >= total {
                            break None;
                        }
                        match next.compare_exchange_weak(
                            curr,
                            curr + 1,
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                        ) {
                            Ok(idx) => break Some(idx),
                            Err(actual) => curr = actual,
                        }
                    }
                };
                let Some(idx) = idx else {
                    break;
                };
                let (ip, port) = job_at(&targets, &ports, idx).expect("valid job index");

                let outcome = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    result = probe_fn(ip, port, timeout) => result,
                };
                if let probe::ProbeOutcome::Open {
                    rtt,
                    banner,
                    service_verified,
                    confirmed_service,
                } = &outcome
                {
                    let service = confirmed_service.unwrap_or_else(|| probe::service_name(port));
                    let result = ScanResult {
                        ip,
                        port,
                        protocol: config.protocol,
                        state: PortState::Open,
                        service,
                        rtt: *rtt,
                        banner: banner.clone(),
                        service_verified: *service_verified,
                    };
                    if !send_event(&tx, &cancel, ScanEvent::Open(result)).await {
                        break;
                    }
                } else if let probe::ProbeOutcome::OpenFiltered {
                    rtt,
                    confirmed_service,
                } = &outcome
                {
                    let service = confirmed_service.unwrap_or_else(|| probe::service_name(port));
                    let result = ScanResult {
                        ip,
                        port,
                        protocol: config.protocol,
                        state: PortState::OpenFiltered,
                        service,
                        rtt: *rtt,
                        banner: None,
                        service_verified: false,
                    };
                    if !send_event(&tx, &cancel, ScanEvent::Open(result)).await {
                        break;
                    }
                }
                let outcome_counts = counts.record(&outcome);
                let completed = outcome_counts.done();
                if (completed == total || completed.is_multiple_of(50))
                    && !send_event(
                        &tx,
                        &cancel,
                        ScanEvent::Progress {
                            done: completed,
                            total,
                            counts: outcome_counts,
                        },
                    )
                    .await
                {
                    break;
                }
            }
        });
    }

    while let Some(result) = set.join_next().await {
        if let Err(err) = result {
            cancel.cancel();
            set.abort_all();
            while set.join_next().await.is_some() {}
            return Err(format!("Scanner worker failed: {err}"));
        }
    }

    Ok(ScanEvent::Finished {
        done: counts.snapshot().done(),
        cancelled: cancel.is_cancelled(),
        counts: counts.snapshot(),
    })
}

async fn send_event(
    tx: &mpsc::Sender<ScanEvent>,
    cancel: &CancellationToken,
    event: ScanEvent,
) -> bool {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => false,
        result = tx.send(event) => result.is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn collect(handle: &mut ScanHandle) -> Vec<ScanEvent> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut events = Vec::new();
        loop {
            if let Some(event) = handle.try_recv() {
                let terminal =
                    matches!(event, ScanEvent::Finished { .. } | ScanEvent::Failed { .. });
                events.push(event);
                if terminal {
                    return events;
                }
            } else {
                assert!(Instant::now() < deadline, "scanner did not finish");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn jobs_cover_product_without_allocating_it() {
        let targets = [IpAddr::from([127, 0, 0, 1]), IpAddr::from([127, 0, 0, 2])];
        let ports = [22, 80, 443];
        let jobs: Vec<_> = (0..6)
            .map(|i| job_at(&targets, &ports, i).unwrap())
            .collect();
        assert_eq!(
            jobs,
            vec![
                (targets[0], 22),
                (targets[0], 80),
                (targets[0], 443),
                (targets[1], 22),
                (targets[1], 80),
                (targets[1], 443)
            ]
        );
        assert!(job_at(&targets, &ports, 6).is_none());
        assert!(job_at(&targets, &[], 0).is_none());

        let large_targets = vec![targets[0]; 65_536];
        let large_ports: Vec<u16> = (1..=u16::MAX).collect();
        let total = checked_total(large_targets.len(), large_ports.len()).unwrap();
        assert_eq!(
            job_at(&large_targets, &large_ports, total - 1),
            Some((targets[0], u16::MAX))
        );
        assert!(checked_total(usize::MAX, 2).is_err());
    }

    #[test]
    fn aggregates_probe_outcomes_without_emitting_closed_results() {
        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1])].into(),
            ports: vec![1, 2, 3, 4, 5],
            timeout: Duration::from_secs(1),
            concurrency: 1,
            banner_grabbing: false,
            banner_timeout: Duration::from_secs(1),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        let events = collect(
            &mut spawn_with(
                config,
                || {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string())
                },
                |_ip, port, _timeout| async move {
                    match port {
                        1 => probe::ProbeOutcome::open_simple(Duration::from_millis(4)),
                        2 => probe::ProbeOutcome::Refused,
                        3 => probe::ProbeOutcome::TimedOut,
                        4 => probe::ProbeOutcome::Unreachable,
                        _ => probe::ProbeOutcome::Error,
                    }
                },
            )
            .unwrap(),
        );

        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ScanEvent::Open(_)))
                .count(),
            1
        );
        assert!(matches!(
            events.last(),
            Some(ScanEvent::Finished {
                done: 5,
                cancelled: false,
                counts: ProbeCounts {
                    open: 1,
                    open_filtered: 0,
                    refused: 1,
                    timed_out: 1,
                    unreachable: 1,
                    errors: 1,
                },
            })
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            ScanEvent::Progress {
                done: 5,
                counts: ProbeCounts {
                    open: 1,
                    open_filtered: 0,
                    refused: 1,
                    timed_out: 1,
                    unreachable: 1,
                    errors: 1,
                },
                ..
            }
        )));
    }

    #[test]
    fn scans_local_ports_and_can_restart() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let open = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let accepting = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let accepting_thread = accepting.clone();
        let acceptor = std::thread::spawn(move || {
            while accepting_thread.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok(_) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(err) => panic!("accept failed: {err}"),
                }
            }
        });
        let unused = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed = unused.local_addr().unwrap().port();
        drop(unused);
        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1]); 26].into(),
            ports: vec![open, closed],
            timeout: Duration::from_millis(200),
            concurrency: 8,
            banner_grabbing: false,
            banner_timeout: Duration::from_secs(1),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        for _ in 0..2 {
            let events = collect(&mut spawn(config.clone()).unwrap());
            assert!(matches!(
                events.first(),
                Some(ScanEvent::Started { total: 52 })
            ));
            assert!(matches!(
                events.last(),
                Some(ScanEvent::Finished {
                    done: 52,
                    cancelled: false,
                    counts,
                }) if counts.done() == 52 && counts.open == 26
            ));
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, ScanEvent::Open(r) if r.port == open))
                    .count(),
                26
            );
            assert!(
                !events
                    .iter()
                    .any(|e| matches!(e, ScanEvent::Open(r) if r.port == closed))
            );
            assert!(events.iter().any(|e| matches!(
                e,
                ScanEvent::Progress {
                    done: 50,
                    total: 52,
                    counts: ProbeCounts { .. }
                }
            )));
        }
        accepting.store(false, Ordering::Relaxed);
        acceptor.join().unwrap();
    }

    #[tokio::test]
    async fn cancellation_unblocks_workers_with_full_channel() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1]); 20].into(),
            ports: vec![listener.local_addr().unwrap().port()],
            timeout: Duration::from_secs(1),
            concurrency: 4,
            banner_grabbing: false,
            banner_timeout: Duration::from_secs(1),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        let (tx, mut rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(run(config, 20, tx, cancel.clone()));
        // Started fills the channel; workers must be able to stop while their sends wait.
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
        let outcome = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(
            outcome,
            ScanEvent::Finished {
                cancelled: true,
                ..
            }
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(ScanEvent::Started { total: 20 })
        ));
    }

    #[test]
    fn invalid_configuration_reports_error() {
        let config = ScanConfig {
            targets: vec![].into(),
            ports: vec![80],
            timeout: Duration::from_secs(1),
            concurrency: 1,
            banner_grabbing: false,
            banner_timeout: Duration::from_secs(1),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        assert!(spawn(config).is_err());
    }

    #[test]
    fn cancelled_scan_finishes_and_next_scan_is_independent() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1]); 100].into(),
            ports: vec![listener.local_addr().unwrap().port()],
            timeout: Duration::from_secs(1),
            concurrency: 1,
            banner_grabbing: false,
            banner_timeout: Duration::from_secs(1),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        let mut handle = spawn(config.clone()).unwrap();
        handle.cancel();
        let events = collect(&mut handle);
        assert!(matches!(
            events.last(),
            Some(ScanEvent::Finished {
                cancelled: true,
                ..
            })
        ));

        let next = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1])].into(),
            ..config
        };
        let events = collect(&mut spawn(next).unwrap());
        assert!(matches!(
            events.last(),
            Some(ScanEvent::Finished {
                done: 1,
                cancelled: false,
                ..
            })
        ));
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, ScanEvent::Open(_)))
                .count(),
            1
        );
    }

    #[test]
    fn thread_spawn_failure_reports_error() {
        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1])].into(),
            ports: vec![80],
            timeout: Duration::from_secs(1),
            concurrency: 1,
            banner_grabbing: false,
            banner_timeout: Duration::from_secs(1),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        let result = spawn_with_spawner(
            config,
            || Err("unreachable".to_owned()),
            probe::probe_connect,
            |_task| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::OutOfMemory,
                    "synthetic thread spawn failure",
                ))
            },
        );

        match result {
            Err(err) => {
                assert!(err.contains("Failed to start scanner thread"));
                assert!(err.contains("synthetic thread spawn failure"));
            }
            Ok(_) => panic!("expected thread spawn failure"),
        }
    }

    #[test]
    fn runtime_init_failure_emits_failed_event() {
        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1])].into(),
            ports: vec![80],
            timeout: Duration::from_secs(1),
            concurrency: 1,
            banner_grabbing: false,
            banner_timeout: Duration::from_secs(1),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        let mut handle = spawn_with(
            config,
            || Err("synthetic runtime failure".to_owned()),
            probe::probe_connect,
        )
        .unwrap();
        let events = collect(&mut handle);
        assert!(matches!(
            events.as_slice(),
            [ScanEvent::Failed { message }] if message.contains("synthetic runtime failure")
        ));
    }

    #[test]
    fn worker_panic_emits_failed_event() {
        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1]); 8].into(),
            ports: vec![80],
            timeout: Duration::from_secs(1),
            concurrency: 2,
            banner_grabbing: false,
            banner_timeout: Duration::from_secs(1),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        let mut handle = spawn_with(
            config,
            || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|err| err.to_string())
            },
            |_ip, _port, _timeout| async move {
                panic!("synthetic probe panic");
                #[allow(unreachable_code)]
                probe::ProbeOutcome::Error
            },
        )
        .unwrap();
        let events = collect(&mut handle);
        assert!(matches!(
            events.last(),
            Some(ScanEvent::Failed { message }) if message.contains("Scanner worker failed")
        ));
    }

    #[test]
    fn cancellation_interrupts_in_flight_probe() {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1])].into(),
            ports: vec![80],
            timeout: Duration::from_secs(30),
            concurrency: 1,
            banner_grabbing: false,
            banner_timeout: Duration::from_secs(1),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        let mut handle = spawn_with(
            config,
            || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|err| err.to_string())
            },
            move |_ip, _port, _timeout| {
                let entered_tx = entered_tx.clone();
                let done_tx = done_tx.clone();
                async move {
                    struct Guard(std::sync::mpsc::Sender<()>);
                    impl Drop for Guard {
                        fn drop(&mut self) {
                            let _ = self.0.send(());
                        }
                    }
                    let _guard = Guard(done_tx);
                    let _ = entered_tx.send(());
                    std::future::pending::<probe::ProbeOutcome>().await
                }
            },
        )
        .unwrap();

        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let started = Instant::now();
        handle.cancel();
        assert!(handle.is_cancelled());
        done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let events = collect(&mut handle);
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            events.last(),
            Some(ScanEvent::Finished {
                done: 0,
                cancelled: true,
                ..
            })
        ));
    }

    #[test]
    fn scan_collects_and_verifies_service_banners() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = std::thread::spawn(move || {
            if let Ok((mut socket, _)) = listener.accept() {
                let _ = socket.write_all(b"SSH-2.0-OpenSSH_9.3\r\n");
                let _ = socket.flush();
                std::thread::sleep(Duration::from_millis(200));
            }
        });

        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1])].into(),
            ports: vec![port],
            timeout: Duration::from_secs(1),
            concurrency: 1,
            banner_grabbing: true,
            banner_timeout: Duration::from_millis(500),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };
        let mut handle = spawn(config).unwrap();
        let events = collect(&mut handle);
        server.join().unwrap();

        let open_event = events
            .into_iter()
            .find_map(|e| match e {
                ScanEvent::Open(r) => Some(r),
                _ => None,
            })
            .expect("open port event");

        assert_eq!(open_event.port, port);
        assert_eq!(open_event.service, "ssh");
        assert!(open_event.service_verified);
        assert_eq!(open_event.banner.as_deref(), Some("SSH-2.0-OpenSSH_9.3"));
    }

    #[tokio::test]
    async fn scan_executes_udp_scan_and_emits_open_and_filtered_states() {
        let server = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let open_port = server.local_addr().unwrap().port();

        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            if let Ok((len, peer)) = server.recv_from(&mut buf).await {
                let _ = server.send_to(&buf[..len], peer).await;
            }
        });

        let silent_server = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let silent_port = silent_server.local_addr().unwrap().port();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(800)).await;
            drop(silent_server);
        });

        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1])].into(),
            ports: vec![open_port, silent_port],
            timeout: Duration::from_millis(200),
            concurrency: 2,
            banner_grabbing: false,
            banner_timeout: Duration::from_millis(200),
            protocol: ScanProtocol::Udp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };

        let (tx, mut rx) = mpsc::channel(16);
        let cancel = CancellationToken::new();
        let _ = run(config, 2, tx, cancel).await.unwrap();

        let mut open_states = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let ScanEvent::Open(result) = event {
                open_states.push(result);
            }
        }

        assert!(
            open_states
                .iter()
                .any(|r| r.port == open_port && r.state == PortState::Open)
        );
        assert!(
            open_states
                .iter()
                .any(|r| r.port == silent_port && r.state == PortState::OpenFiltered)
        );
    }

    #[tokio::test]
    async fn scan_resolves_hostname_and_runs_scan() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let config = ScanConfig {
            targets: TargetList::new(Vec::new(), vec!["localhost".to_owned()]),
            ports: vec![port],
            timeout: Duration::from_millis(500),
            concurrency: 2,
            banner_grabbing: false,
            banner_timeout: Duration::from_millis(500),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };

        let (tx, mut rx) = mpsc::channel(16);
        let cancel = CancellationToken::new();
        let final_event = run(config, 1, tx, cancel).await.unwrap();

        assert!(matches!(
            final_event,
            ScanEvent::Finished {
                cancelled: false,
                ..
            }
        ));

        let mut opened = false;
        while let Ok(event) = rx.try_recv() {
            if let ScanEvent::Open(r) = event
                && r.port == port
            {
                opened = true;
            }
        }
        assert!(
            opened,
            "Expected to find open port on resolved localhost target"
        );
    }

    #[tokio::test]
    async fn scan_reports_error_when_hostname_fails_to_resolve() {
        let config = ScanConfig {
            targets: TargetList::new(
                Vec::new(),
                vec!["this-domain-does-not-exist-12345.invalid".to_owned()],
            ),
            ports: vec![80],
            timeout: Duration::from_millis(500),
            concurrency: 1,
            banner_grabbing: false,
            banner_timeout: Duration::from_millis(500),
            protocol: ScanProtocol::Tcp,
            bind_addr: None,
            exclusions: scope::Exclusions::default(),
        };

        let (tx, _rx) = mpsc::channel(16);
        let cancel = CancellationToken::new();
        let err = run(config, 1, tx, cancel).await.unwrap_err();
        assert!(err.contains("this-domain-does-not-exist-12345.invalid"));
        assert!(err.contains("Failed to resolve") || err.contains("timed out"));
    }
}
