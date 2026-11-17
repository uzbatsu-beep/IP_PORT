use crate::scanner::ScanProtocol;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortPreset {
    pub name: &'static str,
    pub description: &'static str,
    pub ports: &'static str,
    pub count: usize,
    pub protocol: ScanProtocol,
}

pub const TCP_PRESETS: &[PortPreset] = &[
    PortPreset {
        name: "Common",
        description: "20 most frequent TCP ports (Web, SSH, Mail, Database, Remote)",
        ports: "21,22,23,25,53,80,110,111,135,139,143,443,445,993,995,1723,3306,3389,5900,8080",
        count: 20,
        protocol: ScanProtocol::Tcp,
    },
    PortPreset {
        name: "Top 100",
        description: "Nmap standard Top 100 most frequent TCP ports",
        ports: "7,9,13,21-23,25-26,37,53,79-81,88,106,110-111,113,119,135,139,143-144,179,199,389,427,443-445,465,513-515,543-544,548,554,587,631,646,873,990,993,995,1025-1029,1110,1433,1720,1723,1755,1900,2000-2001,2049,2121,2717,3000,3128,3306,3389,3986,4899,5000,5009,5051,5060,5101,5190,5357,5432,5631,5666,5800,5900,6000-6001,6646,7070,8000,8008-8009,8080-8081,8443,8888,9100,9999-10000,32768,49152-49157",
        count: 100,
        protocol: ScanProtocol::Tcp,
    },
    PortPreset {
        name: "Web",
        description: "Standard and alternate HTTP/HTTPS web application ports",
        ports: "80,443,3000,5000,8000,8008,8080,8081,8443,8888,9000,9090",
        count: 12,
        protocol: ScanProtocol::Tcp,
    },
    PortPreset {
        name: "Database",
        description: "Relational, NoSQL and in-memory databases (MySQL, Postgres, MSSQL, Oracle, Mongo, Redis)",
        ports: "1433,1521,3306,5432,6379,9042,11211,27017,28017",
        count: 9,
        protocol: ScanProtocol::Tcp,
    },
    PortPreset {
        name: "Remote",
        description: "Remote administration: SSH, Telnet, RDP, VNC, WinRM",
        ports: "22,23,3389,5900,5901,5985,5986",
        count: 7,
        protocol: ScanProtocol::Tcp,
    },
    PortPreset {
        name: "1-1024",
        description: "Privileged system ports (1 to 1024)",
        ports: "1-1024",
        count: 1024,
        protocol: ScanProtocol::Tcp,
    },
];

pub const UDP_PRESETS: &[PortPreset] = &[
    PortPreset {
        name: "Common UDP",
        description: "Essential UDP services: DNS, DHCP, TFTP, NTP, NetBIOS, SNMP, Syslog, OpenVPN, WireGuard",
        ports: "53,67,68,69,123,137,138,161,162,500,514,1194,1900,4500,51820",
        count: 15,
        protocol: ScanProtocol::Udp,
    },
    PortPreset {
        name: "Top 20 UDP",
        description: "20 most responsive UDP services",
        ports: "53,67,68,69,123,135,137,138,139,161,162,445,500,514,520,631,1434,1900,4500,5353",
        count: 20,
        protocol: ScanProtocol::Udp,
    },
    PortPreset {
        name: "VPN / Infra",
        description: "IPsec IKE, NAT-T, OpenVPN, WireGuard, L2TP",
        ports: "500,1194,1701,4500,51820",
        count: 5,
        protocol: ScanProtocol::Udp,
    },
    PortPreset {
        name: "Discovery",
        description: "SSDP (UPnP), mDNS, NetBIOS Name & Datagram, DHCP",
        ports: "67,68,137,138,1900,5353",
        count: 6,
        protocol: ScanProtocol::Udp,
    },
];

