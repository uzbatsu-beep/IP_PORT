//! Offline IEEE OUI cache. All CSV parsing and persistence happens off the UI
//! thread. Imported data is copied into an application-owned directory, not
//! loaded from a potentially removable original location on every startup.
use crate::oui_registry::{OuiRegistry, RegistryKind};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_CSV_BYTES: u64 = 16 * 1024 * 1024;
const KINDS: [RegistryKind; 3] = [RegistryKind::MaL, RegistryKind::MaM, RegistryKind::MaS];

pub struct ImportedOui {
    pub kind: RegistryKind,
    pub count: usize,
    pub registry: OuiRegistry,
}

pub struct RestoredOui {
    pub registry: OuiRegistry,
    pub loaded: usize,
    pub warnings: Vec<String>,
}

/// The cache contains IEEE public vendor data, never scanned hosts or IPs.
pub fn default_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))
            .map(|root| PathBuf::from(root).join("IP_PORT").join("oui"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|root| {
            PathBuf::from(root)
                .join("Library")
                .join("Application Support")
                .join("IP_PORT")
                .join("oui")
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|root| PathBuf::from(root).join(".local").join("share"))
            })
            .map(|root| root.join("ip_port").join("oui"))
    }
}

fn slug(kind: RegistryKind) -> &'static str {
    match kind {
        RegistryKind::MaL => "ma-l",
        RegistryKind::MaM => "ma-m",
        RegistryKind::MaS => "ma-s",
    }
}

fn paths(dir: &Path, kind: RegistryKind) -> (PathBuf, PathBuf, PathBuf) {
    let name = slug(kind);
    (
        dir.join(format!("{name}.csv")),
        dir.join(format!("{name}.new")),
        dir.join(format!("{name}.bak")),
    )
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path)
        .map_err(|error| format!("Cannot open IEEE CSV {}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_CSV_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read IEEE CSV {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_CSV_BYTES {
        return Err(format!("IEEE CSV {} exceeds 16 MiB", path.display()));
    }
    Ok(bytes)
}

fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Cannot delete {}: {error}", path.display())),
    }
}

/// Recover when a process stopped between renaming the old cache to .bak and
/// promoting the validated .new file. Never prefer uncommitted .new data.
fn recover_cache(dir: &Path, kind: RegistryKind) -> Result<(), String> {
    let (target, _pending, backup) = paths(dir, kind);
    if !target.exists() && backup.exists() {
        fs::rename(&backup, &target)
            .map_err(|error| format!("Cannot restore {}: {error}", target.display()))?;
    }
    Ok(())
}

fn persist_cache(dir: &Path, kind: RegistryKind, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|error| format!("Cannot create IEEE cache {}: {error}", dir.display()))?;
    recover_cache(dir, kind)?;
    let (target, pending, backup) = paths(dir, kind);
    remove_if_exists(&pending)?;
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(|error| format!("Cannot stage IEEE cache: {error}"))?;
        if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
            let _ = remove_if_exists(&pending);
            return Err(format!("Cannot write IEEE cache: {error}"));
        }
    }
    let had_previous = target.exists();
    if had_previous {
        remove_if_exists(&backup)?;
        if let Err(error) = fs::rename(&target, &backup) {
            let _ = remove_if_exists(&pending);
            return Err(format!("Cannot back up previous IEEE cache: {error}"));
        }
    }
    if let Err(error) = fs::rename(&pending, &target) {
        let restore = if had_previous {
            fs::rename(&backup, &target).err()
        } else {
            None
        };
        let _ = remove_if_exists(&pending);
        return Err(format!(
            "Cannot activate IEEE cache: {error}; rollback: {}",
            restore.map_or_else(|| "completed".to_owned(), |err| err.to_string())
        ));
    }
    // A leftover backup is harmless if cleanup is interrupted: the new target
    // takes precedence on the next startup.
    let _ = remove_if_exists(&backup);
    Ok(())
}

