use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultView {
    #[default]
    Grid,
    Preview,
}

impl DefaultView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Grid => "Photos grid",
            Self::Preview => "Preview",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct Settings {
    pub default_folder: String,
    pub default_view: DefaultView,
}

pub fn path() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|directory| directory.join("imgvw").join("settings.json"))
        .ok_or_else(|| "Cannot locate the user's settings directory.".into())
}

pub fn load(path: &Path) -> Result<Settings, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Settings::default()),
        Err(error) => {
            return Err(format!(
                "Cannot read settings at {}: {error}",
                path.display()
            ));
        }
    };
    serde_json::from_reader(file)
        .map_err(|error| format!("Invalid settings at {}: {error}", path.display()))
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if !settings.default_folder.is_empty() && !Path::new(&settings.default_folder).is_dir() {
        return Err("Default folder location must be an existing folder, or left empty.".into());
    }
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "The settings file must have a parent directory.".to_string())?;
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(directory)?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer_pretty(&mut temporary, settings)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary.persist(path)?;
        Ok(())
    };
    write().map_err(|error| format!("Cannot save settings at {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_use_defaults_and_saved_settings_can_be_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("imgvw").join("settings.json");
        assert_eq!(load(&path).unwrap(), Settings::default());
        let mut settings = Settings {
            default_folder: directory.path().to_str().unwrap().into(),
            default_view: DefaultView::Preview,
        };
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        settings.default_view = DefaultView::Grid;
        settings.default_folder.clear();
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).unwrap(), settings);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn invalid_settings_and_write_failures_are_reported() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, b"invalid json").unwrap();
        assert!(load(&path).unwrap_err().contains("Invalid settings"));
        fs::write(&path, br#"{"default_view":"unknown"}"#).unwrap();
        assert!(load(&path).is_err());
        assert!(load(directory.path()).is_err());
        let settings = Settings {
            default_folder: directory.path().join("missing").display().to_string(),
            ..Settings::default()
        };
        assert!(
            save(&path, &settings)
                .unwrap_err()
                .contains("existing folder")
        );
        assert!(save(&path.join("settings.json"), &Settings::default()).is_err());
        assert!(save(directory.path(), &Settings::default()).is_err());
        assert_eq!(fs::read(&path).unwrap(), br#"{"default_view":"unknown"}"#);
    }
}
