use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use ipnet::IpNet;
use tokio_util::sync::CancellationToken;

/// Максимальное количество адресов, которое может дать одна цель (CIDR/диапазон) или всё сканирование.
pub const MAX_TARGETS: usize = 65_536;

/// Таймаут по умолчанию для разрешения каждого DNS-имени хоста.
pub const DEFAULT_DNS_TIMEOUT: Duration = Duration::from_secs(3);

/// Элемент разобранного списка целей: прямой IP-адрес или доменное имя.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TargetEntry {
    Ip(IpAddr),
    Hostname(String),
}

/// Разобранный список целей, готовый для отображения в UI и разрешения перед сканированием.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TargetList {
    pub ips: Vec<IpAddr>,
    pub hostnames: Vec<String>,
}

impl TargetList {
    #[allow(dead_code)]
    pub fn new(ips: Vec<IpAddr>, hostnames: Vec<String>) -> Self {
        Self { ips, hostnames }
    }

    pub fn from_ips(ips: Vec<IpAddr>) -> Self {
        Self {
            ips,
            hostnames: Vec::new(),
        }
    }

    pub fn estimated_count(&self) -> usize {
        self.ips.len().saturating_add(self.hostnames.len())
    }

    pub fn is_empty(&self) -> bool {
        self.ips.is_empty() && self.hostnames.is_empty()
    }
}

impl From<Vec<IpAddr>> for TargetList {
    fn from(ips: Vec<IpAddr>) -> Self {
        Self::from_ips(ips)
    }
}

/// Разбирает многострочный список целей в синтаксически валидированный `TargetList`.
///
/// Обработка выполняется исключительно синхронно и локально (без сетевых вызовов DNS),
/// что гарантирует отсутствие блокировок потока рендеринга GUI.
///
/// Поддерживаются:
/// * одиночные IP-адреса — `10.0.0.1`, `::1`, `2001:db8::1`
/// * CIDR-подсети — `192.168.1.0/24`, `2001:db8::/120`
/// * диапазоны IPv4 и IPv6 — `10.0.0.5-10.0.0.20`, `::1-::5`
/// * DNS-имена хостов — `localhost`, `example.com`, `sub.domain.local`, `scanme.nmap.org`
/// * строки, начинающиеся с `#`, пропускаются
pub fn parse_targets(input: &str) -> Result<TargetList, String> {
    let mut seen_ips = HashSet::new();
    let mut seen_hostnames = HashSet::new();
    let mut ips = Vec::new();
    let mut hostnames = Vec::new();

    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for entry in expand(line)? {
            match entry {
                TargetEntry::Ip(ip) => {
                    if seen_ips.insert(ip) {
                        ips.push(ip);
                    }
                }
                TargetEntry::Hostname(name) => {
                    let lower = name.to_ascii_lowercase();
                    if seen_hostnames.insert(lower.clone()) {
                        hostnames.push(lower);
                    }
                }
            }
        }
        if ips.len() + hostnames.len() > MAX_TARGETS {
            return Err(format!(
                "Too many targets: limit is {MAX_TARGETS} addresses per scan"
            ));
        }
    }

    if ips.is_empty() && hostnames.is_empty() {
        return Err("No targets specified".to_owned());
    }

    Ok(TargetList { ips, hostnames })
}

