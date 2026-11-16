use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

use ipnet::IpNet;

/// Максимальное количество адресов, которое может дать одна цель (CIDR/диапазон).
const MAX_TARGETS: usize = 65_536;

/// Разбирает многострочный список целей в плоский список IP-адресов.
///
/// Поддерживаются:
/// * одиночные адреса — `10.0.0.1`
/// * CIDR-подсети — `192.168.1.0/24`
/// * IPv4-диапазоны — `10.0.0.5-10.0.0.20`
/// * строки, начинающиеся с `#`, пропускаются
pub fn parse_targets(input: &str) -> Result<Vec<IpAddr>, String> {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();

    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for ip in expand(line)? {
            if seen.insert(ip) {
                targets.push(ip);
            }
        }
        if targets.len() > MAX_TARGETS {
            return Err(format!(
                "Too many targets: limit is {MAX_TARGETS} addresses per scan"
            ));
        }
    }

    if targets.is_empty() {
        return Err("No targets specified".to_owned());
    }
    Ok(targets)
}

fn expand(entry: &str) -> Result<Vec<IpAddr>, String> {
    if let Ok(ip) = entry.parse::<IpAddr>() {
        return Ok(vec![ip]);
    }

    if let Ok(net) = entry.parse::<IpNet>() {
        return expand_net(net);
    }

    if let Some((start, end)) = entry.split_once('-') {
        let start = start
            .trim()
            .parse::<Ipv4Addr>()
            .map_err(|_| format!("Invalid range start in '{entry}'"))?;
        let end = end
            .trim()
            .parse::<Ipv4Addr>()
            .map_err(|_| format!("Invalid range end in '{entry}'"))?;
        let (start, end) = (u32::from(start), u32::from(end));
        if end < start {
            return Err(format!("Range end is before start in '{entry}'"));
        }
        let count = (end - start) as usize + 1;
        if count > MAX_TARGETS {
            return Err(format!("Range '{entry}' is too large (max {MAX_TARGETS})"));
        }
        return Ok((start..=end)
            .map(|n| IpAddr::V4(Ipv4Addr::from(n)))
            .collect());
    }

    Err(format!("Cannot parse target '{entry}'"))
}

fn expand_net(net: IpNet) -> Result<Vec<IpAddr>, String> {
    match net {
        IpNet::V4(net) if net.prefix_len() < 16 => Err(format!(
            "IPv4 subnet '{net}' is too large (smallest allowed is /16)"
        )),
        IpNet::V6(net) if net.prefix_len() < 120 => Err(format!(
            "IPv6 subnet '{net}' is too large (smallest allowed is /120)"
        )),
        IpNet::V4(net) => Ok(net.hosts().map(IpAddr::V4).collect()),
        IpNet::V6(net) => Ok(net.hosts().map(IpAddr::V6).collect()),
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
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], "127.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn parses_ranges_and_dedups() {
        let targets = parse_targets("10.0.0.1-10.0.0.3\n10.0.0.2").unwrap();
        assert_eq!(targets.len(), 3);
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
}
