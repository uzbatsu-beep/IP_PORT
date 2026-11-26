//! Single owner of immutable scan observations and derived row text.
use crate::scanner::{PortState, ScanResult};

pub const MAX_STORED_RESULTS: usize = 100_000;

/// A session-scoped observation identity, independent of display order.
/// Distinct repeated observations of the same IP/port receive distinct IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EndpointId {
    generation: u64,
    index: usize,
}

#[derive(Clone, Debug)]
pub struct ResultRow {
    pub raw: ScanResult,
    pub ip_text: String,
    pub port_text: String,
    pub protocol_text: &'static str,
    pub state_text: &'static str,
    pub latency_text: String,
}

impl ResultRow {
    fn from_result(raw: ScanResult) -> Self {
        let ip_text = raw.ip.to_string();
        let port_text = raw.port.to_string();
        let protocol_text = raw.protocol.as_str();
        let state_text = raw.state.as_str();
        let latency_text = if raw.state == PortState::OpenFiltered {
            "—".to_owned()
        } else {
            format!("{:.2} ms", raw.rtt.as_secs_f64() * 1000.0)
        };
        Self {
            raw,
            ip_text,
            port_text,
            protocol_text,
            state_text,
            latency_text,
        }
    }

    pub(crate) fn matches_lowercase_query(&self, query_lower: &str) -> bool {
        query_lower.is_empty()
            || contains_ascii_case_insensitive(&self.ip_text, query_lower)
            || self.port_text.contains(query_lower)
            || contains_ascii_case_insensitive(self.protocol_text, query_lower)
            || contains_ascii_case_insensitive(self.state_text, query_lower)
            || contains_ascii_case_insensitive(&self.raw.service, query_lower)
            || self
                .raw
                .banner
                .as_ref()
                .is_some_and(|banner| contains_ascii_case_insensitive(banner, query_lower))
    }
}

pub(crate) fn contains_ascii_case_insensitive(haystack: &str, needle_lower: &str) -> bool {
    let needle = needle_lower.as_bytes();
    if needle.is_empty() {
        return true;
    }
    let hay = haystack.as_bytes();
    if needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle.iter())
            .all(|(h, n)| h.to_ascii_lowercase() == *n)
    })
}

#[derive(Default)]
pub struct ResultsStore {
    rows: Vec<ResultRow>,
    generation: u64,
    revision: u64,
    dropped: usize,
}

impl ResultsStore {
    pub fn insert(&mut self, observation: ScanResult) -> Option<EndpointId> {
        if self.rows.len() >= MAX_STORED_RESULTS {
            self.dropped = self.dropped.saturating_add(1);
            return None;
        }
        let id = EndpointId {
            generation: self.generation,
            index: self.rows.len(),
        };
        self.rows.push(ResultRow::from_result(observation));
        self.revision = self
            .revision
            .checked_add(1)
            .expect("result revision overflow");
        Some(id)
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.dropped = 0;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("result generation overflow");
        self.revision = self
            .revision
            .checked_add(1)
            .expect("result revision overflow");
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn dropped(&self) -> usize {
        self.dropped
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn id_at(&self, index: usize) -> Option<EndpointId> {
        (index < self.rows.len()).then_some(EndpointId {
            generation: self.generation,
            index,
        })
    }

    pub fn resolve(&self, id: EndpointId) -> Option<&ScanResult> {
        if id.generation != self.generation {
            return None;
        }
        self.rows.get(id.index).map(|row| &row.raw)
    }

    /// Compatibility with the index-based projection until C2/C3.
    pub fn get(&self, index: usize) -> Option<&ScanResult> {
        self.resolve(self.id_at(index)?)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &ScanResult> {
        self.rows.iter().map(|row| &row.raw)
    }

    pub fn rows(&self) -> &[ResultRow] {
        &self.rows
    }

    /// Export takes an owned snapshot once, never a second live result store.
    pub fn snapshot(&self) -> Vec<ScanResult> {
        self.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    fn observation(port: u16) -> ScanResult {
        ScanResult::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
            "http",
            Duration::from_millis(2),
        )
    }

    #[test]
    fn ids_are_stable_and_repeated_observations_are_distinct() {
        let mut store = ResultsStore::default();
        let first = store.insert(observation(80)).unwrap();
        let second = store.insert(observation(80)).unwrap();
        assert_ne!(first, second);
        for _ in 0..2000 {
            store.insert(observation(443)).unwrap();
        }
        assert_eq!(store.id_at(0), Some(first));
        assert_eq!(store.resolve(first).unwrap().port, 80);
        assert_eq!(store.resolve(second).unwrap().port, 80);
        assert_eq!(store.revision(), 2002);
        assert!(store.id_at(store.len()).is_none());
    }

    #[test]
    fn clear_invalidates_old_ids_even_when_index_is_reused() {
        let mut store = ResultsStore::default();
        let old = store.insert(observation(80)).unwrap();
        let revision = store.revision();
        store.clear();
        assert!(store.is_empty());
        assert!(store.revision() > revision);
        assert!(store.resolve(old).is_none());
        let new = store.insert(observation(443)).unwrap();
        assert_ne!(old, new);
        assert!(store.resolve(old).is_none());
        assert_eq!(store.resolve(new).unwrap().port, 443);
    }

    #[test]
    fn saturation_and_snapshot_preserve_order_and_revision() {
        let mut store = ResultsStore::default();
        for index in 0..MAX_STORED_RESULTS {
            store.insert(observation((index % 100 + 1) as u16)).unwrap();
        }
        let revision = store.revision();
        assert!(store.insert(observation(443)).is_none());
        assert_eq!(store.dropped(), 1);
        assert_eq!(store.revision(), revision);
        assert_eq!(store.len(), MAX_STORED_RESULTS);
        let snapshot = store.snapshot();
        assert_eq!(snapshot.len(), MAX_STORED_RESULTS);
        assert_eq!(snapshot[0].port, 1);
        assert_eq!(snapshot[MAX_STORED_RESULTS - 1].port, 100);
        store.clear();
        store.insert(observation(502)).unwrap();
        assert_eq!(store.dropped(), 0);
        assert_eq!(snapshot[0].port, 1);
        assert_eq!(store.get(0).unwrap().port, 502);
    }
}
