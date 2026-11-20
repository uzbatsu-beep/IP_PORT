use std::io::ErrorKind;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Максимальный размер буфера для чтения баннера сервиса (защита от переполнения памяти).
pub const MAX_BANNER_BYTES: usize = 2048;

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

    if error.kind() == ErrorKind::NetworkUnreachable
        || error.kind() == ErrorKind::HostUnreachable
        || error.kind() == ErrorKind::AddrNotAvailable
    {
        return ProbeOutcome::Unreachable;
    }

    #[cfg(target_os = "windows")]
    if let Some(code) = error.raw_os_error() {
        // WSAEAFNOSUPPORT (10047), WSAEADDRNOTAVAIL (10049), WSAENETUNREACH (10051), WSAEHOSTUNREACH (10065)
        if code == 10047 || code == 10049 || code == 10051 || code == 10065 {
            return ProbeOutcome::Unreachable;
        }
    }

    #[cfg(not(target_os = "windows"))]
    if let Some(code) = error.raw_os_error() {
        // EAFNOSUPPORT (97), EADDRNOTAVAIL (99), ENETUNREACH (101), EHOSTUNREACH (113)
        if code == 97 || code == 99 || code == 101 || code == 113 {
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

/// Декодирует основные именованные и числовые HTML-сущности.
pub fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::with_capacity(10);
            let mut closed = false;
            while let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next();
                    closed = true;
                    break;
                } else if next_ch.is_alphanumeric() || next_ch == '#' {
                    entity.push(next_ch);
                    chars.next();
                    if entity.len() > 10 {
                        break;
                    }
                } else {
                    break;
                }
            }

            if closed {
                match entity.as_str() {
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    "apos" => out.push('\''),
                    "nbsp" => out.push(' '),
                    "mdash" => out.push('—'),
                    "ndash" => out.push('–'),
                    "copy" => out.push('©'),
                    _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                        if let Ok(code) = u32::from_str_radix(&entity[2..], 16)
                            && let Some(c) = char::from_u32(code)
                        {
                            out.push(c);
                        } else {
                            out.push('&');
                            out.push_str(&entity);
                            out.push(';');
                        }
                    }
                    _ if entity.starts_with('#') => {
                        if let Ok(code) = entity[1..].parse::<u32>()
                            && let Some(c) = char::from_u32(code)
                        {
                            out.push(c);
                        } else {
                            out.push('&');
                            out.push_str(&entity);
                            out.push(';');
                        }
                    }
                    _ => {
                        out.push('&');
                        out.push_str(&entity);
                        out.push(';');
                    }
                }
            } else {
                out.push('&');
                out.push_str(&entity);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Извлекает и очищает название веб-страницы из HTML (<title>...</title>).
pub fn extract_html_title(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let title_tag = "<title";
    let start_idx = lower.find(title_tag)?;

    // Находим конец открывающего тега <title ...>
    let rel_open_end = body[start_idx..].find('>')?;
    let tag_open_end = start_idx + rel_open_end + 1;
    let after_tag = &body[tag_open_end..];
    let after_tag_lower = &lower[tag_open_end..];

    // Ищем закрывающий тег </title>
    let title_content = if let Some(end_idx) = after_tag_lower.find("</title>") {
        &after_tag[..end_idx]
    } else {
        // Если тег не закрыт (например, усеченный буфер), берем до следующего тега '<'
        if let Some(next_tag) = after_tag.find('<') {
            &after_tag[..next_tag]
        } else {
            let max_take = after_tag.len().min(120);
            &after_tag[..max_take]
        }
    };

    // Декодируем HTML-сущности (&amp;, &#8212;, &quot; и т.д.)
    let decoded = decode_html_entities(title_content);

    // Нормализуем пробельные и управляющие символы в один пробел
    let mut normalized = String::with_capacity(decoded.len());
    let mut in_whitespace = false;
    for ch in decoded.chars() {
        if ch.is_control() || ch == '\x7F' || ch.is_whitespace() {
            if !in_whitespace {
                normalized.push(' ');
                in_whitespace = true;
            }
        } else {
            normalized.push(ch);
            in_whitespace = false;
        }
    }

    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Ограничиваем длину заголовка до 80 символов с многоточием при превышении
    let char_count = trimmed.chars().count();
    let safe_title = if char_count > 80 {
        let mut truncated: String = trimmed.chars().take(77).collect();
        truncated.push_str("...");
        truncated
    } else {
        trimmed.to_string()
    };

    Some(safe_title)
}

/// Анализирует сырые байты HTTP-ответа и формирует информативный баннер со строкой статуса,
/// опциональным сервером и извлечённым HTML-заголовком [Title: ...].
pub fn parse_http_banner(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    let lossy = String::from_utf8_lossy(bytes);
    let is_http = lossy.starts_with("HTTP/") || lossy.starts_with("http/");
    let html_title = extract_html_title(&lossy);

    if is_http {
        // Извлекаем строку статуса (первая строка до \r или \n)
        let status_line = lossy.lines().next().unwrap_or("").trim();
        let status = if status_line.len() > 40 {
            status_line.chars().take(40).collect::<String>()
        } else {
            status_line.to_string()
        };

        // Извлекаем заголовок Server, если присутствует
        let mut server_val = None;
        for line in lossy.lines().skip(1) {
            let line_trimmed = line.trim();
            if line_trimmed.is_empty() {
                break;
            }
            if let Some(rest) = line_trimmed.strip_prefix("Server:") {
                let s = rest.trim();
                if !s.is_empty() {
                    server_val = Some(s.chars().take(30).collect::<String>());
                    break;
                }
            } else if let Some(rest) = line_trimmed.strip_prefix("server:") {
                let s = rest.trim();
                if !s.is_empty() {
                    server_val = Some(s.chars().take(30).collect::<String>());
                    break;
                }
            }
        }

        // Если заголовок найден, собираем компактный информативный баннер
        if let Some(title) = html_title {
            let mut banner = status;
            if let Some(srv) = server_val {
                banner.push_str(" Server: ");
                banner.push_str(&srv);
            }
            banner.push_str(" [Title: ");
            banner.push_str(&title);
            banner.push(']');
            let final_banner: String = banner.chars().take(160).collect();
            return Some(final_banner);
        }

        // Если HTML title нет, но есть Server:
        if let Some(srv) = server_val {
            let mut banner = status;
            banner.push_str(" Server: ");
            banner.push_str(&srv);
            let final_banner: String = banner.chars().take(160).collect();
            return Some(final_banner);
        }

        // Fallback: стандартная очистка заголовков
        sanitize_banner(bytes)
    } else if let Some(title) = html_title {
        // Сырой HTML ответ без стандартных HTTP-заголовков
        let banner = format!("HTML [Title: {title}]");
        let final_banner: String = banner.chars().take(160).collect();
        Some(final_banner)
    } else {
        None
    }
}

/// Определяет подтверждённый сервис по содержимому баннера.
pub fn infer_service(banner: &str) -> Option<&'static str> {
    let lower = banner.to_ascii_lowercase();
    if banner.starts_with("SSH-") {
        Some("ssh")
    } else if banner.starts_with("HTTP/")
        || banner.starts_with("HTML [")
        || lower.contains("server: ")
        || lower.contains("<html")
        || lower.contains("[title:")
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
#[allow(dead_code)]
pub async fn grab_banner(
    stream: &mut TcpStream,
    timeout: Duration,
    max_bytes: usize,
) -> Option<String> {
    grab_banner_with_target(stream, timeout, max_bytes, None).await
}

/// Читает приветственный баннер из потока или посылает HTTP/проверочный запрос с указанием целевого IP.
pub async fn grab_banner_with_target(
    stream: &mut TcpStream,
    timeout: Duration,
    max_bytes: usize,
    target_ip: Option<IpAddr>,
) -> Option<String> {
    let mut buf = vec![0u8; max_bytes.clamp(256, 8192)];

    // 1. Ожидаем приветственное сообщение от сервисов с немедленным баннером (SSH, FTP, SMTP, VNC).
    let greeting_wait = Duration::from_millis(250).min(timeout);
    let initial_read = tokio::time::timeout(greeting_wait, stream.read(&mut buf)).await;

    let bytes_read = match initial_read {
        Ok(Ok(n)) if n > 0 => n,
        _ => {
            // 2. Если приветствия не поступило, отправляем HTTP probe для request-response сервисов.
            let remaining = timeout.saturating_sub(greeting_wait);
            if remaining.is_zero() {
                return None;
            }
            let host_header = match target_ip {
                Some(IpAddr::V4(v4)) => format!("{v4}"),
                Some(IpAddr::V6(v6)) => format!("[{v6}]"),
                None => "localhost".to_string(),
            };
            let req = format!(
                "GET / HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: IPPortScanner\r\nAccept: text/html,*/*\r\nConnection: close\r\n\r\n"
            );
            if stream.write_all(req.as_bytes()).await.is_err() {
                return None;
            }

            match tokio::time::timeout(remaining, stream.read(&mut buf)).await {
                Ok(Ok(n)) if n > 0 => {
                    let mut total = n;
                    // Если это HTTP ответ, но тег <title> ещё не встретился в первом фрагменте,
                    // пробуем дочитать дополнительную порцию данных (до размера буфера), если есть оставшееся время.
                    let lossy = String::from_utf8_lossy(&buf[..total]);
                    if lossy.starts_with("HTTP/")
                        && !lossy.to_ascii_lowercase().contains("<title")
                        && total < buf.len()
                    {
                        let quick_wait = Duration::from_millis(150).min(remaining);
                        if let Ok(Ok(more)) =
                            tokio::time::timeout(quick_wait, stream.read(&mut buf[total..])).await
                        {
                            total += more;
                        }
                    }
                    total
                }
                _ => return None,
            }
        }
    };

    if let Some(http_banner) = parse_http_banner(&buf[..bytes_read]) {
        Some(http_banner)
    } else {
        sanitize_banner(&buf[..bytes_read])
    }
}

/// Пробует установить TCP-соединение и классифицирует исход попытки.
/// При передаче `banner_config` выполняет опциональный захват баннера.
/// При передаче `bind_addr` связывает исходящий сокет с указанным локальным IP.
pub async fn probe(
    ip: IpAddr,
    port: u16,
    timeout: Duration,
    banner_config: Option<BannerConfig>,
    bind_addr: Option<IpAddr>,
) -> ProbeOutcome {
    if let Some(src_ip) = bind_addr
        && src_ip.is_ipv4() != ip.is_ipv4()
    {
        return ProbeOutcome::Unreachable;
    }

    let started = Instant::now();
    let connect_fut = async {
        if let Some(src_ip) = bind_addr {
            let socket = if ip.is_ipv4() {
                tokio::net::TcpSocket::new_v4()?
            } else {
                tokio::net::TcpSocket::new_v6()?
            };
            socket.bind(std::net::SocketAddr::new(src_ip, 0))?;
            socket.connect(std::net::SocketAddr::new(ip, port)).await
        } else {
            TcpStream::connect((ip, port)).await
        }
    };

    match tokio::time::timeout(timeout, connect_fut).await {
        Ok(Ok(mut stream)) => {
            let rtt = started.elapsed();
            if let Some(config) = banner_config {
                let banner = grab_banner_with_target(
                    &mut stream,
                    config.timeout,
                    config.max_bytes,
                    Some(ip),
                )
                .await;
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
    probe(ip, port, timeout, None, None).await
}

/// Выполняет UDP пробу и преобразует результат в ProbeOutcome.
pub async fn probe_udp(
    ip: IpAddr,
    port: u16,
    timeout: Duration,
    bind_addr: Option<IpAddr>,
) -> ProbeOutcome {
    match super::udp::probe_udp(ip, port, timeout, bind_addr).await {
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
pub fn service_name(port: u16) -> String {
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
                None,
                None,
            )
            .await,
            ProbeOutcome::Open { .. }
        ));
    }

    #[tokio::test]
    async fn detects_open_port_with_bind_addr() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(matches!(
            probe(
                IpAddr::from([127, 0, 0, 1]),
                port,
                Duration::from_secs(1),
                None,
                Some(IpAddr::from([127, 0, 0, 1])),
            )
            .await,
            ProbeOutcome::Open { .. }
        ));

        let mismatch = probe(
            IpAddr::from([127, 0, 0, 1]),
            port,
            Duration::from_secs(1),
            None,
            Some("::1".parse().unwrap()),
        )
        .await;
        assert_eq!(mismatch, ProbeOutcome::Unreachable);
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
            probe(
                "::1".parse().unwrap(),
                port,
                Duration::from_secs(1),
                None,
                None
            )
            .await,
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
            None,
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
            None,
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
            None,
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

    #[test]
    fn decodes_html_entities_correctly() {
        assert_eq!(
            decode_html_entities(
                "AT&amp;T &quot;Router&quot; &lt;v1&gt; &#8212; &apos;Admin&apos; &copy; &#x2014;"
            ),
            "AT&T \"Router\" <v1> — 'Admin' © —"
        );
        assert_eq!(
            decode_html_entities("Plain text without entities"),
            "Plain text without entities"
        );
        assert_eq!(
            decode_html_entities("&copy; 2026 Company &nbsp; Inc."),
            "© 2026 Company   Inc."
        );
    }

    #[test]
    fn extracts_html_title_various_formats() {
        // Стандартный тег
        assert_eq!(
            extract_html_title("<html><head><title>My Home Page</title></head></html>"),
            Some("My Home Page".to_string())
        );
        // С атрибутами и верхним регистром
        assert_eq!(
            extract_html_title(
                "<HTML><HEAD><TITLE lang=\"en\" id=\"t1\">  Dashboard &amp; Stats  </TITLE></HEAD></HTML>"
            ),
            Some("Dashboard & Stats".to_string())
        );
        // Многострочный заголовок с лишними пробелами
        let multiline = "
            <html>
                <head>
                    <title>
                        Internal
                        Gateway
                        Console
                    </title>
                </head>
            </html>
        ";
        assert_eq!(
            extract_html_title(multiline),
            Some("Internal Gateway Console".to_string())
        );
        // Усеченный HTML без закрывающего тега
        assert_eq!(
            extract_html_title("<html><head><title>Incomplete Page Header<body"),
            Some("Incomplete Page Header".to_string())
        );
        // Пустой заголовок
        assert_eq!(
            extract_html_title("<html><head><title>   </title></head></html>"),
            None
        );
        // Отсутствующий заголовок
        assert_eq!(
            extract_html_title("<html><body><h1>Hello World</h1></body></html>"),
            None
        );
        // Очень длинный заголовок (должен безопасно обрезаться до 80 символов)
        let long_title = format!("<title>{}</title>", "A".repeat(100));
        let extracted = extract_html_title(&long_title).unwrap();
        assert_eq!(extracted.chars().count(), 80);
        assert!(extracted.ends_with("..."));
    }

    #[test]
    fn parses_http_banner_with_status_server_and_title() {
        let raw = b"HTTP/1.1 200 OK\r\nServer: nginx/1.24.0\r\nContent-Type: text/html\r\n\r\n<html><head><title>Network Portal</title></head><body><h1>Welcome</h1></body></html>";
        let banner = parse_http_banner(raw).unwrap();
        assert_eq!(
            banner,
            "HTTP/1.1 200 OK Server: nginx/1.24.0 [Title: Network Portal]"
        );
        assert_eq!(infer_service(&banner), Some("http"));

        // Без заголовка Server, но с Title
        let raw_no_srv = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><head><title>Admin Dashboard</title></head></html>";
        let banner_no_srv = parse_http_banner(raw_no_srv).unwrap();
        assert_eq!(banner_no_srv, "HTTP/1.1 200 OK [Title: Admin Dashboard]");
        assert_eq!(infer_service(&banner_no_srv), Some("http"));

        // 400 Bad Request на HTTPS порту
        let raw_https_err = b"HTTP/1.1 400 Bad Request\r\nServer: nginx\r\n\r\n<html><head><title>400 The plain HTTP request was sent to HTTPS port</title></head></html>";
        let banner_https = parse_http_banner(raw_https_err).unwrap();
        assert!(banner_https.contains("400 Bad Request"));
        assert!(
            banner_https.contains("[Title: 400 The plain HTTP request was sent to HTTPS port]")
        );
        assert_eq!(infer_service(&banner_https), Some("http"));

        // Сырой HTML без HTTP-заголовков
        let raw_html = b"<html><head><title>Embedded Device Setup</title></head></html>";
        let banner_html = parse_http_banner(raw_html).unwrap();
        assert_eq!(banner_html, "HTML [Title: Embedded Device Setup]");
        assert_eq!(infer_service(&banner_html), Some("http"));
    }

    #[tokio::test]
    async fn grabs_http_title_during_live_probe() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 512];
                let _ = socket.read(&mut buf).await;
                let response = b"HTTP/1.1 200 OK\r\nServer: MiniWeb/1.0\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<!DOCTYPE html><html><head><title>Antigravity Live Portal</title></head><body><h1>OK</h1></body></html>";
                let _ = socket.write_all(response).await;
            }
        });

        let outcome = probe(
            IpAddr::from([127, 0, 0, 1]),
            port,
            Duration::from_secs(1),
            Some(BannerConfig {
                timeout: Duration::from_millis(500),
                max_bytes: 2048,
            }),
            None,
        )
        .await;

        match outcome {
            ProbeOutcome::Open {
                banner,
                service_verified,
                confirmed_service,
                ..
            } => {
                let b = banner.expect("Banner should be captured");
                assert!(b.contains("HTTP/1.1 200 OK"));
                assert!(b.contains("Server: MiniWeb/1.0"));
                assert!(b.contains("[Title: Antigravity Live Portal]"));
                assert!(service_verified);
                assert_eq!(confirmed_service, Some("http"));
            }
            _ => panic!("Expected Open with HTTP title banner, got {outcome:?}"),
        }
    }
}
