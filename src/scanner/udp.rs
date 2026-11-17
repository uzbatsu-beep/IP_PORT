use std::io::ErrorKind;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;

/// Протокол сканирования.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ScanProtocol {
    #[default]
    Tcp,
    Udp,
}

impl ScanProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
        }
    }
}

/// Состояние обнаруженного порта.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum PortState {
    #[default]
    Open,
    OpenFiltered,
}

impl PortState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::OpenFiltered => "Open|Filtered",
        }
    }
}

/// Исход UDP-пробы.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UdpProbeOutcome {
    /// Получен подтверждённый ответ датаграммы от сервиса.
    Open {
        rtt: Duration,
        response_len: usize,
        service: Option<&'static str>,
    },
    /// Ответа нет за время таймаута: порт либо открыт (служба молчит), либо отфильтрован фаерволом.
    OpenFiltered,
    /// Получен ICMP Port Unreachable (WSAECONNRESET на Windows / ECONNREFUSED на Linux/macOS).
    Closed,
    /// Недоступность сети или хоста (ICMP Network/Host Unreachable).
    Unreachable,
    /// Прочая системная ошибка ввода-вывода.
    Error,
}

/// Генерирует зондирующую датаграмму для конкретного UDP-порта.
pub fn payload_for_port(port: u16) -> &'static [u8] {
    match port {
        // DNS: Стандартный запрос Transaction ID=0x1032, Flags=0x0100 (Query), Questions=1, QD: root '.' Type A
        53 => &[
            0x10, 0x32, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x01,
        ],
        // NTP: Запрос клиента NTP v4, режим 3 (Client), 48 байт (0x23 и 47 нулей)
        123 => &[
            0x23, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
        // SNMP: GetRequest v2c, community "public", OID sysDescr 1.3.6.1.2.1.1.1.0
        161 => &[
            0x30, 0x29, 0x02, 0x01, 0x01, 0x04, 0x06, 0x70, 0x75, 0x62, 0x6c, 0x69, 0x63, 0xa0,
            0x1c, 0x02, 0x04, 0x00, 0x00, 0x00, 0x01, 0x02, 0x01, 0x00, 0x02, 0x01, 0x00, 0x30,
            0x0e, 0x30, 0x0c, 0x06, 0x08, 0x2b, 0x06, 0x01, 0x02, 0x01, 0x01, 0x01, 0x00, 0x05,
            0x00,
        ],
        // NetBIOS Name Service (137): Status Query '*'
        137 => &[
            0x80, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x43,
            0x4b, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
            0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41, 0x41,
            0x41, 0x41, 0x41, 0x00, 0x00, 0x21, 0x00, 0x01,
        ],
        // По умолчанию: 1-байтовый проверочный зонд
        _ => b"\0",
    }
}

pub fn classify_udp_error(error: &std::io::Error) -> UdpProbeOutcome {
    if error.kind() == ErrorKind::ConnectionRefused || error.kind() == ErrorKind::ConnectionReset {
        return UdpProbeOutcome::Closed;
    }

    if error.kind() == ErrorKind::NetworkUnreachable || error.kind() == ErrorKind::HostUnreachable {
        return UdpProbeOutcome::Unreachable;
    }

    #[cfg(target_os = "windows")]
    if let Some(code) = error.raw_os_error() {
        // WSAECONNRESET (10054) - ICMP Port Unreachable
        if code == 10054 {
            return UdpProbeOutcome::Closed;
        }
        // WSAEAFNOSUPPORT (10047), WSAENETUNREACH (10051), WSAEHOSTUNREACH (10065)
        if code == 10047 || code == 10051 || code == 10065 {
            return UdpProbeOutcome::Unreachable;
        }
    }

    #[cfg(not(target_os = "windows"))]
    if let Some(code) = error.raw_os_error() {
        // ECONNREFUSED (111 / 61) - ICMP Port Unreachable
        if code == 111 || code == 61 {
            return UdpProbeOutcome::Closed;
        }
        // EAFNOSUPPORT (97), ENETUNREACH (101), EHOSTUNREACH (113)
        if code == 97 || code == 101 || code == 113 {
            return UdpProbeOutcome::Unreachable;
        }
    }

    UdpProbeOutcome::Error
}

