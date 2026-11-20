//! Best-effort, read-only host enrichment. Never transmits ARP/NDP probes.
//! PTR uses the platform resolver via dns-lookup; neighbors are OS cache snapshots.
//! No host-identification I/O runs on egui's rendering thread.
use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

/// The platform DNS resolver does not guarantee a network timeout. Report
/// measured duration, but never block the egui UI thread while resolving.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PtrStatus {
    #[default]
    Disabled,
    Resolved,
    NoName,
    Failed(String),
}

impl PtrStatus {
    pub fn summary(&self) -> &str {
        match self {
            Self::Disabled => "Disabled",
            Self::Resolved => "Resolved",
            Self::NoName => "No PTR name",
            Self::Failed(_) => "Resolver error",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostInfo {
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub oui: Option<String>,
    pub vendor: Option<String>,
    pub ptr_status: PtrStatus,
    pub ptr_duration_ms: Option<u64>,
    pub neighbor_error: Option<String>,
    pub neighbor_interfaces: Vec<String>,
    pub neighbor_ambiguous: bool,
}

/// Map platform results without claiming all DNS lookup failures mean NXDOMAIN.
fn classify_ptr(ip: IpAddr, result: io::Result<String>) -> (Option<String>, PtrStatus) {
    match result {
        Ok(name) if !name.trim().is_empty() && name.trim() != ip.to_string() => {
            (Some(name.trim().to_owned()), PtrStatus::Resolved)
        }
        Ok(_) => (None, PtrStatus::NoName),
        Err(err) if err.kind() == io::ErrorKind::NotFound => (None, PtrStatus::NoName),
        Err(err) => {
            let message: String = err.to_string().chars().take(240).collect();
            (None, PtrStatus::Failed(message))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NeighborRecord {
    pub mac: String,
    pub interface: String,
}
pub type NeighborCache = HashMap<IpAddr, Vec<NeighborRecord>>;
fn insert_neighbor(cache: &mut NeighborCache, ip: IpAddr, mac: String, interface: String) {
    let records = cache.entry(ip).or_default();
    if !records.iter().any(|r| r.mac == mac && r.interface == interface) {
        records.push(NeighborRecord { mac, interface });
    }
}
fn merge_neighbors(cache: &mut NeighborCache, additional: NeighborCache) {
    for (ip, entries) in additional {
        for entry in entries {
            insert_neighbor(cache, ip, entry.mac, entry.interface);
        }
    }
}
/// Never assign an arbitrary MAC when the IP is present on multiple adapters.
pub fn select_neighbor(cache: &NeighborCache, ip: &IpAddr)
    -> (Option<String>, Vec<String>, bool)
{
    let Some(rows) = cache.get(ip) else { return (None, Vec::new(), false) };
    let interfaces = rows.iter()
        .map(|entry| format!("{} — {}", entry.interface, entry.mac))
        .collect();
    if rows.len() != 1 {
        return (None, interfaces, true);
    }
    (Some(rows[0].mac.clone()), interfaces, false)
}

pub enum HostInfoEvent {
    Started(IpAddr),
    Finished(IpAddr, HostInfo),
}
pub struct HostInfoWorker {
    sender: mpsc::SyncSender<IpAddr>,
    updates: mpsc::Receiver<HostInfoEvent>,
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
                    let mut neighbors = NeighborCache::new();
                    let mut neighbor_error: Option<String> = None;
                    let mut last_refresh: Option<Instant> = None;
                    loop {
                        let ip = {
                            let Ok(queue) = requests.lock() else { break };
                            queue.recv()
                        };
                        let Ok(ip) = ip else { break };
                        if updates_sender.send(HostInfoEvent::Started(ip)).is_err() {
                            break;
                        }
                        let (hostname, ptr_status, ptr_duration_ms) = if resolve_ptr {
                            let started = Instant::now();
                            let (name, status) = classify_ptr(ip, dns_lookup::lookup_addr(&ip));
                            let elapsed =
                                started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                            (name, status, Some(elapsed))
                        } else {
                            (None, PtrStatus::Disabled, None)
                        };
                        if neighbor_cache
                            && last_refresh
                                .is_none_or(|last| last.elapsed() >= Duration::from_secs(3))
                        {
                            match read_neighbor_cache() {
                                Ok(found) => {
                                    neighbors = found;
                                    neighbor_error = None;
                                }
                                Err(err) => {
                                    neighbors.clear();
                                    neighbor_error = Some(err);
                                }
                            }
                            last_refresh = Some(Instant::now());
                        }
                        let (mac, neighbor_interfaces, neighbor_ambiguous) = if neighbor_cache {
                            select_neighbor(&neighbors, &ip)
                        } else {
                            (None, Vec::new(), false)
                        };
                        let oui = mac.as_deref().and_then(oui_prefix);
                        let vendor = mac.as_deref().and_then(oui_vendor).map(str::to_owned);
                        if updates_sender
                            .send(HostInfoEvent::Finished(
                                ip,
                                HostInfo {
                                    hostname,
                                    mac,
                                    oui,
                                    vendor,
                                    ptr_status,
                                    ptr_duration_ms,
                                    neighbor_error: neighbor_error.clone(),
                                    neighbor_interfaces,
                                    neighbor_ambiguous,
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

    pub fn submit(&self, ip: IpAddr) -> Result<(), mpsc::TrySendError<IpAddr>> {
        self.sender.try_send(ip)
    }

    pub fn try_recv(&self) -> Option<HostInfoEvent> {
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

#[cfg(any(not(target_os = "windows"), test))]
pub fn parse_neighbor_cache(output: &str) -> NeighborCache {
    let mut map = NeighborCache::new();
    for line in output.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let mut ip = None;
        let mut mac = None;
        let mut zone = None;
        for token in &tokens {
            let clean = token.trim_matches(|c| c == '(' || c == ')' || c == '?' || c == ',');
            if ip.is_none() {
                let (bare, interface) = clean.split_once('%')
                    .map_or((clean, None), |(address, scope)| (address, Some(scope)));
                if let Ok(address) = bare.parse::<IpAddr>() {
                    ip = Some(address);
                    zone = interface.map(str::to_owned);
                }
            }
            if mac.is_none() {
                mac = normalize_mac(clean);
            }
        }
        if let (Some(ip), Some(mac)) = (ip, mac) {
            let interface = tokens.windows(2).find_map(|pair| {
                (pair[0] == "dev" || pair[0] == "on").then(|| pair[1].to_owned())
            }).or_else(|| {
                (tokens.len() >= 6 && tokens[1].starts_with("0x"))
                    .then(|| tokens.last().unwrap_or(&"unknown").to_string())
            }).or(zone).unwrap_or_else(|| "unknown interface".to_owned());
            insert_neighbor(&mut map, ip, mac, interface);
        }
    }
    map
}

fn read_neighbor_cache() -> Result<NeighborCache, String> {
    #[cfg(target_os = "windows")]
    {
        read_windows_neighbor_cache()
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut neighbors = HashMap::new();
        let mut any_source = false;
        #[cfg(target_os = "linux")]
        {
            if let Ok(contents) = std::fs::read_to_string("/proc/net/arp") {
                merge_neighbors(&mut neighbors, parse_neighbor_cache(&contents));
                any_source = true;
            }
            if let Ok(output) = Command::new("ip").args(["-6", "neigh", "show"]).output()
                && output.status.success()
            {
                merge_neighbors(&mut neighbors, parse_neighbor_cache(&String::from_utf8_lossy(
                    &output.stdout,
                )));
                any_source = true;
            }
        }
        #[cfg(target_os = "macos")]
        {
            if let Ok(output) = Command::new("arp").arg("-an").output()
                && output.status.success()
            {
                merge_neighbors(&mut neighbors, parse_neighbor_cache(&String::from_utf8_lossy(
                    &output.stdout,
                )));
                any_source = true;
            }
            if let Ok(output) = Command::new("ndp").arg("-an").output()
                && output.status.success()
            {
                merge_neighbors(&mut neighbors, parse_neighbor_cache(&String::from_utf8_lossy(
                    &output.stdout,
                )));
                any_source = true;
            }
        }
        if any_source {
            Ok(neighbors)
        } else {
            Err("No readable ARP/NDP neighbor-cache source on this host".to_owned())
        }
    }
}

#[cfg(target_os = "windows")]
fn read_windows_neighbor_cache() -> Result<NeighborCache, String> {
    use std::net::{Ipv4Addr, Ipv6Addr};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIpNetTable2, MIB_IPNET_TABLE2,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC};

    // GetIpNetTable2 is read-only: unlike ResolveIpNetEntry2, it never
    // issues ARP requests or IPv6 neighbor solicitations.
    let mut table: *mut MIB_IPNET_TABLE2 = std::ptr::null_mut();
    // SAFETY: The API initializes the out-pointer; the OS owns the result.
    let code = unsafe { GetIpNetTable2(AF_UNSPEC, &mut table) };
    if code != 0 {
        return Err(format!("GetIpNetTable2 failed (Windows error {code})"));
    }
    if table.is_null() {
        return Err("GetIpNetTable2 returned a null neighbor table".to_owned());
    }

    struct TableOwner(*mut MIB_IPNET_TABLE2);
    impl Drop for TableOwner {
        fn drop(&mut self) {
            // SAFETY: This allocation was returned by GetIpNetTable2 and
            // must be released by FreeMibTable exactly once.
            unsafe { FreeMibTable(self.0.cast()) };
        }
    }
    let owner = TableOwner(table);
    // SAFETY: GetIpNetTable2 provides NumEntries valid aligned rows.
    // Use the Table field's actual alignment: the header can contain padding.
    let rows = unsafe {
        std::slice::from_raw_parts((*owner.0).Table.as_ptr(), (*owner.0).NumEntries as usize)
    };
    let mut neighbors = NeighborCache::with_capacity(rows.len());
    for row in rows {
        if row.PhysicalAddressLength != 6 {
            continue;
        }
        // SAFETY: The si_family discriminant selects the active sockaddr union field.
        let ip = unsafe {
            if row.Address.si_family == AF_INET {
                IpAddr::V4(Ipv4Addr::from(
                    row.Address.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes(),
                ))
            } else if row.Address.si_family == AF_INET6 {
                IpAddr::V6(Ipv6Addr::from(row.Address.Ipv6.sin6_addr.u.Byte))
            } else {
                continue;
            }
        };
        let mac_bytes = &row.PhysicalAddress[..6];
        let mac = mac_bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":");
        if let Some(valid) = normalize_mac(&mac) {
            insert_neighbor(&mut neighbors, ip, valid, format!("ifIndex {}", row.InterfaceIndex));
        }
    }
    Ok(neighbors)
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
        let ip: IpAddr = "192.168.1.2".parse().unwrap();
        assert_eq!(select_neighbor(&table, &ip).0.as_deref(), Some("B8:27:EB:12:34:56"));
        assert_eq!(table[&ip][0].interface, "eth0");
        assert_eq!(select_neighbor(&table, &"192.168.1.3".parse().unwrap()).0.as_deref(), Some("00:1B:21:AA:BB:CC"));
        let v6: IpAddr = "fe80::2".parse().unwrap();
        assert_eq!(select_neighbor(&table, &v6).0.as_deref(), Some("00:00:0C:AA:BB:CC"));
        assert_eq!(table[&v6][0].interface, "en0");
        assert_eq!(oui_vendor(&table[&ip][0].mac), Some("Raspberry Pi Foundation"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn native_windows_neighbor_enumeration_is_read_only_and_operational() {
        // Windows CI executes the real IP Helper API without needing hosts
        // on the local link. Zero cached entries is a valid result.
        let result = read_windows_neighbor_cache();
        assert!(result.is_ok(), "GetIpNetTable2 query failed: {result:?}");
        for (_ip, entries) in result.unwrap() {
            for entry in entries {
                assert_eq!(normalize_mac(&entry.mac), Some(entry.mac));
                assert!(entry.interface.starts_with("ifIndex "));
            }
        }
    }

    #[test]
    fn same_ipv6_linklocal_on_two_adapters_is_not_misattributed() {
        let ip: IpAddr = "fe80::1234".parse().unwrap();
        let mut cache = NeighborCache::new();
        insert_neighbor(&mut cache, ip, "00:1B:21:11:22:33".to_owned(), "ifIndex 4".to_owned());
        insert_neighbor(&mut cache, ip, "00:1B:21:AA:BB:CC".to_owned(), "ifIndex 12".to_owned());
        let (mac, interfaces, ambiguous) = select_neighbor(&cache, &ip);
        assert!(mac.is_none());
        assert!(ambiguous);
        assert_eq!(interfaces.len(), 2);
    }

    #[test]
    fn distinguishes_ptr_success_absence_and_resolver_errors() {
        let ip: IpAddr = "192.0.2.10".parse().unwrap();
        let (name, state) = classify_ptr(ip, Ok("host.example.test".to_owned()));
        assert_eq!(name.as_deref(), Some("host.example.test"));
        assert_eq!(state, PtrStatus::Resolved);
        let (name, state) = classify_ptr(ip, Ok(ip.to_string()));
        assert!(name.is_none());
        assert_eq!(state, PtrStatus::NoName);
        let (name, state) = classify_ptr(ip, Err(io::Error::from(io::ErrorKind::NotFound)));
        assert!(name.is_none());
        assert_eq!(state, PtrStatus::NoName);
        let (_, state) = classify_ptr(
            ip,
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "synthetic resolver timeout",
            )),
        );
        assert!(matches!(state, PtrStatus::Failed(reason) if reason.contains("timeout")));
    }

    #[test]
    fn invalid_multicast_and_randomized_mac_are_not_falsely_attributed() {
        assert!(oui_prefix("ff:ff:ff:ff:ff:ff").is_none());
        assert!(oui_prefix("this-is-not-a-mac").is_none());
        assert_eq!(oui_vendor("02:00:00:00:00:01"), None);
        assert_eq!(oui_vendor("AA:BB:CC:00:11:22"), None);
    }
}
