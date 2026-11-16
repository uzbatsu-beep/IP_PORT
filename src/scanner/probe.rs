use std::net::IpAddr;
use std::time::{Duration, Instant};

use tokio::net::TcpStream;

/// Пробует установить TCP-соединение. Возвращает задержку при успехе.
pub async fn probe(ip: IpAddr, port: u16, timeout: Duration) -> Option<Duration> {
    let started = Instant::now();
    match tokio::time::timeout(timeout, TcpStream::connect((ip, port))).await {
        Ok(Ok(_stream)) => Some(started.elapsed()),
        _ => None,
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
        let rtt = probe(IpAddr::from([127, 0, 0, 1]), port, Duration::from_secs(1)).await;
        assert!(rtt.is_some());
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
