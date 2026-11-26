use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use crate::scanner::PortState;

use super::store::ResultRow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceRow {
    Host(usize),
    Endpoint(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct HostGroup {
    pub ip: IpAddr,
    pub ip_text: String,
    pub indices: Vec<usize>,
    pub ports_count: usize,
    pub services_summary: String,
    pub min_rtt: Duration,
    pub has_measured_rtt: bool,
}

impl HostGroup {
    pub fn build(ip: IpAddr, indices: Vec<usize>, rows: &[ResultRow]) -> Self {
        let ip_text = ip.to_string();
        let ports_count = indices.len();
        let confirmed_rtt = indices
            .iter()
            .filter(|&&idx| rows[idx].raw.state == PortState::Open)
            .map(|&idx| rows[idx].raw.rtt)
            .min();
        let min_rtt = confirmed_rtt.unwrap_or_default();

        let mut distinct_services: Vec<&str> = Vec::new();
        for &idx in &indices {
            let svc = rows[idx].raw.service.as_str();
            if svc != "unknown" && !distinct_services.contains(&svc) {
                distinct_services.push(svc);
            }
        }

        let services_summary = if distinct_services.is_empty() {
            "unknown".to_owned()
        } else if distinct_services.len() <= 3 {
            distinct_services.join(", ")
        } else {
            format!(
                "{}, +{} more",
                distinct_services[..3].join(", "),
                distinct_services.len() - 3
            )
        };

        Self {
            ip,
            ip_text,
            indices,
            ports_count,
            services_summary,
            min_rtt,
            has_measured_rtt: confirmed_rtt.is_some(),
        }
    }
}

pub(super) fn group_hosts(indices: &[usize], rows: &[ResultRow]) -> Vec<HostGroup> {
    // Keep display order while using O(1) average lookup for each unique host.
    let mut groups_map: Vec<(IpAddr, Vec<usize>)> = Vec::new();
    let mut host_indices = HashMap::<IpAddr, usize>::new();
    for &idx in indices {
        let ip = rows[idx].raw.ip;
        if let Some(&group_idx) = host_indices.get(&ip) {
            groups_map[group_idx].1.push(idx);
        } else {
            host_indices.insert(ip, groups_map.len());
            groups_map.push((ip, vec![idx]));
        }
    }
    groups_map
        .into_iter()
        .map(|(ip, indices)| HostGroup::build(ip, indices, rows))
        .collect()
}
