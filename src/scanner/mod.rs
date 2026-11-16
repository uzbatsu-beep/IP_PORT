pub mod probe;
pub mod services;
pub mod targets;

use std::future::Future;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Параметры одного запуска сканирования.
#[derive(Clone, Debug)]
pub struct ScanConfig {
    pub targets: Vec<IpAddr>,
    pub ports: Vec<u16>,
    pub timeout: Duration,
    pub concurrency: usize,
}

/// Найденный открытый порт.
#[derive(Clone, Debug)]
pub struct ScanResult {
    pub ip: IpAddr,
    pub port: u16,
    pub service: &'static str,
    pub rtt: Duration,
}

/// События, которые движок отправляет в UI.
#[derive(Clone, Debug)]
pub enum ScanEvent {
    Started { total: usize },
    Open(ScanResult),
    Progress { done: usize, total: usize },
    Finished { done: usize, cancelled: bool },
    Failed { message: String },
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
        checked_total(self.targets.len(), self.ports.len())
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
    spawn_with(
        config,
        move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(workers)
                .enable_all()
                .build()
                .map_err(|err| format!("Failed to start scanner runtime: {err}"))
        },
        probe::probe,
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
    Fut: Future<Output = Option<Duration>> + Send + 'static,
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
    Fut: Future<Output = Option<Duration>> + Send + 'static,
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
    run_with_probe(config, total, tx, cancel, probe::probe).await
}

async fn run_with_probe<P, Fut>(
    config: ScanConfig,
    total: usize,
    tx: mpsc::Sender<ScanEvent>,
    cancel: CancellationToken,
    probe_fn: P,
) -> Result<ScanEvent, String>
where
    P: Fn(IpAddr, u16, Duration) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Option<Duration>> + Send + 'static,
{
    if tx.send(ScanEvent::Started { total }).await.is_err() {
        return Err("Scanner event receiver disconnected".to_owned());
    }

    let targets = Arc::new(config.targets);
    let ports = Arc::new(config.ports);
    let probe_fn = Arc::new(probe_fn);
    let next = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let workers = config.concurrency.clamp(1, 1024).min(total.max(1));

    let mut set = JoinSet::new();
    for _ in 0..workers {
        let targets = targets.clone();
        let ports = ports.clone();
        let probe_fn = probe_fn.clone();
        let next = next.clone();
        let done = done.clone();
        let cancel = cancel.clone();
        let tx = tx.clone();
        let timeout = config.timeout;

        set.spawn(async move {
            loop {
                if cancel.is_cancelled() {
                    break;
                }
                let Ok(idx) = next.try_update(Ordering::Relaxed, Ordering::Relaxed, |idx| {
                    (idx < total).then(|| idx + 1)
                }) else {
                    break;
                };
                let (ip, port) = job_at(&targets, &ports, idx).expect("valid job index");

                let outcome = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    rtt = probe_fn(ip, port, timeout) => rtt,
                };

                if let Some(rtt) = outcome {
                    let result = ScanResult {
                        ip,
                        port,
                        service: probe::service_name(port),
                        rtt,
                    };
                    if !send_event(&tx, &cancel, ScanEvent::Open(result)).await {
                        break;
                    }
                }

                let completed = done.fetch_add(1, Ordering::Relaxed) + 1;
                if (completed == total || completed.is_multiple_of(50))
                    && !send_event(
                        &tx,
                        &cancel,
                        ScanEvent::Progress {
                            done: completed,
                            total,
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
        done: done.load(Ordering::Relaxed),
        cancelled: cancel.is_cancelled(),
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
            targets: vec![IpAddr::from([127, 0, 0, 1]); 26],
            ports: vec![open, closed],
            timeout: Duration::from_millis(200),
            concurrency: 8,
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
                    cancelled: false
                })
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
                    total: 52
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
            targets: vec![IpAddr::from([127, 0, 0, 1]); 20],
            ports: vec![listener.local_addr().unwrap().port()],
            timeout: Duration::from_secs(1),
            concurrency: 4,
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
            targets: vec![],
            ports: vec![80],
            timeout: Duration::from_secs(1),
            concurrency: 1,
        };
        assert!(spawn(config).is_err());
    }

    #[test]
    fn cancelled_scan_finishes_and_next_scan_is_independent() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let config = ScanConfig {
            targets: vec![IpAddr::from([127, 0, 0, 1]); 100],
            ports: vec![listener.local_addr().unwrap().port()],
            timeout: Duration::from_secs(1),
            concurrency: 1,
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
            targets: vec![IpAddr::from([127, 0, 0, 1])],
            ..config
        };
        let events = collect(&mut spawn(next).unwrap());
        assert!(matches!(
            events.last(),
            Some(ScanEvent::Finished {
                done: 1,
                cancelled: false
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
            targets: vec![IpAddr::from([127, 0, 0, 1])],
            ports: vec![80],
            timeout: Duration::from_secs(1),
            concurrency: 1,
        };
        let result = spawn_with_spawner(
            config,
            || Err("unreachable".to_owned()),
            probe::probe,
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
            targets: vec![IpAddr::from([127, 0, 0, 1])],
            ports: vec![80],
            timeout: Duration::from_secs(1),
            concurrency: 1,
        };
        let mut handle = spawn_with(
            config,
            || Err("synthetic runtime failure".to_owned()),
            probe::probe,
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
            targets: vec![IpAddr::from([127, 0, 0, 1]); 8],
            ports: vec![80],
            timeout: Duration::from_secs(1),
            concurrency: 2,
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
            targets: vec![IpAddr::from([127, 0, 0, 1])],
            ports: vec![80],
            timeout: Duration::from_secs(30),
            concurrency: 1,
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
                    std::future::pending::<Option<Duration>>().await
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
                cancelled: true
            })
        ));
    }
}