fn expand(entry: &str) -> Result<Vec<TargetEntry>, String> {
    // 1. Явная диагностика для IPv6 с идентификатором зоны/интерфейса (%eth0, %1 и т.д.)
    if entry.contains('%') {
        let (ip_part, _zone) = entry.split_once('%').unwrap();
        if ip_part.trim().parse::<Ipv6Addr>().is_ok() {
            return Err(format!(
                "IPv6 zone identifiers ('{entry}') are interface-specific and not supported. Use global unicast, ULA (fd00::/8) or loopback (::1)"
            ));
        }
    }

    // 2. Одиночный IP-адрес
    if let Ok(ip) = entry.parse::<IpAddr>() {
        return Ok(vec![TargetEntry::Ip(ip)]);
    }

    // 3. CIDR-нотация
    if let Ok(net) = entry.parse::<IpNet>() {
        return expand_net(net);
    }

    // 4. Попытка разобрать диапазон с дефисом
    if let Some((start_raw, end_raw)) = entry.split_once('-') {
        let start_str = start_raw.trim();
        let end_str = end_raw.trim();

        // 4a. Диапазон IPv4
        if let (Ok(start), Ok(end)) = (start_str.parse::<Ipv4Addr>(), end_str.parse::<Ipv4Addr>()) {
            let (start, end) = (u32::from(start), u32::from(end));
            if end < start {
                return Err(format!("Range end is before start in '{entry}'"));
            }
            let count = (end - start) as usize + 1;
            if count > MAX_TARGETS {
                return Err(format!("Range '{entry}' is too large (max {MAX_TARGETS})"));
            }
            return Ok((start..=end)
                .map(|n| TargetEntry::Ip(IpAddr::V4(Ipv4Addr::from(n))))
                .collect());
        }

        // 4b. Диапазон IPv6
        if let (Ok(start), Ok(end)) = (start_str.parse::<Ipv6Addr>(), end_str.parse::<Ipv6Addr>()) {
            let (start, end) = (u128::from(start), u128::from(end));
            if end < start {
                return Err(format!("IPv6 range end is before start in '{entry}'"));
            }
            let count = end - start + 1;
            if count > MAX_TARGETS as u128 {
                return Err(format!(
                    "IPv6 range '{entry}' is too large (max {MAX_TARGETS})"
                ));
            }
            return Ok((start..=end)
                .map(|n| TargetEntry::Ip(IpAddr::V6(Ipv6Addr::from(n))))
                .collect());
        }

        // Если хотя бы одна из сторон была распознана как IP, значит пользователь вводил диапазон адресов
        if start_str.parse::<IpAddr>().is_ok() || end_str.parse::<IpAddr>().is_ok() {
            return Err(format!("Invalid range syntax in '{entry}'"));
        }

        // В противном случае дефис может быть частью имени хоста (напр. 'web-server-01.corp')
    }

    // 5. Валидация DNS-имени хоста по RFC 1123 / RFC 952
    validate_hostname(entry)?;
    Ok(vec![TargetEntry::Hostname(entry.to_ascii_lowercase())])
}

fn expand_net(net: IpNet) -> Result<Vec<TargetEntry>, String> {
    match net {
        IpNet::V4(net) if net.prefix_len() < 16 => Err(format!(
            "IPv4 subnet '{net}' is too large (smallest allowed is /16)"
        )),
        IpNet::V6(net) if net.prefix_len() < 120 => Err(format!(
            "IPv6 subnet '{net}' is too large (smallest allowed is /120, max 256 hosts)"
        )),
        IpNet::V4(net) => Ok(net
            .hosts()
            .map(|ip| TargetEntry::Ip(IpAddr::V4(ip)))
            .collect()),
        IpNet::V6(net) => Ok(net
            .hosts()
            .map(|ip| TargetEntry::Ip(IpAddr::V6(ip)))
            .collect()),
    }
}

