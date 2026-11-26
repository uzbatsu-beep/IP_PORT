use super::*;
use crate::results::store::contains_ascii_case_insensitive;
use crate::scanner::{ScanProtocol, ScanResult};
use std::time::Duration;

// Reference: C1 full rebuild, including stable sort from insertion order.
// Reuses the original predicates and HostGroup summary, not the new caches.
fn legacy(
    store: &ResultsStore,
    info: &HashMap<IpAddr, HostInfo>,
    settings: &ProjectionSettings,
) -> (Vec<usize>, Vec<HostGroup>) {
    let rows = store.rows();
    let query_lower = settings.results_filter.trim().to_ascii_lowercase();
    let vendor_filter = settings.filters.vendor.trim().to_ascii_lowercase();
    let service_filter = settings.filters.service.trim().to_ascii_lowercase();
    let mut indices = Vec::new();
    for (idx, row) in rows.iter().enumerate() {
        let info_matches =
            info.get(&row.raw.ip).is_some_and(|info| {
                info.hostname
                    .as_deref()
                    .is_some_and(|value| contains_ascii_case_insensitive(value, &query_lower))
                    || info.mac.as_deref().is_some_and(|value| {
                        contains_ascii_case_insensitive(value, &query_lower)
                    })
                    || info.vendor.as_deref().is_some_and(|value| {
                        contains_ascii_case_insensitive(value, &query_lower)
                    })
            });
        if (row.matches_lowercase_query(&query_lower) || info_matches)
            && settings.filters.matches(
                row,
                info.get(&row.raw.ip),
                &vendor_filter,
                &service_filter,
            )
        {
            indices.push(idx);
        }
    }
    indices.sort_by(|&a_idx, &b_idx| {
        let a = &rows[a_idx].raw;
        let b = &rows[b_idx].raw;
        let ordering = match settings.sort_column {
            SortColumn::Address => a.ip.cmp(&b.ip).then_with(|| a.port.cmp(&b.port)),
            SortColumn::Port => a.port.cmp(&b.port).then_with(|| a.ip.cmp(&b.ip)),
            SortColumn::Service => a
                .service
                .cmp(&b.service)
                .then_with(|| a.ip.cmp(&b.ip))
                .then_with(|| a.port.cmp(&b.port)),
            SortColumn::Latency => a
                .rtt
                .cmp(&b.rtt)
                .then_with(|| a.ip.cmp(&b.ip))
                .then_with(|| a.port.cmp(&b.port)),
        };

        if settings.sort_ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
    // Keep display order while using O(1) average lookup for each unique host.
    let mut groups_map: Vec<(IpAddr, Vec<usize>)> = Vec::new();
    let mut host_indices = std::collections::HashMap::<IpAddr, usize>::new();
    for &idx in &indices {
        let ip = rows[idx].raw.ip;
        if let Some(&group_idx) = host_indices.get(&ip) {
            groups_map[group_idx].1.push(idx);
        } else {
            host_indices.insert(ip, groups_map.len());
            groups_map.push((ip, vec![idx]));
        }
    }
    let mut groups: Vec<HostGroup> = groups_map
        .into_iter()
        .map(|(ip, indices)| HostGroup::build(ip, indices, rows))
        .collect();
    groups.sort_by(|a, b| {
        let vendor = |ip: IpAddr| {
            info.get(&ip)
                .and_then(|i| i.vendor.as_deref())
                .unwrap_or("")
        };
        let order = match settings.host_sort {
            HostSort::Address => a.ip.cmp(&b.ip),
            HostSort::Vendor => vendor(a.ip).cmp(vendor(b.ip)),
            HostSort::Endpoints => a.ports_count.cmp(&b.ports_count),
            HostSort::Rtt => {
                (!a.has_measured_rtt, a.min_rtt).cmp(&(!b.has_measured_rtt, b.min_rtt))
            }
        }
        .then_with(|| a.ip.cmp(&b.ip));
        if settings.host_sort_ascending { order } else { order.reverse() }
    });
    (indices, groups)
}

fn fixture() -> ResultsStore {
    let mut store = ResultsStore::default();
    for (ip, port, service, ms) in [
        ("10.0.0.2", 443, "https", 9),
        ("10.0.0.1", 80, "http", 2),
        ("10.0.0.1", 22, "ssh", 4),
        ("10.0.0.1", 80, "http", 2),
        ("2001:db8::1", 502, "modbus", 7),
        ("10.0.0.3", 502, "unknown", 7),
    ] {
        store.insert(ScanResult::new(
            ip.parse().unwrap(),
            port,
            service,
            Duration::from_millis(ms),
        )).unwrap();
    }
    store
}

fn assert_legacy(
    projection: &mut ResultsProjection,
    store: &ResultsStore,
    info: &HashMap<IpAddr, HostInfo>,
) {
    projection.ensure_device_rows(store, info);
    let (indices, groups) = legacy(store, info, &projection.settings);
    assert_eq!(projection.visible_indices(), indices.as_slice());
    assert_eq!(projection.host_groups(), groups.as_slice());
    let mut expected = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        expected.push(DeviceRow::Host(index));
        if projection.host_expanded(group.ip) {
            expected.extend(group.indices.iter().copied().map(DeviceRow::Endpoint));
        }
    }
    assert_eq!(projection.device_rows(), expected.as_slice());
}

#[test]
fn all_sort_transitions_and_filters_match_legacy() {
    let store = fixture();
    let info = HashMap::from([(
        "10.0.0.1".parse().unwrap(),
        HostInfo {
            hostname: Some("line1.example".to_owned()),
            vendor: Some("Siemens".to_owned()),
            ..HostInfo::default()
        },
    )]);
    let mut projection = ResultsProjection::default();
    projection.toggle_host("10.0.0.1".parse().unwrap());
    for query in ["", "HTTP", "2001:db8", "missing", "LINE1"] {
        projection.settings.results_filter = query.to_owned();
        for transport in [None, Some(ScanProtocol::Tcp), Some(ScanProtocol::Udp)] {
            projection.settings.filters.transport = transport;
            for sort in [SortColumn::Address, SortColumn::Port, SortColumn::Service, SortColumn::Latency] {
                projection.settings.sort_column = sort;
                for ascending in [false, true] {
                    projection.settings.sort_ascending = ascending;
                    for host_sort in [HostSort::Address, HostSort::Vendor, HostSort::Endpoints, HostSort::Rtt] {
                        projection.settings.host_sort = host_sort;
                        for host_ascending in [false, true] {
                            projection.settings.host_sort_ascending = host_ascending;
                            assert_legacy(&mut projection, &store, &info);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn invalidation_is_stage_specific() {
    let mut store = fixture();
    let info = HashMap::new();
    let mut projection = ResultsProjection::default();
    assert_legacy(&mut projection, &store, &info);
    let initial = projection.metrics();
    assert_legacy(&mut projection, &store, &info);
    assert_eq!(projection.metrics(), initial);
    projection.settings.host_sort = HostSort::Endpoints;
    assert_legacy(&mut projection, &store, &info);
    let host_sort = projection.metrics();
    assert_eq!(host_sort.filter_rebuilds, initial.filter_rebuilds);
    assert_eq!(host_sort.port_sorts, initial.port_sorts);
    assert_eq!(host_sort.group_rebuilds, initial.group_rebuilds);
    assert_eq!(host_sort.host_sorts, initial.host_sorts + 1);
    projection.toggle_host("10.0.0.1".parse().unwrap());
    assert_legacy(&mut projection, &store, &info);
    let expanded = projection.metrics();
    assert_eq!(expanded.view_rebuilds, host_sort.view_rebuilds);
    assert_eq!(expanded.device_row_rebuilds, host_sort.device_row_rebuilds + 1);
    projection.settings.sort_column = SortColumn::Latency;
    assert_legacy(&mut projection, &store, &info);
    let ports = projection.metrics();
    assert_eq!(ports.filter_rebuilds, expanded.filter_rebuilds);
    assert_eq!(ports.port_sorts, expanded.port_sorts + 1);
    assert_eq!(ports.group_rebuilds, expanded.group_rebuilds + 1);
    let old = projection.metrics();
    store.insert(ScanResult::new(
        "10.0.0.9".parse().unwrap(),
        102,
        "unknown",
        Duration::from_millis(5),
    )).unwrap();
    assert_legacy(&mut projection, &store, &info);
    assert_eq!(projection.metrics().filter_rebuilds, old.filter_rebuilds + 1);
    store.clear();
    projection.clear(&store);
    assert_legacy(&mut projection, &store, &info);
    assert!(projection.device_rows().is_empty());
    assert!(!projection.host_expanded("10.0.0.1".parse().unwrap()));
    store.insert(ScanResult::new(
        "10.0.0.9".parse().unwrap(),
        502,
        "modbus",
        Duration::from_millis(1),
    )).unwrap();
    assert_legacy(&mut projection, &store, &info);
    assert_eq!(projection.visible_indices(), &[0]);
    assert_eq!(store.snapshot()[0].port, 502);
}

#[test]
fn metadata_changes_refresh_matching_and_vendor_sort() {
    let store = fixture();
    let mut info = HashMap::new();
    let mut projection = ResultsProjection::default();
    projection.settings.results_filter = "line1".to_owned();
    assert_legacy(&mut projection, &store, &info);
    assert!(projection.visible_indices().is_empty());
    let initial = projection.metrics();
    info.insert(
        "10.0.0.1".parse().unwrap(),
        HostInfo {
            hostname: Some("LINE1.example".to_owned()),
            vendor: Some("Siemens".to_owned()),
            mac: Some("00:11:22:33:44:55".to_owned()),
            ..HostInfo::default()
        },
    );
    projection.invalidate_metadata();
    assert_legacy(&mut projection, &store, &info);
    assert_eq!(projection.metrics().filter_rebuilds, initial.filter_rebuilds + 1);
    assert_eq!(projection.visible_indices(), &[2, 1, 3]);
    projection.settings.results_filter.clear();
    projection.settings.filters.vendor = "SIEMENS".to_owned();
    assert_legacy(&mut projection, &store, &info);
    assert_eq!(projection.visible_indices(), &[2, 1, 3]);
    info.get_mut(&"10.0.0.1".parse().unwrap()).unwrap().vendor = Some("Other".to_owned());
    projection.invalidate_metadata();
    assert_legacy(&mut projection, &store, &info);
    assert!(projection.visible_indices().is_empty());
    assert_eq!(store.len(), 6);
}
