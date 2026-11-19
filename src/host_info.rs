//! Best-effort, read-only host enrichment. Never transmits ARP/NDP probes.
//! PTR uses the platform resolver via dns-lookup; neighbors are OS cache snapshots.
//! No host-identification I/O runs on egui's rendering thread.
use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostInfo {
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub oui: Option<String>,
    pub vendor: Option<String>,
}

pub struct HostInfoWorker {
    sender: mpsc::SyncSender<IpAddr>,
    updates: mpsc::Receiver<(IpAddr, HostInfo)>,
}

impl HostInfoWorker {
    pub fn start(resolve_ptr: bool, neighbor_cache: bool) -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel(512);
        let requests = Arc::new(Mutex::new(requests));
        let (updates_sender, updates) = mpsc::channel();
        for worker_idx in 0..2 {
            let requests = Arc::clone(&requests);
            let updates_sender = updates_sender.clone();
            std::thread::Builder::new()
                .name(format!("host-enrichment-{worker_idx}"))
                .spawn(move || {
                    let mut neighbors = HashMap::new();
                    let mut last_refresh: Option<Instant> = None;
                    loop {
                        let ip = {
                            let Ok(queue) = requests.lock() else { break };
                            queue.recv()
                        };
                        let Ok(ip) = ip else { break };
                        let hostname = if resolve_ptr {
                            dns_lookup::lookup_addr(&ip)
                                .ok()
                                .filter(|name| !name.is_empty() && name != &ip.to_string())
                        } else {
                            None
                        };
                        if neighbor_cache
                            && last_refresh
                                .is_none_or(|last| last.elapsed() >= Duration::from_secs(3))
                        {
                            neighbors = read_neighbor_cache();
                            last_refresh = Some(Instant::now());
                        }
                        let mac = if neighbor_cache {
                            neighbors.get(&ip).cloned()
                        } else {
                            None
                        };
                        let oui = mac.as_deref().and_then(oui_prefix);
                        let vendor = mac.as_deref().and_then(oui_vendor).map(str::to_owned);
                        if updates_sender
                            .send((
                                ip,
                                HostInfo {
                                    hostname,
                                    mac,
                                    oui,
                                    vendor,
                                },
                            ))
                            .is_err()
                        {
                            break;
                        }
                    }
                })?;
        }
        Ok(Self { sender, updates })
    }

    pub fn submit(&self, ip: IpAddr) -> bool {
        self.sender.try_send(ip).is_ok()
    }

    pub fn try_recv(&self) -> Option<(IpAddr, HostInfo)> {
        self.updates.try_recv().ok()
    }
}

