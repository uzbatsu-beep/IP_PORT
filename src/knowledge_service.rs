//! Read-only facade over independent knowledge sources. No network dispatch.
use crate::scanner::{ScanProtocol, presets, targets};
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
pub struct Catalog {
    pub schema_version: u32,
    pub reviewed: String,
    pub entries: Vec<Entry>,
}

#[derive(Deserialize)]
pub struct Entry {
    pub id: String,
    pub vendor: String,
    pub family: String,
    pub transport: String,
    pub ports: String,
    pub purpose: String,
    pub notes: String,
    pub kind: String,
    pub source: String,
}

pub fn catalog() -> &'static Result<Catalog, String> {
    static DATA: OnceLock<Result<Catalog, String>> = OnceLock::new();
    DATA.get_or_init(|| {
        let data: Catalog = serde_json::from_str(include_str!("../data/industrial_protocols.json"))
            .map_err(|error| error.to_string())?;
        if data.schema_version != 1 {
            return Err("Unsupported catalog schema".to_owned());
        }
        validate_catalog(&data)?;
        Ok(data)
    })
}

fn validate_catalog(data: &Catalog) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    if data.entries.is_empty() || data.reviewed.trim().is_empty() {
        return Err("Empty catalog or review date".to_owned());
    }
    for entry in &data.entries {
        if !ids.insert(entry.id.as_str())
            || [&entry.id, &entry.vendor, &entry.family, &entry.transport, &entry.ports, &entry.purpose, &entry.notes, &entry.kind, &entry.source].iter().any(|v| v.trim().is_empty())
            || !entry.source.starts_with("https://")
            || !matches!(entry.kind.as_str(), "documented_default" | "conditional" | "configurable" | "unverified_port" | "layer2" | "registered_candidate" | "research_pending")
        {
            return Err(format!("Invalid or duplicate catalog entry {}", entry.id));
        }
    }
    for id in CANDIDATE_IDS {
        let entry = data.entries.iter().find(|e| e.id == *id).ok_or_else(|| format!("Missing candidate {id}"))?;
        if entry.kind != "documented_default" || !matches!(entry.transport.as_str(), "TCP" | "UDP" | "TCP/UDP") {
            return Err(format!("Invalid candidate {id}"));
        }
        targets::parse_ports(&entry.ports.replace('–', "-")).map_err(|e| format!("Candidate {id}: {e}"))?;
    }
    for (name, group) in PROTOCOL_GROUPS {
        if group.is_empty() || !group.iter().all(|id| CANDIDATE_IDS.contains(id)) {
            return Err(format!("Invalid protocol group {name}"));
        }
    }
    Ok(())
}

/// Current services registry and MAC allocation counts remain independent sources.
pub fn source_summary(app: &crate::app::App) -> (String, (usize, usize, usize)) {
    (
        crate::scanner::services::services_db_info(),
        app.oui_registry.counts(),
    )
}

/// Deliberate review allowlist, not automatic promotion of catalog records.
/// Excludes discovery-only broadcasts, ambiguous direction, cyclic IO, traps,
/// unverified/configurable ports and non-IP transport. These are candidates only.
const CANDIDATE_IDS: &[&str] = &[
    "ot-001", "ot-004", "ot-005", "ot-006", "ot-007", "ot-008", "ot-010", "ot-014", "ot-015",
    "ot-022", "ot-023", "ot-026", "ot-027", "ot-030", "ot-031", "ot-032", "ot-033", "ot-034",
    "ot-035", "ot-036", "ot-038", "ot-040", "ot-041", "ot-042", "ot-043", "ot-044",
];

pub const PROTOCOL_GROUPS: &[(&str, &[&str])] = &[
    ("Modbus TCP", &["ot-040", "ot-044"]),
    ("S7 Communication", &["ot-022"]),
    ("EtherNet/IP", &["ot-027", "ot-042", "ot-043"]),
    ("FINS", &["ot-026"]),
    ("OPC UA", &["ot-030", "ot-041"]),
    ("ADS / Secure ADS", &["ot-038"]),
    ("YMConnect / High Speed Ethernet Server", &["ot-015"]),
    (
        "CODESYS Runtime / Gateway",
        &["ot-031", "ot-032", "ot-033", "ot-035", "ot-036"],
    ),
    (
        "HMI communication / diagnostics",
        &["ot-007", "ot-008", "ot-010", "ot-014"],
    ),
];

pub fn vendors(data: &Catalog) -> Vec<&str> {
    let mut values: Vec<_> = data
        .entries
        .iter()
        .map(|entry| entry.vendor.as_str())
        .collect();
    values.sort_unstable();
    values.dedup();
    values
}