pub fn presets_for_protocol(protocol: ScanProtocol) -> &'static [PortPreset] {
    match protocol {
        ScanProtocol::Tcp => TCP_PRESETS,
        ScanProtocol::Udp => UDP_PRESETS,
    }
}

/// Проверяет, совпадает ли текущий ввод портов с пресетом (с учётом нормализации и сортировки).
pub fn ports_input_matches(input: &str, preset_ports: &str) -> bool {
    match (
        crate::scanner::targets::parse_ports(input),
        crate::scanner::targets::parse_ports(preset_ports),
    ) {
        (Ok(input_ports), Ok(preset)) => input_ports == preset,
        _ => false,
    }
}

/// Применяет выбранный пресет: либо полностью заменяет текущий ввод,
/// либо объединяет порты без дублирования (режим append).
pub fn apply_preset(current: &str, preset_ports: &str, append: bool) -> String {
    if !append || current.trim().is_empty() {
        return preset_ports.to_owned();
    }

    match (
        crate::scanner::targets::parse_ports(current),
        crate::scanner::targets::parse_ports(preset_ports),
    ) {
        (Ok(mut existing), Ok(added)) => {
            existing.extend(added);
            existing.sort_unstable();
            existing.dedup();
            format_ports(&existing)
        }
        _ => format!("{},{}", current.trim_end_matches(','), preset_ports),
    }
}

/// Форматирует отсортированный срез номеров портов в компактную запись с объединением непрерывных диапазонов.
pub fn format_ports(ports: &[u16]) -> String {
    if ports.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let mut range_start = ports[0];
    let mut prev = ports[0];

    for &port in &ports[1..] {
        if port == prev + 1 {
            prev = port;
        } else {
            append_range(&mut out, range_start, prev);
            range_start = port;
            prev = port;
        }
    }
    append_range(&mut out, range_start, prev);
    out
}

fn append_range(out: &mut String, start: u16, end: u16) {
    if !out.is_empty() {
        out.push(',');
    }
    if start == end {
        use std::fmt::Write;
        let _ = write!(out, "{start}");
    } else if end == start + 1 {
        use std::fmt::Write;
        let _ = write!(out, "{start},{end}");
    } else {
        use std::fmt::Write;
        let _ = write!(out, "{start}-{end}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_parse_validly_and_match_declared_counts() {
        for preset in TCP_PRESETS.iter().chain(UDP_PRESETS.iter()) {
            let parsed = crate::scanner::targets::parse_ports(preset.ports)
                .unwrap_or_else(|e| panic!("Preset '{}' failed to parse: {e}", preset.name));
            assert_eq!(
                parsed.len(),
                preset.count,
                "Preset '{}' port count mismatch: expected {}, got {}",
                preset.name,
                preset.count,
                parsed.len()
            );
            assert!(
                ports_input_matches(preset.ports, preset.ports),
                "Preset '{}' should match its own string",
                preset.name
            );
        }
    }

    #[test]
    fn format_ports_compacts_single_and_ranges() {
        assert_eq!(format_ports(&[]), "");
        assert_eq!(format_ports(&[80]), "80");
        assert_eq!(format_ports(&[80, 81]), "80,81");
        assert_eq!(format_ports(&[80, 81, 82]), "80-82");
        assert_eq!(
            format_ports(&[22, 80, 81, 82, 443, 8080, 8081, 8082, 8083]),
            "22,80-82,443,8080-8083"
        );
    }

    #[test]
    fn apply_preset_replaces_or_merges_without_duplicates() {
        // Replace
        assert_eq!(apply_preset("22,80", "443,8443", false), "443,8443");
        assert_eq!(apply_preset("", "443,8443", true), "443,8443");

        // Merge (append)
        let merged = apply_preset("80,443", "22,80,3389", true);
        assert_eq!(merged, "22,80,443,3389");

        // Merge with range compaction
        let merged_range = apply_preset("80,81", "82,83,84", true);
        assert_eq!(merged_range, "80-84");
    }
}
