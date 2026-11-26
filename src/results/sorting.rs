use std::collections::HashMap;
use std::net::IpAddr;

use crate::host_info::HostInfo;

use super::grouping::HostGroup;
use super::store::ResultRow;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortColumn {
    #[default]
    Address,
    Port,
    Service,
    Latency,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostSort {
    #[default]
    Address,
    Vendor,
    Endpoints,
    Rtt,
}

pub(super) fn sort_ports(
    rows: &[ResultRow],
    indices: &mut [usize],
    sort_column: SortColumn,
    sort_ascending: bool,
) {
    indices.sort_by(|&a_idx, &b_idx| {
        let a = &rows[a_idx].raw;
        let b = &rows[b_idx].raw;
        let ordering = match sort_column {
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

        let order = if sort_ascending {
            ordering
        } else {
            ordering.reverse()
        };
        // Stable ties retain insertion order even when sorting descending.
        order.then_with(|| a_idx.cmp(&b_idx))
    });
}

pub(super) fn sort_hosts(
    groups: &mut [HostGroup],
    info: &HashMap<IpAddr, HostInfo>,
    sort: HostSort,
    ascending: bool,
) {
    groups.sort_by(|a, b| {
        let vendor = |ip: IpAddr| {
            info.get(&ip)
                .and_then(|i| i.vendor.as_deref())
                .unwrap_or("")
        };
        let order = match sort {
            HostSort::Address => a.ip.cmp(&b.ip),
            HostSort::Vendor => vendor(a.ip).cmp(vendor(b.ip)),
            HostSort::Endpoints => a.ports_count.cmp(&b.ports_count),
            HostSort::Rtt => {
                (!a.has_measured_rtt, a.min_rtt).cmp(&(!b.has_measured_rtt, b.min_rtt))
            }
        }
        .then_with(|| a.ip.cmp(&b.ip));
        if ascending { order } else { order.reverse() }
    });
}
