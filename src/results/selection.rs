//! User selection, independent of the order and visibility of projected rows.
use std::net::IpAddr;

use super::store::{EndpointId, ResultsStore};
use crate::scanner::ScanResult;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    #[default]
    None,
    Host(IpAddr),
    Endpoint(EndpointId),
}

#[derive(Default)]
pub struct SelectionState {
    current: Selection,
}

impl SelectionState {
    pub fn select_host(&mut self, ip: IpAddr, store: &ResultsStore) -> bool {
        // Validate only at the transition, not on every frame.
        if !store.iter().any(|result| result.ip == ip) {
            return false;
        }
        self.current = Selection::Host(ip);
        true
    }

    pub fn select_endpoint(&mut self, id: EndpointId, store: &ResultsStore) -> bool {
        if store.index_of(id).is_none() {
            return false;
        }
        self.current = Selection::Endpoint(id);
        true
    }

    pub fn clear(&mut self) {
        self.current = Selection::None;
    }

    pub fn is_none(&self) -> bool {
        self.current == Selection::None
    }

    pub fn endpoint_index(&self, store: &ResultsStore) -> Option<usize> {
        match self.current {
            Selection::Endpoint(id) => store.index_of(id),
            _ => None,
        }
    }

    pub fn endpoint<'a>(&self, store: &'a ResultsStore) -> Option<&'a ScanResult> {
        match self.current {
            Selection::Endpoint(id) => store.resolve(id),
            _ => None,
        }
    }

    /// App clears selection whenever it clears its store. Host is not a physical device ID.
    pub fn host(&self, store: &ResultsStore) -> Option<IpAddr> {
        match self.current {
            Selection::None => None,
            Selection::Host(ip) => Some(ip),
            Selection::Endpoint(id) => store.resolve(id).map(|result| result.ip),
        }
    }

    pub fn copy_text(&self, store: &ResultsStore) -> Option<String> {
        match self.current {
            Selection::None => None,
            Selection::Host(ip) => Some(ip.to_string()),
            Selection::Endpoint(id) => store
                .resolve(id)
                .map(|result| format!("{}:{}", result.ip, result.port)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn observation(ip: IpAddr, port: u16) -> ScanResult {
        ScanResult::new(ip, port, "http", Duration::from_millis(2))
    }

    #[test]
    fn host_selection_is_independent_of_endpoint() {
        let mut store = ResultsStore::default();
        let ip = "2001:db8::1".parse().unwrap();
        let first = store.insert(observation(ip, 80)).unwrap();
        let second = store.insert(observation(ip, 80)).unwrap();
        let mut selection = SelectionState::default();
        assert!(selection.select_endpoint(second, &store));
        assert_eq!(selection.endpoint_index(&store), Some(1));
        assert!(selection.select_host(ip, &store));
        assert_eq!(selection.current, Selection::Host(ip));
        assert_eq!(selection.host(&store), Some(ip));
        assert!(selection.endpoint(&store).is_none());
        assert!(selection.endpoint_index(&store).is_none());
        assert_eq!(selection.copy_text(&store), Some(ip.to_string()));
        assert!(selection.select_endpoint(first, &store));
        assert_eq!(selection.endpoint_index(&store), Some(0));
        assert_eq!(selection.current, Selection::Endpoint(first));
        assert_eq!(selection.copy_text(&store), Some(format!("{ip}:80")));
    }

    #[test]
    fn stale_endpoint_id_is_rejected_after_clear() {
        let mut store = ResultsStore::default();
        let ip = "10.0.0.1".parse().unwrap();
        let old = store.insert(observation(ip, 80)).unwrap();
        let mut selection = SelectionState::default();
        assert!(selection.select_endpoint(old, &store));
        store.clear();
        let new = store.insert(observation(ip, 443)).unwrap();
        assert!(selection.endpoint_index(&store).is_none());
        assert!(selection.endpoint(&store).is_none());
        assert!(selection.host(&store).is_none());
        assert!(selection.copy_text(&store).is_none());
        assert!(!selection.select_endpoint(old, &store));
        assert!(selection.select_endpoint(new, &store));
        assert_eq!(selection.endpoint(&store).unwrap().port, 443);
        selection.clear();
        assert!(selection.is_none());
    }

    #[test]
    fn invalid_host_does_not_replace_valid_selection() {
        let mut store = ResultsStore::default();
        let ip = "10.0.0.1".parse().unwrap();
        let id = store.insert(observation(ip, 502)).unwrap();
        let mut selection = SelectionState::default();
        assert!(selection.select_endpoint(id, &store));
        assert!(!selection.select_host("10.0.0.9".parse().unwrap(), &store));
        assert_eq!(selection.current, Selection::Endpoint(id));
    }
}