/// Синтаксическая валидация доменного имени или имени хоста (RFC 1123 / RFC 952).
pub fn validate_hostname(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Hostname cannot be empty".to_owned());
    }
    if name.len() > 253 {
        return Err(format!(
            "Hostname '{name}' is too long ({} characters, maximum is 253)",
            name.len()
        ));
    }
    if name.contains("://") {
        return Err(format!(
            "Protocol scheme in '{name}' is not allowed — specify only the hostname (e.g. 'example.com')"
        ));
    }
    if name.contains('/') {
        return Err(format!(
            "Slash in '{name}' is not allowed. CIDR notation ('/24') is only supported for IP addresses"
        ));
    }
    if name.contains(':') {
        return Err(format!(
            "Port in '{name}' is not allowed — specify ports in the Ports field"
        ));
    }
    if name.contains('%') {
        return Err(format!("Character '%' in '{name}' is not allowed"));
    }
    if name.chars().any(char::is_whitespace) {
        return Err(format!("Whitespace in '{name}' is not allowed"));
    }

    let labels: Vec<&str> = name.split('.').collect();
    for label in &labels {
        if label.is_empty() {
            return Err(format!(
                "Invalid hostname '{name}': consecutive dots or leading/trailing dot are not allowed"
            ));
        }
        if label.len() > 63 {
            return Err(format!(
                "Hostname label '{label}' in '{name}' is too long ({} characters, maximum is 63)",
                label.len()
            ));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(format!(
                "Hostname label '{label}' in '{name}' cannot start or end with a hyphen"
            ));
        }
        for ch in label.chars() {
            if !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '_') {
                return Err(format!(
                    "Invalid character '{ch}' in hostname '{name}'. Only letters, digits, hyphens and underscores are allowed"
                ));
            }
        }
    }

    // Проверка TLD и односложных имен на чисто числовой вид
    if labels.len() == 1 {
        if labels[0].chars().all(|c| c.is_ascii_digit()) {
            return Err(format!("Target '{name}' cannot be purely numeric"));
        }
    } else {
        let last_label = labels.last().unwrap();
        if last_label.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!(
                "Top-level domain '{last_label}' in '{name}' cannot be all-numeric (invalid IP address or TLD)"
            ));
        }
    }

    Ok(())
}

/// Асинхронно разрешает `TargetList` во множество уникальных IP-адресов с поддержкой отмены.
///
/// Прямые IP-адреса включаются без сетевых обращений. Для каждого доменного имени выполняется
/// асинхронный поиск A и AAAA записей (`lookup_host`) с таймаутом `timeout`. Все найденные
/// адреса дедуплицируются с сохранением порядка первого обнаружения.
pub async fn resolve_targets_with_cancel(
    targets: &TargetList,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Option<Vec<IpAddr>>, String> {
    if cancel.is_cancelled() {
        return Ok(None);
    }

    if targets.hostnames.is_empty() {
        return Ok(Some(targets.ips.clone()));
    }

    let mut seen: HashSet<IpAddr> = targets.ips.iter().copied().collect();
    let mut resolved = targets.ips.clone();

    // Асинхронно разрешаем имена хостов
    for hostname in &targets.hostnames {
        if cancel.is_cancelled() {
            return Ok(None);
        }

        let lookup = tokio::net::lookup_host((hostname.as_str(), 0));
        let addrs = match tokio::time::timeout(timeout, lookup).await {
            Ok(Ok(iter)) => iter,
            Ok(Err(err)) => {
                return Err(format!("Failed to resolve hostname '{hostname}': {err}"));
            }
            Err(_) => {
                return Err(format!(
                    "DNS resolution timed out for '{hostname}' after {} ms",
                    timeout.as_millis()
                ));
            }
        };

        let mut resolved_any = false;
        for socket_addr in addrs {
            resolved_any = true;
            let ip = socket_addr.ip();
            if seen.insert(ip) {
                resolved.push(ip);
                if resolved.len() > MAX_TARGETS {
                    return Err(format!(
                        "Too many targets after DNS resolution: limit is {MAX_TARGETS} addresses per scan"
                    ));
                }
            }
        }

        if !resolved_any {
            return Err(format!(
                "Hostname '{hostname}' did not resolve to any IP address"
            ));
        }
    }

    if resolved.is_empty() {
        return Err("No targets specified or resolved".to_owned());
    }

    Ok(Some(resolved))
}

/// Асинхронное разрешение списка целей без токена отмены.
#[allow(dead_code)]
pub async fn resolve_targets(
    targets: &TargetList,
    timeout: Duration,
) -> Result<Vec<IpAddr>, String> {
    let cancel = CancellationToken::new();
    match resolve_targets_with_cancel(targets, timeout, &cancel).await? {
        Some(ips) => Ok(ips),
        None => Err("Resolution was cancelled".to_owned()),
    }
}

/// Разбирает список портов: `22,80,443`, `1-1024`, `22 80 443`.
pub fn parse_ports(input: &str) -> Result<Vec<u16>, String> {
    let mut ports = Vec::new();

    for token in input.split(|c: char| c == ',' || c.is_whitespace()) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some((start, end)) = token.split_once('-') {
            let start = parse_port(start, token)?;
            let end = parse_port(end, token)?;
            if end < start {
                return Err(format!("Port range end is before start in '{token}'"));
            }
            ports.extend(start..=end);
        } else {
            ports.push(parse_port(token, token)?);
        }
    }

    ports.sort_unstable();
    ports.dedup();

    if ports.is_empty() {
        return Err("No ports specified".to_owned());
    }
    Ok(ports)
}

