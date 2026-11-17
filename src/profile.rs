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
}

impl ScanProfile {
    pub fn new(
        targets: String,
        ports: String,
        timeout_ms: u64,
        concurrency: usize,
        banner_grabbing: bool,
        banner_timeout_ms: u64,
        protocol: crate::scanner::ScanProtocol,
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
        if !(50..=5000).contains(&self.timeout_ms) {
            return Err("Timeout must be between 50 and 5000 ms".to_owned());
        }
        if !(1..=1024).contains(&self.concurrency) {
            return Err("Concurrency must be between 1 and 1024".to_owned());
        }
        if !(100..=5000).contains(&self.banner_timeout_ms) {
            return Err("Banner timeout must be between 100 and 5000 ms".to_owned());
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
        ScanProfile::new(
            "127.0.0.1\n192.168.1.0/30".to_owned(),
            "22,80,443".to_owned(),
            800,
            256,
            true,
            1200,
            crate::scanner::ScanProtocol::Udp,
        )
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
            ),
            ScanProfile::new(
                "127.0.0.1".to_owned(),
                "0".to_owned(),
                800,
                1,
                false,
                1000,
                proto,
            ),
            ScanProfile::new(
                "127.0.0.1".to_owned(),
                "80".to_owned(),
                49,
                1,
                false,
                1000,
                proto,
            ),
            ScanProfile::new(
                "127.0.0.1".to_owned(),
                "80".to_owned(),
                800,
                1025,
                false,
                1000,
                proto,
            ),
            ScanProfile::new(
                "127.0.0.1".to_owned(),
                "80".to_owned(),
                800,
                1,
                false,
                50,
                proto,
            ),
        ];
        for profile in invalid {
            assert!(profile.validate().is_err());
        }
    }

    #[test]
    fn reports_missing_profile_file() {
        let path = std::env::temp_dir().join("ip_port_scanner_missing_profile.json");
        let _ = std::fs::remove_file(&path);
        assert!(load(&path).unwrap_err().contains("os error"));
    }
}