/// Выполняет UDP-пробу на указанный адрес и порт.
/// При передаче `bind_addr` связывает исходящий сокет с указанным локальным IP.
pub async fn probe_udp(
    ip: IpAddr,
    port: u16,
    timeout: Duration,
    bind_addr: Option<IpAddr>,
) -> UdpProbeOutcome {
    let local_bind = match bind_addr {
        Some(src_ip) => {
            if src_ip.is_ipv4() == ip.is_ipv4() {
                std::net::SocketAddr::new(src_ip, 0)
            } else {
                return UdpProbeOutcome::Unreachable;
            }
        }
        None => {
            if ip.is_ipv6() {
                std::net::SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], 0))
            } else {
                std::net::SocketAddr::from(([0, 0, 0, 0], 0))
            }
        }
    };

    let socket = match UdpSocket::bind(local_bind).await {
        Ok(s) => s,
        Err(err) => return classify_udp_error(&err),
    };

    // Привязываем сокет к удалённой паре (connect позволяет ловить асинхронные ICMP-ответы через ОС)
    if let Err(err) = socket.connect((ip, port)).await {
        return classify_udp_error(&err);
    }

    let payload = payload_for_port(port);
    let started = Instant::now();

    if let Err(err) = socket.send(payload).await {
        return classify_udp_error(&err);
    }

    let mut buf = [0u8; 512];
    match tokio::time::timeout(timeout, socket.recv(&mut buf)).await {
        Ok(Ok(len)) => {
            let rtt = started.elapsed();
            let service = match port {
                53 => Some("domain"),
                123 => Some("ntp"),
                161 => Some("snmp"),
                137 => Some("netbios-ns"),
                _ => None,
            };
            UdpProbeOutcome::Open {
                rtt,
                response_len: len,
                service,
            }
        }
        Ok(Err(err)) => classify_udp_error(&err),
        Err(_) => {
            // Истечение таймаута без ответа: в соответствии с RFC 768/1812 это Open|Filtered
            UdpProbeOutcome::OpenFiltered
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provides_payloads_for_standard_udp_services() {
        assert_eq!(payload_for_port(53).len(), 17);
        assert_eq!(payload_for_port(123).len(), 48);
        assert_eq!(payload_for_port(161).len(), 43);
        assert_eq!(payload_for_port(137).len(), 50);
        assert_eq!(payload_for_port(9999).len(), 1);
    }

    #[tokio::test]
    async fn detects_open_udp_service_when_response_is_received() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();

        tokio::spawn(async move {
            let mut buf = [0u8; 128];
            while let Ok((len, peer)) = server.recv_from(&mut buf).await {
                if server.send_to(&buf[..len], peer).await.is_err() {
                    break;
                }
            }
        });

        let outcome = probe_udp(
            IpAddr::from([127, 0, 0, 1]),
            port,
            Duration::from_millis(500),
            None,
        )
        .await;

        match outcome {
            UdpProbeOutcome::Open { response_len, .. } => {
                assert_eq!(response_len, 1);
            }
            _ => panic!("Expected Open UDP outcome, got {outcome:?}"),
        }

        // Test with explicit loopback bind
        let outcome_bound = probe_udp(
            IpAddr::from([127, 0, 0, 1]),
            port,
            Duration::from_millis(500),
            Some(IpAddr::from([127, 0, 0, 1])),
        )
        .await;
        assert!(matches!(outcome_bound, UdpProbeOutcome::Open { .. }));

        // Test with IP family mismatch
        let mismatch = probe_udp(
            IpAddr::from([127, 0, 0, 1]),
            port,
            Duration::from_millis(500),
            Some("::1".parse().unwrap()),
        )
        .await;
        assert_eq!(mismatch, UdpProbeOutcome::Unreachable);
    }

    #[tokio::test]
    async fn classifies_silent_udp_port_as_open_filtered() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();

        // Server listens but never sends anything back
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            drop(server);
        });

        let outcome = probe_udp(
            IpAddr::from([127, 0, 0, 1]),
            port,
            Duration::from_millis(60),
            None,
        )
        .await;

        assert_eq!(outcome, UdpProbeOutcome::OpenFiltered);
    }

    #[test]
    fn classifies_udp_errors() {
        assert_eq!(
            classify_udp_error(&std::io::Error::from(ErrorKind::ConnectionRefused)),
            UdpProbeOutcome::Closed
        );
        assert_eq!(
            classify_udp_error(&std::io::Error::from(ErrorKind::ConnectionReset)),
            UdpProbeOutcome::Closed
        );
        assert_eq!(
            classify_udp_error(&std::io::Error::from(ErrorKind::NetworkUnreachable)),
            UdpProbeOutcome::Unreachable
        );
        assert_eq!(
            classify_udp_error(&std::io::Error::from(ErrorKind::HostUnreachable)),
            UdpProbeOutcome::Unreachable
        );
        assert_eq!(
            classify_udp_error(&std::io::Error::from(ErrorKind::PermissionDenied)),
            UdpProbeOutcome::Error
        );
    }
}
