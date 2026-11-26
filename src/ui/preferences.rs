//! Independent language preference; appearance.json remains compatible.
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    English,
    Russian,
}

impl Language {
    pub fn text(self, english: &'static str, russian: &'static str) -> &'static str {
        match self {
            Self::English => english,
            Self::Russian => russian,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Settings {
    schema_version: u32,
    language: Language,
}

pub fn path() -> Option<PathBuf> {
    Some(super::theme::settings_path()?.with_file_name("workspace-language.json"))
}

pub fn load(path: &Path) -> io::Result<Language> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Language::default()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Language file too large"));
    }
    let settings: Settings = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if settings.schema_version != 1 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Unsupported language schema"));
    }
    Ok(settings.language)
}

pub fn save(path: &Path, language: Language) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::other("Missing settings directory"))?;
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(&Settings { schema_version: 1, language })
        .map_err(io::Error::other)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let pending = dir.join(format!(".language-{}-{nonce}.tmp", std::process::id()));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&pending)?;
    let result = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = result.and_then(|_| fs::rename(&pending, path));
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_round_trip_and_invalid_file() {
        let dir = std::env::temp_dir().join(format!(
            "ip_port_language_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let path = dir.join("language.json");
        assert_eq!(load(&path).unwrap(), Language::English);
        save(&path, Language::Russian).unwrap();
        assert_eq!(load(&path).unwrap(), Language::Russian);
        save(&path, Language::English).unwrap();
        assert_eq!(load(&path).unwrap(), Language::English);
        fs::write(&path, b"invalid").unwrap();
        assert!(load(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