fn parse_port(value: &str, context: &str) -> Result<u16, String> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or_else(|| format!("Invalid port in '{context}' (expected 1-65535)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_and_cidr() {
        let targets = parse_targets("127.0.0.1\n192.168.1.0/30").unwrap();
        assert_eq!(targets.ips.len(), 3);
        assert_eq!(targets.ips[0], "127.0.0.1".parse::<IpAddr>().unwrap());
        assert!(targets.hostnames.is_empty());
    }

    #[test]
    fn parses_ranges_and_dedups() {
        let targets = parse_targets("10.0.0.1-10.0.0.3\n10.0.0.2").unwrap();
        assert_eq!(targets.ips.len(), 3);
        assert!(targets.hostnames.is_empty());
    }

    #[test]
    fn rejects_large_subnets() {
        assert!(parse_targets("10.0.0.0/8").is_err());
    }

    #[test]
    fn parses_ports() {
        assert_eq!(parse_ports("22, 80, 443").unwrap(), vec![22, 80, 443]);
        assert_eq!(parse_ports("1-3").unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_ports("80,80").unwrap(), vec![80]);
        assert!(parse_ports("0").is_err());
        assert!(parse_ports("10-5").is_err());
    }

    #[test]
    fn parses_ipv6_single_cidr_and_ranges() {
        // Single IPv6
        let targets = parse_targets("::1\n2001:db8::1").unwrap();
        assert_eq!(targets.ips.len(), 2);
        assert_eq!(targets.ips[0], "::1".parse::<IpAddr>().unwrap());
        assert_eq!(targets.ips[1], "2001:db8::1".parse::<IpAddr>().unwrap());

        // IPv6 CIDR
        let cidr = parse_targets("2001:db8::/126").unwrap();
        assert_eq!(cidr.ips.len(), 4);
        assert_eq!(cidr.ips[0], "2001:db8::".parse::<IpAddr>().unwrap());
        assert_eq!(cidr.ips[3], "2001:db8::3".parse::<IpAddr>().unwrap());

        // IPv6 range
        let range = parse_targets("::1-::5").unwrap();
        assert_eq!(range.ips.len(), 5);
        assert_eq!(range.ips[0], "::1".parse::<IpAddr>().unwrap());
        assert_eq!(range.ips[4], "::5".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn rejects_ipv6_zone_and_large_subnets() {
        let err = parse_targets("fe80::1%eth0").unwrap_err();
        assert!(err.contains("zone identifiers"));

        let err2 = parse_targets("fe80::1%1").unwrap_err();
        assert!(err2.contains("zone identifiers"));

        let err3 = parse_targets("2001:db8::/64").unwrap_err();
        assert!(err3.contains("smallest allowed is /120"));

        let err4 = parse_targets("::5-::1").unwrap_err();
        assert!(err4.contains("range end is before start"));
    }

    #[test]
    fn parses_valid_hostnames_and_normalizes_case() {
        let targets =
            parse_targets("LocalHost\nexample.COM\nweb-server-01.corp\nrouter.home.arpa").unwrap();
        assert!(targets.ips.is_empty());
        assert_eq!(targets.hostnames.len(), 4);
        assert_eq!(targets.hostnames[0], "localhost");
        assert_eq!(targets.hostnames[1], "example.com");
        assert_eq!(targets.hostnames[2], "web-server-01.corp");
        assert_eq!(targets.hostnames[3], "router.home.arpa");
    }

    #[test]
    fn deduplicates_hostnames_and_ips() {
        let targets = parse_targets("127.0.0.1\nlocalhost\n127.0.0.1\nLOCALHOST").unwrap();
        assert_eq!(targets.ips.len(), 1);
        assert_eq!(targets.hostnames.len(), 1);
        assert_eq!(targets.estimated_count(), 2);
    }

    #[test]
    fn validates_hostname_syntax_and_rejects_common_mistakes() {
        // Scheme in hostname
        let err_scheme = parse_targets("http://example.com").unwrap_err();
        assert!(err_scheme.contains("Protocol scheme"));

        // Slash in hostname
        let err_slash = parse_targets("example.com/24").unwrap_err();
        assert!(err_slash.contains("Slash in"));

        // Port in hostname
        let err_port = parse_targets("example.com:80").unwrap_err();
        assert!(err_port.contains("Port in"));

        // Invalid IP address masquerading as hostname
        let err_ip = parse_targets("192.168.1.300").unwrap_err();
        assert!(err_ip.contains("cannot be all-numeric"));

        // Purely numeric single label
        let err_num = parse_targets("12345").unwrap_err();
        assert!(err_num.contains("cannot be purely numeric"));

        // Label starts or ends with hyphen
        let err_hyphen_start = parse_targets("-bad.com").unwrap_err();
        assert!(err_hyphen_start.contains("cannot start or end with a hyphen"));

        let err_hyphen_end = parse_targets("bad-.com").unwrap_err();
        assert!(err_hyphen_end.contains("cannot start or end with a hyphen"));

        // Consecutive dots
        let err_dots = parse_targets("foo..bar").unwrap_err();
        assert!(err_dots.contains("consecutive dots"));

        // Whitespace
        let err_space = parse_targets("foo bar.com").unwrap_err();
        assert!(err_space.contains("Whitespace"));
    }

    #[tokio::test]
    async fn resolves_localhost_and_deduplicates_with_loopback_ip() {
        let targets = TargetList::new(
            vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            vec!["localhost".to_owned()],
        );
        let resolved = resolve_targets(&targets, Duration::from_secs(2))
            .await
            .unwrap();

        // 127.0.0.1 must be present and deduplicated
        assert!(resolved.contains(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
        // Count should not duplicate 127.0.0.1 (it may also contain ::1 on dual-stack OSes)
        let v4_count = resolved
            .iter()
            .filter(|ip| **ip == IpAddr::V4(Ipv4Addr::LOCALHOST))
            .count();
        assert_eq!(v4_count, 1);
    }

    #[tokio::test]
    async fn resolution_fails_for_nonexistent_domain() {
        let targets = TargetList::new(
            Vec::new(),
            vec!["this-domain-does-not-exist-12345.invalid".to_owned()],
        );
        let err = resolve_targets(&targets, Duration::from_secs(2))
            .await
            .unwrap_err();
        assert!(err.contains("this-domain-does-not-exist-12345.invalid"));
        assert!(err.contains("Failed to resolve") || err.contains("timed out"));
    }

    #[tokio::test]
    async fn resolution_respects_cancellation() {
        let targets = TargetList::new(Vec::new(), vec!["localhost".to_owned()]);
        let cancel = CancellationToken::new();
        cancel.cancel(); // Pre-cancel
        let result = resolve_targets_with_cancel(&targets, Duration::from_secs(2), &cancel)
            .await
            .unwrap();
        assert_eq!(result, None);
    }
}
