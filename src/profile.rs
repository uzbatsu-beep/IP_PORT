use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::scanner::targets;

const PROFILE_VERSION: u32 = 1;

fn default_banner_timeout_ms() -> u64 {
    1000
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScanProfile {
    pub version: u32,
    pub targets: String,
    pub ports: String,
    pub timeout_ms: u64,
    pub concurrency: usize,
    #[serde(default)]
    pub banner_grabbing: bool,
    #[serde(default = "default_banner_timeout_ms")]
    pub banner_timeout_ms: u64,
    #[serde(default)]
    pub protocol: crate::scanner::ScanProtocol,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_addr: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub exclusions: String,
    #[serde(default)]
    pub resolve_ptr: bool,
    #[serde(default)]
    pub read_neighbor_cache: bool,
}

impl ScanProfile {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        targets: String,
        ports: String,
        timeout_ms: u64,
        concurrency: usize,
        banner_grabbing: bool,
        banner_timeout_ms: u64,
        protocol: crate::scanner::ScanProtocol,
        bind_addr: Option<String>,
    ) -> Self {
        Self {
            version: PROFILE_VERSION,
            targets,
            ports,
            timeout_ms,
            concurrency,
            banner_grabbing,
            banner_timeout_ms,
            protocol,
            bind_addr,
            exclusions: String::new(),
            resolve_ptr: false,
            read_neighbor_cache: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != PROFILE_VERSION {
            return Err(format!(
                "Unsupported profile version {} (supported: {})",
                self.version, PROFILE_VERSION
            ));
        }
        targets::parse_targets(&self.targets)
            .map_err(|error| format!("Invalid targets: {error}"))?;
        targets::parse_ports(&self.ports).map_err(|error| format!("Invalid ports: {error}"))?;
        let excluded = crate::scanner::scope::Exclusions::parse(&self.exclusions)
            .map_err(|error| format!("Invalid exclusions: {error}"))?;
        let targets = targets::parse_targets(&self.targets)
            .map_err(|error| format!("Invalid targets: {error}"))?;
        if targets.hostnames.is_empty() && targets.ips.iter().all(|ip| excluded.contains(ip)) {
            return Err("All targets are excluded from scan scope".to_owned());
        }
        if !(50..=5000).contains(&self.timeout_ms) {
            return Err("Timeout must be between 50 and 5000 ms".to_owned());
        }
        if !(1..=1024).contains(&self.concurrency) {
            return Err("Concurrency must be between 1 and 1024".to_owned());
        }
        if !(100..=5000).contains(&self.banner_timeout_ms) {
            return Err("Banner timeout must be between 100 and 5000 ms".to_owned());
        }
        if let Some(bind) = &self.bind_addr
            && !bind.trim().is_empty()
            && !bind.trim().eq_ignore_ascii_case("default")
            && !bind.trim().eq_ignore_ascii_case("any")
            && bind.trim().parse::<std::net::IpAddr>().is_err()
        {
            return Err(format!("Invalid bind address in profile: {bind}"));
        }
        Ok(())
    }
}

pub fn save(path: &Path, profile: &ScanProfile) -> io::Result<()> {
    profile
        .validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let file = std::fs::File::create(path)?;
    serde_json::to_writer_pretty(file, profile).map_err(io::Error::other)
}

pub fn load(path: &Path) -> Result<ScanProfile, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let profile: ScanProfile =
        serde_json::from_reader(file).map_err(|error| format!("Invalid profile file: {error}"))?;
    profile.validate()?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ScanProfile {
        let mut profile = ScanProfile::new(
            "127.0.0.1\n192.168.1.0/30".to_owned(),
            "22,80,443".to_owned(),
            800,
            256,
            true,
            1200,
            crate::scanner::ScanProtocol::Udp,
            Some("192.168.1.50".to_owned()),
        );
        profile.exclusions = "10.10.0.0/16".to_owned();
        profile.resolve_ptr = true;
        profile.read_neighbor_cache = true;
        profile
    }

    #[test]
    fn saves_and_loads_profile_round_trip() {
        let path = std::env::temp_dir().join("ip_port_scanner_profile_test.json");
        let profile = sample();
        save(&path, &profile).unwrap();
        assert_eq!(load(&path).unwrap(), profile);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn loads_legacy_profile_without_banner_fields() {
        let path = std::env::temp_dir().join("ip_port_scanner_legacy_profile_test.json");
        std::fs::write(
            &path,
            r#"{"version":1,"targets":"127.0.0.1","ports":"80","timeout_ms":800,"concurrency":10}"#,
        )
        .unwrap();
        let loaded = load(&path).unwrap();
        assert!(!loaded.banner_grabbing);
        assert!(loaded.exclusions.is_empty());
        assert!(!loaded.resolve_ptr);
        assert!(!loaded.read_neighbor_cache);
        assert_eq!(loaded.banner_timeout_ms, 1000);
        assert_eq!(loaded.protocol, crate::scanner::ScanProtocol::Tcp);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_unknown_version_and_fields() {
        let path = std::env::temp_dir().join("ip_port_scanner_invalid_profile.json");
        std::fs::write(
            &path,
            r#"{"version":2,"targets":"127.0.0.1","ports":"80","timeout_ms":800,"concurrency":1}"#,
        )
        .unwrap();
        assert!(
            load(&path)
                .unwrap_err()
                .contains("Unsupported profile version")
        );

        std::fs::write(
            &path,
            r#"{"version":1,"targets":"127.0.0.1","ports":"80","timeout_ms":800,"concurrency":1,"extra":true}"#,
        )
        .unwrap();
        assert!(load(&path).unwrap_err().contains("unknown field"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_invalid_profile_values() {
        let proto = crate::scanner::ScanProtocol::Tcp;
        let invalid = [
            ScanProfile::new(
                "http://invalid".to_owned(),
                "80".to_owned(),
                800,
                1,
                false,
                1000,
                proto,
                None,
            ),
            ScanProfile::new(
                "127.0.0.1".to_owned(),
                "0".to_owned(),
                800,
                1,
                false,
                1000,
                proto,
                None,
            ),
            ScanProfile::new(
                "127.0.0.1".to_owned(),
                "80".to_owned(),
                49,
                1,
                false,
                1000,
                proto,
                None,
            ),
            ScanProfile::new(
                "127.0.0.1".to_owned(),
                "80".to_owned(),
                800,
                1025,
                false,
                1000,
                proto,
                None,
            ),
            ScanProfile::new(
                "127.0.0.1".to_owned(),
                "80".to_owned(),
                800,
                1,
                false,
                50,
                proto,
                None,
            ),
            ScanProfile::new(
                "127.0.0.1".to_owned(),
                "80".to_owned(),
                800,
                1,
                false,
                1000,
                proto,
                Some("999.999.999.999".to_owned()),
            ),
        ];
        for profile in invalid {
            assert!(profile.validate().is_err());
        }
    }

    #[test]
    fn rejects_malformed_exclusions_in_profile() {
        let mut profile = sample();
        profile.exclusions = "not-an-ip".to_owned();
        assert!(
            profile
                .validate()
                .unwrap_err()
                .contains("Invalid exclusions")
        );
        profile.exclusions = "127.0.0.0/8\n192.168.1.0/24".to_owned();
        assert!(profile.validate().unwrap_err().contains("All targets"));
    }

    #[test]
    fn reports_missing_profile_file() {
        let path = std::env::temp_dir().join("ip_port_scanner_missing_profile.json");
        let _ = std::fs::remove_file(&path);
        assert!(load(&path).unwrap_err().contains("os error"));
    }
}