pub fn families<'a>(data: &'a Catalog, vendor: &str) -> Vec<&'a str> {
    let mut values: Vec<_> = data
        .entries
        .iter()
        .filter(|entry| entry.vendor == vendor)
        .map(|entry| entry.family.as_str())
        .collect();
    values.sort_unstable();
    values.dedup();
    values
}

#[derive(Clone, Copy)]
pub enum Selection<'a> {
    Vendor {
        vendor: &'a str,
        family: Option<&'a str>,
    },
    Protocol(&'a str),
}

pub struct Candidates {
    pub ports: Vec<u16>,
    pub entries: Vec<&'static Entry>,
}

impl Candidates {
    pub fn port_text(&self) -> String {
        presets::format_ports(&self.ports)
    }
}

pub fn candidates(selection: Selection<'_>, protocol: ScanProtocol) -> Result<Candidates, String> {
    let data = catalog().as_ref().map_err(Clone::clone)?;
    let mut ports = Vec::new();
    let mut entries = Vec::new();
    for entry in &data.entries {
        if entry.kind != "documented_default" || !CANDIDATE_IDS.contains(&entry.id.as_str()) {
            continue;
        }
        let selected = match selection {
            Selection::Vendor { vendor, family } => {
                entry.vendor == vendor && family.is_none_or(|name| entry.family == name)
            }
            Selection::Protocol(name) => PROTOCOL_GROUPS
                .iter()
                .any(|(label, ids)| *label == name && ids.contains(&entry.id.as_str())),
        };
        let transport = match protocol {
            ScanProtocol::Tcp => matches!(entry.transport.as_str(), "TCP" | "TCP/UDP"),
            ScanProtocol::Udp => matches!(entry.transport.as_str(), "UDP" | "TCP/UDP"),
        };
        if !selected || !transport {
            continue;
        }
        let normalized = entry.ports.replace('–', "-");
        ports.extend(
            targets::parse_ports(&normalized)
                .map_err(|error| format!("Catalog entry {}: {error}", entry.id))?,
        );
        entries.push(entry);
    }
    ports.sort_unstable();
    ports.dedup();
    Ok(Candidates { ports, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_integrity_rejects_duplicate_and_missing_candidate() {
        let mut data: Catalog = serde_json::from_str(include_str!("../data/industrial_protocols.json")).unwrap();
        assert!(validate_catalog(&data).is_ok());
        data.entries[1].id = data.entries[0].id.clone();
        assert!(validate_catalog(&data).is_err());
    }

    #[test]
    fn presets_derive_only_reviewed_transport_specific_candidates() {
        let data = catalog().as_ref().unwrap();
        for id in CANDIDATE_IDS {
            let entry = data.entries.iter().find(|entry| entry.id == *id).unwrap();
            assert_eq!(entry.kind, "documented_default");
            assert!(targets::parse_ports(&entry.ports.replace('–', "-")).is_ok());
        }
        for (name, ids) in PROTOCOL_GROUPS {
            assert!(!ids.is_empty(), "{name}");
            assert!(ids.iter().all(|id| CANDIDATE_IDS.contains(id)));
        }
        let nx = candidates(
            Selection::Vendor {
                vendor: "Yaskawa",
                family: Some("NX100"),
            },
            ScanProtocol::Udp,
        )
        .unwrap();
        assert!(nx.ports.is_empty());
        let yaskawa = candidates(
            Selection::Vendor {
                vendor: "Yaskawa",
                family: None,
            },
            ScanProtocol::Udp,
        )
        .unwrap();
        assert_eq!(yaskawa.ports, [10040, 10041]);
        let omron = candidates(
            Selection::Vendor {
                vendor: "Omron",
                family: None,
            },
            ScanProtocol::Udp,
        )
        .unwrap();
        assert_eq!(omron.ports, [9600, 44818]);
        let codesys = candidates(
            Selection::Protocol("CODESYS Runtime / Gateway"),
            ScanProtocol::Udp,
        )
        .unwrap();
        assert_eq!(codesys.ports, [1740, 1741, 1742, 1743]);
        let modbus = candidates(Selection::Protocol("Modbus TCP"), ScanProtocol::Tcp).unwrap();
        assert_eq!(modbus.ports, [502]);
        assert!(
            candidates(Selection::Protocol("Modbus TCP"), ScanProtocol::Udp)
                .unwrap()
                .ports
                .is_empty()
        );
    }
}
