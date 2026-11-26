use std::collections::HashMap;
use std::net::IpAddr;

use crate::host_info::HostInfo;
use crate::scanner::{PortState, ScanProtocol};

use super::projection::ProjectionSettings;
use super::store::{ResultRow, contains_ascii_case_insensitive};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultFilters {
    pub vendor: String,
    pub service: String,
    pub transport: Option<ScanProtocol>,
    pub state: Option<PortState>,
    pub evidence: Option<bool>,
}
impl ResultFilters {
    pub(super) fn matches(
        &self,
        row: &ResultRow,
        info: Option<&HostInfo>,
        vendor: &str,
        service: &str,
    ) -> bool {
        (vendor.is_empty()
            || info
                .and_then(|i| i.vendor.as_deref())
                .is_some_and(|v| contains_ascii_case_insensitive(v, vendor)))
            && (service.is_empty() || contains_ascii_case_insensitive(&row.raw.service, service))
            && self.transport.is_none_or(|v| v == row.raw.protocol)
            && self.state.is_none_or(|v| v == row.raw.state)
            && self
                .evidence
                .is_none_or(|v| v == (row.raw.service_verified && row.raw.state == PortState::Open))
    }
}

pub(super) fn filter_indices(
    rows: &[ResultRow],
    info: &HashMap<IpAddr, HostInfo>,
    settings: &ProjectionSettings,
    indices: &mut Vec<usize>,
) {
    let query_lower = settings.results_filter.trim().to_ascii_lowercase();
    let vendor_filter = settings.filters.vendor.trim().to_ascii_lowercase();
    let service_filter = settings.filters.service.trim().to_ascii_lowercase();
    indices.clear();
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
}
