//! Index-only cached views; independent of App and egui.
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;

use crate::host_info::HostInfo;

use super::filters::{ResultFilters, filter_indices};
use super::grouping::{DeviceRow, HostGroup, group_hosts};
use super::sorting::{HostSort, SortColumn, sort_hosts, sort_ports};
use super::store::ResultsStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionSettings {
    pub results_filter: String,
    pub filters: ResultFilters,
    pub sort_column: SortColumn,
    pub sort_ascending: bool,
    pub host_sort: HostSort,
    pub host_sort_ascending: bool,
}

impl Default for ProjectionSettings {
    fn default() -> Self {
        Self {
            results_filter: String::new(),
            filters: ResultFilters::default(),
            sort_column: SortColumn::Address,
            sort_ascending: true,
            host_sort: HostSort::Address,
            host_sort_ascending: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ProjectionMetrics {
    pub view_rebuilds: usize,
    pub filter_rebuilds: usize,
    pub port_sorts: usize,
    pub group_rebuilds: usize,
    pub host_sorts: usize,
    pub device_row_rebuilds: usize,
}

#[derive(Default)]
pub struct ResultsProjection {
    // Editable configuration; derived data remains private.
    pub settings: ProjectionSettings,
    cached_settings: ProjectionSettings,
    results_revision: u64,
    metadata_revision: u64,
    cached_metadata_revision: u64,
    visible_indices: Vec<usize>,
    host_groups: Vec<HostGroup>,
    device_rows: Vec<DeviceRow>,
    expanded_hosts: HashSet<IpAddr>,
    device_rows_dirty: bool,
    metrics: ProjectionMetrics,
}

impl ResultsProjection {
    pub fn invalidate_metadata(&mut self) {
        self.metadata_revision = self.metadata_revision.wrapping_add(1);
    }

    pub fn ensure(&mut self, store: &ResultsStore, info: &HashMap<IpAddr, HostInfo>) {
        let settings = &self.settings;
        let cached = &self.cached_settings;
        let filter_dirty = self.results_revision != store.revision()
            || self.cached_metadata_revision != self.metadata_revision
            || cached.results_filter != settings.results_filter
            || cached.filters != settings.filters;
        let ports_dirty = filter_dirty
            || cached.sort_column != settings.sort_column
            || cached.sort_ascending != settings.sort_ascending;
        let hosts_dirty = ports_dirty
            || cached.host_sort != settings.host_sort
            || cached.host_sort_ascending != settings.host_sort_ascending;
        if !hosts_dirty {
            return;
        }

        let rows = store.rows();
        if filter_dirty {
            filter_indices(rows, info, settings, &mut self.visible_indices);
            self.metrics.filter_rebuilds += 1;
        }
        if ports_dirty {
            sort_ports(
                rows,
                &mut self.visible_indices,
                settings.sort_column,
                settings.sort_ascending,
            );
            self.metrics.port_sorts += 1;
            self.host_groups = group_hosts(&self.visible_indices, rows);
            self.metrics.group_rebuilds += 1;
        }
        sort_hosts(
            &mut self.host_groups,
            info,
            settings.host_sort,
            settings.host_sort_ascending,
        );
        self.metrics.host_sorts += 1;
        self.metrics.view_rebuilds += 1;
        self.cached_settings = settings.clone();
        self.results_revision = store.revision();
        self.cached_metadata_revision = self.metadata_revision;
        self.device_rows_dirty = true;
    }

    pub fn ensure_device_rows(&mut self, store: &ResultsStore, info: &HashMap<IpAddr, HostInfo>) {
        self.ensure(store, info);
        if !self.device_rows_dirty {
            return;
        }
        self.device_rows.clear();
        for (index, group) in self.host_groups.iter().enumerate() {
            self.device_rows.push(DeviceRow::Host(index));
            if self.expanded_hosts.contains(&group.ip) {
                self.device_rows
                    .extend(group.indices.iter().copied().map(DeviceRow::Endpoint));
            }
        }
        self.device_rows_dirty = false;
        self.metrics.device_row_rebuilds += 1;
    }

    pub fn visible_indices(&self) -> &[usize] {
        &self.visible_indices
    }

    pub fn host_groups(&self) -> &[HostGroup] {
        &self.host_groups
    }

    pub fn device_rows(&self) -> &[DeviceRow] {
        &self.device_rows
    }

    pub fn host_expanded(&self, ip: IpAddr) -> bool {
        self.expanded_hosts.contains(&ip)
    }

    pub fn toggle_host(&mut self, ip: IpAddr) {
        if !self.expanded_hosts.remove(&ip) {
            self.expanded_hosts.insert(ip);
        }
        self.device_rows_dirty = true;
    }

    pub fn collapse_hosts(&mut self) {
        if !self.expanded_hosts.is_empty() {
            self.expanded_hosts.clear();
            self.device_rows_dirty = true;
        }
    }

    pub fn clear(&mut self, store: &ResultsStore) {
        self.visible_indices.clear();
        self.host_groups.clear();
        self.device_rows.clear();
        self.expanded_hosts.clear();
        self.device_rows_dirty = true;
        self.results_revision = store.revision();
        self.cached_metadata_revision = self.metadata_revision;
    }

    #[cfg(test)]
    pub fn metrics(&self) -> ProjectionMetrics {
        self.metrics
    }
}

#[cfg(test)]
mod tests;
