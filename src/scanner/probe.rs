use std::io::ErrorKind;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Максимальный размер буфера для чтения баннера сервиса (защита от переполнения памяти).
pub const MAX_BANNER_BYTES: usize = 256;

/// Таймаут по умолчанию для ожидания ответа сервиса.
pub const DEFAULT_BANNER_TIMEOUT: Duration = Duration::from_millis(1000);

/// Настройки для опционального захвата баннеров сервисов.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BannerConfig {
    pub timeout: Duration,
    pub max_bytes: usize,
}

impl Default for BannerConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_BANNER_TIMEOUT,
            max_bytes: MAX_BANNER_BYTES,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProbeOutcome {
    Open {
        rtt: Duration,
        banner: Option<String>,
        service_verified: bool,
        confirmed_service: Option<&'static str>,
    },
    OpenFiltered {
        rtt: Duration,
        confirmed_service: Option<&'static str>,
    },
    Refused,
    TimedOut,
    Unreachable,
    Error,
}

impl ProbeOutcome {
    pub fn open_simple(rtt: Duration) -> Self {
        Self::Open {
            rtt,
            banner: None,
            service_verified: false,
            confirmed_service: None,
        }
    }
}

pub fn classify_connect_error(error: &std::io::Error) -> ProbeOutcome {
    if error.kind() == ErrorKind::ConnectionRefused {
        return ProbeOutcome::Refused;
    }

    if error.kind() == ErrorKind::NetworkUnreachable || error.kind() == ErrorKind::HostUnreachable {
        return ProbeOutcome::Unreachable;
    }

    #[cfg(target_os = "windows")]
    if let Some(code) = error.raw_os_error() {
        // WSAEAFNOSUPPORT (10047), WSAENETUNREACH (10051), WSAEHOSTUNREACH (10065)
        if code == 10047 || code == 10051 || code == 10065 {
            return ProbeOutcome::Unreachable;
        }
    }

    #[cfg(not(target_os = "windows"))]
    if let Some(code) = error.raw_os_error() {
        // EAFNOSUPPORT (97), ENETUNREACH (101), EHOSTUNREACH (113)
        if code == 97 || code == 101 || code == 113 {
            return ProbeOutcome::Unreachable;
        }
    }

    ProbeOutcome::Error
}

/// Очищает и форматирует полученные байты баннера в безопасную однострочную строку.
pub fn sanitize_banner(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    let lossy = String::from_utf8_lossy(bytes);
    let mut sanitized = String::with_capacity(lossy.len());
    let mut in_whitespace = false;

    for ch in lossy.chars() {
        if ch.is_control() || ch == '\x7F' || ch.is_whitespace() {
            if !in_whitespace {
                sanitized.push(' ');
                in_whitespace = true;
            }
        } else {
            sanitized.push(ch);
            in_whitespace = false;
        }
    }

    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Ограничиваем длину безопасной строкой до 160 символов
    let truncated: String = trimmed.chars().take(160).collect();
    Some(truncated)
}

/// Определяет подтверждённый сервис по содержимому баннера.
pub fn infer_service(banner: &str) -> Option<&'static str> {
    let lower = banner.to_ascii_lowercase();
    if banner.starts_with("SSH-") {
        Some("ssh")
    } else if banner.starts_with("HTTP/")
        || lower.contains("server: ")
        || lower.contains("<html")
        || lower.contains("bad request")
    {
        Some("http")
    } else if banner.starts_with("220")
        && (lower.contains("ftp")
            || lower.contains("filezilla")
            || lower.contains("vsftpd")
            || lower.contains("proftpd")
            || lower.contains("pure-ftpd"))
    {
        Some("ftp")
    } else if (banner.starts_with("220") || lower.starts_with("220 "))
        && (lower.contains("smtp")
            || lower.contains("esmtp")
            || lower.contains("postfix")
            || lower.contains("exim")
            || lower.contains("sendmail")
            || lower.contains("mail"))
    {
        Some("smtp")
    } else if banner.starts_with("RFB ") {
        Some("vnc")
    } else if banner.starts_with("+OK") && (lower.contains("pop") || lower.contains("dovecot")) {
        Some("pop3")
    } else if banner.starts_with("* OK") && (lower.contains("imap") || lower.contains("dovecot")) {
        Some("imap")
    } else if banner.starts_with("+PONG")
        || banner.starts_with("-ERR")
        || banner.starts_with("-NOAUTH")
        || banner.starts_with("$-1")
    {
        Some("redis")
    } else if lower.contains("mariadb") || lower.contains("mysql") {
        Some("mysql")
    } else if lower.contains("bittorrent") {
        Some("bittorrent")
    } else {
        None
    }
}

