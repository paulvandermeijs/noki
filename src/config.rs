use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const LOCAL_CONFIG_NAME: &str = ".noki.toml";
const DEFAULT_MAX_VISIBLE_LABELS: usize = 3;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub repository: Option<String>,
    pub note: NoteConfig,
    pub list: ListConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct NoteConfig {
    pub filename: Option<String>,
    pub daily_filename: Option<String>,
    pub daily_title: Option<String>,
    pub daily_label: Option<String>,
    pub meta: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListConfig {
    pub max_visible_labels: Option<usize>,
}

impl Config {
    /// The resolved repository, or an error if none was configured.
    pub fn repository(&self) -> Result<&str> {
        self.repository
            .as_deref()
            .context("No repository configured. Set one with --repository or in .noki.toml.")
    }

    /// The maximum number of labels to show per note in the list.
    pub fn max_visible_labels(&self) -> usize {
        self.list
            .max_visible_labels
            .unwrap_or(DEFAULT_MAX_VISIBLE_LABELS)
    }
}

/// Load configuration from the global config file, any `.noki.toml` files from
/// the current directory up to the filesystem root, and a CLI override.
pub fn load(repository_override: Option<String>) -> Result<Config> {
    let global = global_config_path();
    let start = std::env::current_dir()?;
    load_from(global.as_deref(), &start, repository_override)
}

pub(crate) fn load_from(
    global: Option<&Path>,
    start: &Path,
    repository_override: Option<String>,
) -> Result<Config> {
    let mut config = match global {
        Some(path) if path.exists() => read_config(path)?,
        _ => Config::default(),
    };

    for path in local_config_paths(start) {
        config.merge(read_config(&path)?);
    }

    if repository_override.is_some() {
        config.repository = repository_override;
    }

    Ok(config)
}

fn global_config_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.config_dir().join("noki").join("config.toml"))
}

/// `.noki.toml` files from the filesystem root down to `start` (nearest last).
fn local_config_paths(start: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<&Path> = start.ancestors().collect();
    dirs.reverse();
    dirs.into_iter()
        .map(|dir| dir.join(LOCAL_CONFIG_NAME))
        .filter(|path| path.exists())
        .collect()
}

fn read_config(path: &Path) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("Invalid config file {}", path.display()))
}

impl Config {
    fn merge(&mut self, other: Config) {
        if other.repository.is_some() {
            self.repository = other.repository;
        }
        if other.note.filename.is_some() {
            self.note.filename = other.note.filename;
        }
        if other.note.daily_filename.is_some() {
            self.note.daily_filename = other.note.daily_filename;
        }
        if other.note.daily_title.is_some() {
            self.note.daily_title = other.note.daily_title;
        }
        if other.note.daily_label.is_some() {
            self.note.daily_label = other.note.daily_label;
        }
        for (key, value) in other.note.meta {
            self.note.meta.insert(key, value);
        }
        if other.list.max_visible_labels.is_some() {
            self.list.max_visible_labels = other.list.max_visible_labels;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cli_override_wins_over_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".noki.toml"),
            "repository = \"from-local\"\n",
        )
        .unwrap();
        let config = load_from(None, dir.path(), Some("from-cli".to_string())).unwrap();
        assert_eq!(config.repository().unwrap(), "from-cli");
    }

    #[test]
    fn nearest_local_file_wins_over_ancestor() {
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            root.path().join(".noki.toml"),
            "repository = \"ancestor\"\n",
        )
        .unwrap();
        fs::write(child.join(".noki.toml"), "repository = \"nearest\"\n").unwrap();
        let config = load_from(None, &child, None).unwrap();
        assert_eq!(config.repository().unwrap(), "nearest");
    }

    #[test]
    fn parses_note_section() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".noki.toml"),
            "repository = \"r\"\n\n[note]\nfilename = \"%Y-%title\"\nmeta = { author = \"Paul\" }\n",
        )
        .unwrap();
        let config = load_from(None, dir.path(), None).unwrap();
        assert_eq!(config.note.filename.as_deref(), Some("%Y-%title"));
        assert_eq!(
            config.note.meta.get("author").unwrap().as_str(),
            Some("Paul")
        );
    }

    #[test]
    fn missing_repository_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_from(None, dir.path(), None).unwrap();
        assert!(config.repository().is_err());
    }

    #[test]
    fn parses_list_section() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".noki.toml"),
            "repository = \"r\"\n\n[list]\nmax_visible_labels = 5\n",
        )
        .unwrap();
        let config = load_from(None, dir.path(), None).unwrap();
        assert_eq!(config.max_visible_labels(), 5);
    }

    #[test]
    fn max_visible_labels_defaults_to_three() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_from(None, dir.path(), None).unwrap();
        assert_eq!(config.max_visible_labels(), 3);
    }

    #[test]
    fn parses_daily_filename() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".noki.toml"),
            "repository = \"r\"\n\n[note]\ndaily_filename = \"journal/%Y-%m-%d\"\n",
        )
        .unwrap();
        let config = load_from(None, dir.path(), None).unwrap();
        assert_eq!(
            config.note.daily_filename.as_deref(),
            Some("journal/%Y-%m-%d")
        );
    }

    #[test]
    fn parses_daily_title_and_label() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".noki.toml"),
            "repository = \"r\"\n\n[note]\ndaily_title = \"Journal for %Y-%m-%d\"\ndaily_label = \"journal\"\n",
        )
        .unwrap();
        let config = load_from(None, dir.path(), None).unwrap();
        assert_eq!(
            config.note.daily_title.as_deref(),
            Some("Journal for %Y-%m-%d")
        );
        assert_eq!(config.note.daily_label.as_deref(), Some("journal"));
    }
}
