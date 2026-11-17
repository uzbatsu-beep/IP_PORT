use std::net::IpAddr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub ip: IpAddr,
    pub is_loopback: bool,
}

/// Перечисляет доступные локальные сетевые интерфейсы и их IP-адреса.
/// Сортирует список так, чтобы реальные не-loopback адреса IPv4 шли первыми.
pub fn list_interfaces() -> Vec<NetworkInterfaceInfo> {
    let mut result = Vec::new();
    if let Ok(interfaces) = get_if_addrs::get_if_addrs() {
        for iface in interfaces {
            let ip = iface.ip();
            let is_loopback = iface.is_loopback();
            result.push(NetworkInterfaceInfo {
                name: iface.name,
                ip,
                is_loopback,
            });
        }
    }

    result.sort_by(|a, b| {
        a.is_loopback
            .cmp(&b.is_loopback)
            .then_with(|| a.ip.is_ipv6().cmp(&b.ip.is_ipv6()))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.ip.cmp(&b.ip))
    });

    result
}

/// Форматирует человекочитаемое имя для интерфейса.
pub fn format_interface_label(info: &NetworkInterfaceInfo) -> String {
    let loopback_suffix = if info.is_loopback { " (loopback)" } else { "" };
    format!("{} — {}{}", info.name, info.ip, loopback_suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_interfaces_returns_at_least_loopback() {
        let ifaces = list_interfaces();
        assert!(
            !ifaces.is_empty(),
            "Should discover at least one network interface"
        );
        assert!(
            ifaces.iter().any(|i| i.ip.is_loopback()),
            "Should contain a loopback IP (127.0.0.1 or ::1)"
        );
    }

    #[test]
    fn format_interface_label_displays_name_and_ip() {
        let info = NetworkInterfaceInfo {
            name: "eth0".to_owned(),
            ip: "192.168.1.50".parse().unwrap(),
            is_loopback: false,
        };
        assert_eq!(format_interface_label(&info), "eth0 — 192.168.1.50");

        let loopback = NetworkInterfaceInfo {
            name: "lo".to_owned(),
            ip: "127.0.0.1".parse().unwrap(),
            is_loopback: true,
        };
        assert_eq!(
            format_interface_label(&loopback),
            "lo — 127.0.0.1 (loopback)"
        );
    }
}