/// Читает приветственный баннер из потока или посылает проверочный запрос.
pub async fn grab_banner(
    stream: &mut TcpStream,
    timeout: Duration,
    max_bytes: usize,
) -> Option<String> {
    let mut buf = vec![0u8; max_bytes.clamp(1, 4096)];

    // 1. Ожидаем приветственное сообщение от сервисов с немедленным баннером (SSH, FTP, SMTP, VNC).
    let greeting_wait = Duration::from_millis(250).min(timeout);
    let initial_read = tokio::time::timeout(greeting_wait, stream.read(&mut buf)).await;

    let bytes_read = match initial_read {
        Ok(Ok(n)) if n > 0 => n,
        _ => {
            // 2. Если приветствия не поступило, отправляем легкий HTTP probe для request-response сервисов.
            let remaining = timeout.saturating_sub(greeting_wait);
            if remaining.is_zero() {
                return None;
            }
            if stream.write_all(b"HEAD / HTTP/1.0\r\n\r\n").await.is_err() {
                return None;
            }
            match tokio::time::timeout(remaining, stream.read(&mut buf)).await {
                Ok(Ok(n)) if n > 0 => n,
                _ => return None,
            }
        }
    };

    sanitize_banner(&buf[..bytes_read])
}

/// Пробует установить TCP-соединение и классифицирует исход попытки.
/// При передаче `banner_config` выполняет опциональный захват баннера.
pub async fn probe(
    ip: IpAddr,
    port: u16,
    timeout: Duration,
    banner_config: Option<BannerConfig>,
) -> ProbeOutcome {
    let started = Instant::now();
    match tokio::time::timeout(timeout, TcpStream::connect((ip, port))).await {
        Ok(Ok(mut stream)) => {
            let rtt = started.elapsed();
            if let Some(config) = banner_config {
                let banner = grab_banner(&mut stream, config.timeout, config.max_bytes).await;
                let (service_verified, confirmed_service) = match &banner {
                    Some(b) => match infer_service(b) {
                        Some(svc) => (true, Some(svc)),
                        None => (false, None),
                    },
                    None => (false, None),
                };
                ProbeOutcome::Open {
                    rtt,
                    banner,
                    service_verified,
                    confirmed_service,
                }
            } else {
                ProbeOutcome::open_simple(rtt)
            }
        }
        Ok(Err(error)) => classify_connect_error(&error),
        Err(_) => ProbeOutcome::TimedOut,
    }
}

/// Упрощенный вызов TCP connect пробы без захвата баннера.
#[allow(dead_code)]
pub async fn probe_connect(ip: IpAddr, port: u16, timeout: Duration) -> ProbeOutcome {
    probe(ip, port, timeout, None).await
}

/// Выполняет UDP пробу и преобразует результат в ProbeOutcome.
pub async fn probe_udp(ip: IpAddr, port: u16, timeout: Duration) -> ProbeOutcome {
    match super::udp::probe_udp(ip, port, timeout).await {
        super::udp::UdpProbeOutcome::Open { rtt, service, .. } => ProbeOutcome::Open {
            rtt,
            banner: None,
            service_verified: true,
            confirmed_service: service,
        },
        super::udp::UdpProbeOutcome::OpenFiltered => ProbeOutcome::OpenFiltered {
            rtt: timeout,
            confirmed_service: None,
        },
        super::udp::UdpProbeOutcome::Closed => ProbeOutcome::Refused,
        super::udp::UdpProbeOutcome::Unreachable => ProbeOutcome::Unreachable,
        super::udp::UdpProbeOutcome::Error => ProbeOutcome::Error,
    }
}