fn normalize_mac(raw: &str) -> Option<String> {
    let raw = raw.trim_matches(|c: char| !c.is_ascii_hexdigit() && c != ':' && c != '-');
    let chunks: Vec<&str> = raw.split([':', '-']).collect();
    if chunks.len() != 6 || chunks.iter().any(|p| p.len() != 2) {
        return None;
    }
    let bytes: Vec<u8> = chunks
        .iter()
        .map(|p| u8::from_str_radix(p, 16))
        .collect::<Result<_, _>>()
        .ok()?;
    if bytes.iter().all(|b| *b == 0) || bytes.iter().all(|b| *b == 255) || bytes[0] & 1 != 0 {
        return None;
    }
    Some(
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

pub fn oui_prefix(mac: &str) -> Option<String> {
    let mac = normalize_mac(mac)?;
    Some(mac[..8].to_owned())
}

pub fn oui_vendor(mac: &str) -> Option<&'static str> {
    let mac = normalize_mac(mac)?;
    let first: u8 = u8::from_str_radix(&mac[..2], 16).ok()?;
    if first & 2 != 0 {
        return None; // Randomized / locally administered addresses have no reliable IEEE OUI.
    }
    // Small, curated offline map; unknown prefixes remain unknown, never guessed.
    // IEEE registry: https://standards.ieee.org/products-programs/regauth/
    match &mac[..8] {
        "00:00:0C" | "00:1A:A1" => Some("Cisco Systems"),
        "00:1B:21" | "00:02:B3" => Some("Intel"),
        "00:03:93" | "00:17:F2" | "3C:22:FB" => Some("Apple"),
        "B8:27:EB" => Some("Raspberry Pi Foundation"),
        "DC:A6:32" | "E4:5F:01" | "D8:3A:DD" | "28:CD:C1" => Some("Raspberry Pi"),
        "4C:5E:0C" => Some("MikroTik"),
        "50:C7:BF" | "B0:BE:76" => Some("TP-Link"),
        "00:50:56" | "00:0C:29" => Some("VMware"),
        "08:00:27" => Some("Oracle VirtualBox"),
        "24:0A:C4" => Some("Espressif"),
        _ => None,
    }
}

pub fn parse_neighbor_cache(output: &str) -> HashMap<IpAddr, String> {
    let mut map = HashMap::new();
    for line in output.lines() {
        let mut ip = None;
        let mut mac = None;
        for token in line.split_whitespace() {
            let token = token.trim_matches(|c| c == '(' || c == ')' || c == '?' || c == ',');
            if ip.is_none() {
                // Ignore IPv6 interface zone IDs in NDP output.
                let bare = token.split('%').next().unwrap_or(token);
                ip = bare.parse::<IpAddr>().ok();
            }
            if mac.is_none() {
                mac = normalize_mac(token);
            }
        }
        if let (Some(ip), Some(mac)) = (ip, mac) {
            map.insert(ip, mac);
        }
    }
    map
}

fn read_neighbor_cache() -> HashMap<IpAddr, String> {
    let mut neighbors = HashMap::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/net/arp") {
            neighbors.extend(parse_neighbor_cache(&contents));
        }
        if let Ok(output) = Command::new("ip").args(["-6", "neigh", "show"]).output() {
            neighbors.extend(parse_neighbor_cache(&String::from_utf8_lossy(
                &output.stdout,
            )));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("arp").arg("-a").output() {
            neighbors.extend(parse_neighbor_cache(&String::from_utf8_lossy(
                &output.stdout,
            )));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("arp").arg("-an").output() {
            neighbors.extend(parse_neighbor_cache(&String::from_utf8_lossy(
                &output.stdout,
            )));
        }
        if let Ok(output) = Command::new("ndp").arg("-an").output() {
            neighbors.extend(parse_neighbor_cache(&String::from_utf8_lossy(
                &output.stdout,
            )));
        }
    }
    neighbors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_arp_windows_arp_and_ipv6_ndp() {
        let data = "IP address HW type Flags HW address Mask Device\n\
            192.168.1.2 0x1 0x2 b8:27:eb:12:34:56 * eth0\n\
            192.168.1.3 00-1b-21-aa-bb-cc dynamic\n\
            ? (192.168.1.4) at dc:a6:32:01:02:03 on en0 ifscope [ethernet]\n\
            fe80::2%en0 dev en0 lladdr 00:00:0c:aa:bb:cc REACHABLE";
        let table = parse_neighbor_cache(data);
        assert_eq!(
            table[&"192.168.1.2".parse::<IpAddr>().unwrap()],
            "B8:27:EB:12:34:56"
        );
        assert_eq!(
            table[&"192.168.1.3".parse::<IpAddr>().unwrap()],
            "00:1B:21:AA:BB:CC"
        );
        assert_eq!(
            table[&"fe80::2".parse::<IpAddr>().unwrap()],
            "00:00:0C:AA:BB:CC"
        );
        assert_eq!(
            oui_vendor(&table[&"192.168.1.2".parse::<IpAddr>().unwrap()]),
            Some("Raspberry Pi Foundation")
        );
    }

    #[test]
    fn invalid_multicast_and_randomized_mac_are_not_falsely_attributed() {
        assert!(oui_prefix("ff:ff:ff:ff:ff:ff").is_none());
        assert!(oui_prefix("this-is-not-a-mac").is_none());
        assert_eq!(oui_vendor("02:00:00:00:00:01"), None);
        assert_eq!(oui_vendor("AA:BB:CC:00:11:22"), None);
    }
}
