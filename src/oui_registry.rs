//! Optional IEEE Registration Authority CSV database: MA-L (24), MA-M (28),
//! MA-S (36). User imports local official files; no network download or tracking.
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_CSV_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ROWS: usize = 150_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryKind {
    MaL,
    MaM,
    MaS,
}

impl RegistryKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::MaL => "MA-L",
            Self::MaM => "MA-M",
            Self::MaS => "MA-S",
        }
    }
    fn from_assignment_length(length: usize) -> Option<Self> {
        match length {
            6 => Some(Self::MaL),
            7 => Some(Self::MaM),
            9 => Some(Self::MaS),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct OuiRegistry {
    ma_l: HashMap<u64, String>,
    ma_m: HashMap<u64, String>,
    ma_s: HashMap<u64, String>,
}

fn globally_assigned(mac: &str) -> Option<u64> {
    let compact: String = mac.chars().filter(|c| *c != ':' && *c != '-').collect();
    if compact.len() != 12 || !compact.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let number = u64::from_str_radix(&compact, 16).ok()?;
    let first_octet = (number >> 40) as u8;
    if first_octet & 3 != 0 || number == 0 || number == 0xFFFF_FFFF_FFFF {
        return None; // Local/multicast addresses have no authoritative vendor.
    }
    Some(number)
}

impl OuiRegistry {
    /// Move a previously validated registry class into this database; other
    /// classes stay available without cloning large vendor hash maps.
    pub fn replace_kind_from(&mut self, kind: RegistryKind, mut imported: Self) {
        match kind {
            RegistryKind::MaL => self.ma_l = std::mem::take(&mut imported.ma_l),
            RegistryKind::MaM => self.ma_m = std::mem::take(&mut imported.ma_m),
            RegistryKind::MaS => self.ma_s = std::mem::take(&mut imported.ma_s),
        }
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        (self.ma_l.len(), self.ma_m.len(), self.ma_s.len())
    }

    pub fn lookup(&self, mac: &str) -> Option<String> {
        let value = globally_assigned(mac)?;
        if let Some(vendor) = self.ma_s.get(&(value >> 12)) {
            return Some(vendor.clone());
        }
        if let Some(vendor) = self.ma_m.get(&(value >> 20)) {
            return Some(vendor.clone());
        }
        if let Some(vendor) = self.ma_l.get(&(value >> 24)) {
            return Some(vendor.clone());
        }
        crate::host_info::oui_vendor(mac).map(str::to_owned)
    }

    /// Atomic replacement of one IEEE registry; other imported registries stay.
    pub fn load_file(&mut self, path: &Path) -> Result<(RegistryKind, usize), String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut bytes = Vec::new();
        file.take(MAX_CSV_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Cannot read IEEE CSV: {e}"))?;
        if bytes.len() as u64 > MAX_CSV_BYTES {
            return Err("IEEE CSV exceeds 16 MiB safety limit".to_owned());
        }
        self.load_csv(&bytes)
    }

    pub fn load_csv(&mut self, input: &[u8]) -> Result<(RegistryKind, usize), String> {
        if input.len() as u64 > MAX_CSV_BYTES {
            return Err("IEEE CSV exceeds 16 MiB safety limit".to_owned());
        }
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .trim(csv::Trim::All)
            .from_reader(input);
        let headers = reader
            .headers()
            .map_err(|e| format!("Invalid CSV header: {e}"))?;
        let names: Vec<String> = headers
            .iter()
            .map(|s| s.trim_start_matches('\u{feff}').to_ascii_lowercase())
            .collect();
        let find = |name: &str| names.iter().position(|n| n == name);
        let registry_col = find("registry").ok_or("Missing IEEE CSV field 'Registry'")?;
        let assignment_col = find("assignment").ok_or("Missing IEEE CSV field 'Assignment'")?;
        let vendor_col =
            find("organization name").ok_or("Missing IEEE CSV field 'Organization Name'")?;
        let mut entries = HashMap::new();
        let mut kind = None;
        for (line, row) in reader.records().enumerate() {
            if line >= MAX_ROWS {
                return Err("IEEE CSV contains too many entries".to_owned());
            }
            let row = row.map_err(|e| format!("Invalid CSV row {}: {e}", line + 2))?;
            let prefix = row.get(assignment_col).unwrap_or("").trim();
            let registry = row.get(registry_col).unwrap_or("").trim();
            let vendor = row.get(vendor_col).unwrap_or("").trim();
            if prefix.is_empty() && vendor.is_empty() {
                continue;
            }
            let digits: String = prefix.chars().filter(|c| *c != ':' && *c != '-').collect();
            let row_kind = RegistryKind::from_assignment_length(digits.len())
                .ok_or_else(|| format!("Invalid IEEE assignment at row {}", line + 2))?;
            if !digits.bytes().all(|b| b.is_ascii_hexdigit())
                || !registry.eq_ignore_ascii_case(row_kind.label())
                || vendor.is_empty()
                || vendor.len() > 512
            {
                return Err(format!(
                    "Invalid IEEE assignment/registry/vendor at row {}",
                    line + 2
                ));
            }
            if kind.is_some_and(|existing| existing != row_kind) {
                return Err("One IEEE CSV file must contain only one registry class".to_owned());
            }
            kind = Some(row_kind);
            let key = u64::from_str_radix(&digits, 16)
                .map_err(|e| format!("Invalid IEEE prefix at row {}: {e}", line + 2))?;
            entries.insert(key, vendor.to_owned());
        }
        let kind = kind.ok_or("No valid IEEE assignments in CSV")?;
        let count = entries.len();
        match kind {
            RegistryKind::MaL => self.ma_l = entries,
            RegistryKind::MaM => self.ma_m = entries,
            RegistryKind::MaS => self.ma_s = entries,
        }
        Ok((kind, count))
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_longest_prefix_and_quoted_names() {
        let mut db = OuiRegistry::default();
        assert_eq!(db.load_csv(b"Registry,Assignment,Organization Name,Organization Address\nMA-L,001B21,\"Example, Systems\",Somewhere\n").unwrap(), (RegistryKind::MaL, 1));
        assert_eq!(
            db.load_csv(b"Registry,Assignment,Organization Name\nMA-M,001B21A,Medium Inc\n")
                .unwrap(),
            (RegistryKind::MaM, 1)
        );
        assert_eq!(
            db.load_csv(b"Registry,Assignment,Organization Name\nMA-S,001B21A12,Specific Co\n")
                .unwrap(),
            (RegistryKind::MaS, 1)
        );
        assert_eq!(
            db.lookup("00:1B:21:A1:2F:34").as_deref(),
            Some("Specific Co")
        );
        assert_eq!(
            db.lookup("00:1B:21:A2:00:00").as_deref(),
            Some("Medium Inc")
        );
        assert_eq!(
            db.lookup("00:1B:21:FF:00:00").as_deref(),
            Some("Example, Systems")
        );
        assert!(db.lookup("02:1B:21:A1:2F:34").is_none());
    }

    #[test]
    fn failed_import_preserves_previous_registry() {
        let mut db = OuiRegistry::default();
        db.load_csv(b"Registry,Assignment,Organization Name\nMA-L,001B21,Test Vendor\n")
            .unwrap();
        assert!(
            db.load_csv(b"Registry,Assignment,Organization Name\nMA-L,BAD,Broken\n")
                .is_err()
        );
        assert_eq!(db.counts(), (1, 0, 0));
        assert_eq!(
            db.lookup("00:1B:21:00:00:01").as_deref(),
            Some("Test Vendor")
        );
        db.reset();
        assert_eq!(db.counts(), (0, 0, 0));
    }
}