pub fn import_and_cache(source: &Path, dir: &Path) -> Result<ImportedOui, String> {
    let bytes = read_bounded(source)?;
    let mut registry = OuiRegistry::default();
    let (kind, count) = registry.load_csv(&bytes)?;
    // Commit only validated input; never overwrite the active cache on errors.
    persist_cache(dir, kind, &bytes)?;
    Ok(ImportedOui { kind, count, registry })
}

pub fn restore(dir: &Path) -> RestoredOui {
    let mut registry = OuiRegistry::default();
    let mut loaded = 0;
    let mut warnings = Vec::new();
    for kind in KINDS {
        if let Err(error) = recover_cache(dir, kind) {
            warnings.push(error);
            continue;
        }
        let (target, _, _) = paths(dir, kind);
        if !target.exists() {
            continue;
        }
        let restored = read_bounded(&target).and_then(|bytes| {
            let mut one = OuiRegistry::default();
            let (actual_kind, count) = one.load_csv(&bytes)?;
            if actual_kind != kind {
                return Err(format!(
                    "Cached {} contains {} assignments; ignored",
                    kind.label(),
                    actual_kind.label()
                ));
            }
            Ok((one, count))
        });
        match restored {
            Ok((one, count)) => {
                registry.replace_kind_from(kind, one);
                loaded += count;
            }
            Err(error) => warnings.push(error),
        }
    }
    RestoredOui { registry, loaded, warnings }
}

pub fn clear_cache(dir: &Path) -> Result<(), String> {
    for kind in KINDS {
        let (target, pending, backup) = paths(dir, kind);
        remove_if_exists(&target)?;
        remove_if_exists(&pending)?;
        remove_if_exists(&backup)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn isolated_dir() -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ip_port_oui_cache_test_{}_{}", std::process::id(), stamp
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn imports_and_restores_multiple_ieee_registry_classes() {
        let dir = isolated_dir();
        let source = dir.join("incoming.csv");
        fs::write(&source, b"Registry,Assignment,Organization Name\nMA-L,001B21,Large Vendor\n").unwrap();
        let imported = import_and_cache(&source, &dir).unwrap();
        assert_eq!(imported.kind, RegistryKind::MaL);
        assert_eq!(imported.count, 1);
        fs::write(&source, b"Registry,Assignment,Organization Name\nMA-S,001B21A12,Small Vendor\n").unwrap();
        import_and_cache(&source, &dir).unwrap();
        fs::remove_file(&source).unwrap();

        let restored = restore(&dir);
        assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
        assert_eq!(restored.loaded, 2);
        assert_eq!(restored.registry.lookup("00:1B:21:A1:20:01").as_deref(), Some("Small Vendor"));
        assert_eq!(restored.registry.lookup("00:1B:21:FF:00:01").as_deref(), Some("Large Vendor"));
        clear_cache(&dir).unwrap();
        assert_eq!(restore(&dir).loaded, 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn malformed_reimport_leaves_existing_disk_cache_unchanged() {
        let dir = isolated_dir();
        let source = dir.join("incoming.csv");
        fs::write(&source, b"Registry,Assignment,Organization Name\nMA-L,001B21,Working Vendor\n").unwrap();
        import_and_cache(&source, &dir).unwrap();
        fs::write(&source, b"Registry,Assignment,Organization Name\nMA-L,BAD,Broken\n").unwrap();
        assert!(import_and_cache(&source, &dir).is_err());
        let restored = restore(&dir);
        assert_eq!(restored.loaded, 1);
        assert_eq!(restored.registry.lookup("00:1B:21:11:22:33").as_deref(), Some("Working Vendor"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn recovers_committed_cache_from_backup_after_interrupted_replace() {
        let dir = isolated_dir();
        let (target, _pending, backup) = paths(&dir, RegistryKind::MaL);
        fs::write(&backup, b"Registry,Assignment,Organization Name\nMA-L,001B21,Recovered\n").unwrap();
        let restored = restore(&dir);
        assert_eq!(restored.loaded, 1);
        assert!(target.exists());
        assert_eq!(restored.registry.lookup("00:1B:21:00:00:01").as_deref(), Some("Recovered"));
        fs::remove_dir_all(dir).unwrap();
    }
}