/// Определение сервиса по номеру порта через базу соответствий Nmap/IANA.
pub fn service_name(port: u16) -> &'static str {
    super::services::service_name(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detects_open_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(matches!(
            probe(
                IpAddr::from([127, 0, 0, 1]),
                port,
                Duration::from_secs(1),
                None
            )
            .await,
            ProbeOutcome::Open { .. }
        ));
    }

    #[tokio::test]
    async fn detects_open_port_ipv6_loopback() {
        let listener = match tokio::net::TcpListener::bind("[::1]:0").await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Skipping IPv6 loopback test (IPv6 not available on host: {e})");
                return;
            }
        };
        let port = listener.local_addr().unwrap().port();
        assert!(matches!(
            probe("::1".parse().unwrap(), port, Duration::from_secs(1), None).await,
            ProbeOutcome::Open { .. }
        ));
    }

    #[tokio::test]
    async fn grabs_greeting_banner_and_identifies_ssh() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let _ = socket.write_all(b"SSH-2.0-OpenSSH_9.0p1\r\n").await;
            }
        });

        let outcome = probe(
            IpAddr::from([127, 0, 0, 1]),
            port,
            Duration::from_secs(1),
            Some(BannerConfig {
                timeout: Duration::from_millis(500),
                max_bytes: 256,
            }),
        )
        .await;

        match outcome {
            ProbeOutcome::Open {
                banner,
                service_verified,
                confirmed_service,
                ..
            } => {
                assert_eq!(banner.as_deref(), Some("SSH-2.0-OpenSSH_9.0p1"));
                assert!(service_verified);
                assert_eq!(confirmed_service, Some("ssh"));
            }
            _ => panic!("Expected Open with SSH banner, got {outcome:?}"),
        }
    }

    #[tokio::test]
    async fn grabs_probe_triggered_banner_and_identifies_http() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 128];
                let _ = socket.read(&mut buf).await;
                let _ = socket
                    .write_all(b"HTTP/1.0 200 OK\r\nServer: CustomNginx/1.2\r\n\r\n")
                    .await;
            }
        });

        let outcome = probe(
            IpAddr::from([127, 0, 0, 1]),
            port,
            Duration::from_secs(1),
            Some(BannerConfig {
                timeout: Duration::from_millis(500),
                max_bytes: 256,
            }),
        )
        .await;

        match outcome {
            ProbeOutcome::Open {
                banner,
                service_verified,
                confirmed_service,
                ..
            } => {
                assert!(banner.as_deref().unwrap().contains("HTTP/1.0 200 OK"));
                assert!(service_verified);
                assert_eq!(confirmed_service, Some("http"));
            }
            _ => panic!("Expected Open with HTTP banner, got {outcome:?}"),
        }
    }

    #[tokio::test]
    async fn handles_silent_server_timeout_without_dropping_open_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((_socket, _)) = listener.accept().await {
                // Keep connection open without writing anything
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        });

        let outcome = probe(
            IpAddr::from([127, 0, 0, 1]),
            port,
            Duration::from_secs(1),
            Some(BannerConfig {
                timeout: Duration::from_millis(80),
                max_bytes: 256,
            }),
        )
        .await;

        match outcome {
            ProbeOutcome::Open {
                banner,
                service_verified,
                confirmed_service,
                ..
            } => {
                assert_eq!(banner, None);
                assert!(!service_verified);
                assert_eq!(confirmed_service, None);
            }
            _ => panic!("Expected Open without banner, got {outcome:?}"),
        }
    }

    #[test]
    fn sanitizes_and_truncates_banners() {
        assert_eq!(sanitize_banner(b""), None);
        assert_eq!(sanitize_banner(b"   \r\n\t  "), None);
        assert_eq!(
            sanitize_banner(b"SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.1\r\n"),
            Some("SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.1".to_owned())
        );
        assert_eq!(
            sanitize_banner(b"220 FTP Server\r\nWelcome\tUser\0!"),
            Some("220 FTP Server Welcome User !".to_owned())
        );

        let long_banner = vec![b'A'; 300];
        let sanitized = sanitize_banner(&long_banner).unwrap();
        assert_eq!(sanitized.len(), 160);
    }

    #[test]
    fn infers_known_service_signatures() {
        assert_eq!(infer_service("SSH-2.0-OpenSSH"), Some("ssh"));
        assert_eq!(infer_service("HTTP/1.1 200 OK"), Some("http"));
        assert_eq!(infer_service("220 ProFTPD 1.3 Server"), Some("ftp"));
        assert_eq!(
            infer_service("220 mail.example.com ESMTP Postfix"),
            Some("smtp")
        );
        assert_eq!(infer_service("RFB 003.008"), Some("vnc"));
        assert_eq!(infer_service("+OK Dovecot ready"), Some("pop3"));
        assert_eq!(infer_service("* OK Dovecot ready"), Some("imap"));
        assert_eq!(infer_service("+PONG"), Some("redis"));
        assert_eq!(infer_service("-ERR unknown command"), Some("redis"));
        assert_eq!(infer_service("5.5.5-10.4.11-MariaDB"), Some("mysql"));
        assert_eq!(infer_service("Custom proprietary protocol 1.0"), None);
    }

    #[test]
    fn classifies_connection_refused_error() {
        assert_eq!(
            classify_connect_error(&std::io::Error::from(ErrorKind::ConnectionRefused)),
            ProbeOutcome::Refused
        );
        assert_eq!(
            classify_connect_error(&std::io::Error::from(ErrorKind::NetworkUnreachable)),
            ProbeOutcome::Unreachable
        );
        assert_eq!(
            classify_connect_error(&std::io::Error::from(ErrorKind::HostUnreachable)),
            ProbeOutcome::Unreachable
        );
        assert_eq!(
            classify_connect_error(&std::io::Error::from(ErrorKind::PermissionDenied)),
            ProbeOutcome::Error
        );
    }

    #[test]
    fn names_known_services() {
        assert_eq!(service_name(22), "ssh");
        assert_eq!(service_name(1900), "ssdp");
        assert_eq!(service_name(3702), "ws-discovery");
        assert_eq!(service_name(5353), "mdns");
        assert_eq!(service_name(8200), "trivnet1");
        assert_eq!(service_name(65000), "unknown");
    }
}
