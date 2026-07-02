use crate::config::Config;
use crate::note::{self, Meta, Note};
use crate::vcs::VersionControl;
use anyhow::Result;
use chrono::{DateTime, FixedOffset, Local};
use std::collections::BTreeMap;

/// Capture note content (from stdin and/or the editor) and store it.
pub fn run(vcs: &dyn VersionControl, config: &Config, no_edit: bool) -> Result<()> {
    let input = crate::io::read_stdin();
    let content = if no_edit {
        input.unwrap_or_default()
    } else {
        crate::editor::get_content_from_editor(input)?
    };

    let now = Local::now().fixed_offset();
    save_note(vcs, config, &content, now)?;
    Ok(())
}

pub(crate) fn save_note(
    vcs: &dyn VersionControl,
    config: &Config,
    content: &str,
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    match build_note(content, config, now) {
        None => {
            eprintln!("Skipping empty note.");
            Ok(None)
        }
        Some((path, raw)) => {
            vcs.write_file(&path, &raw, &format!("Add note {path}"))?;
            println!("{path}");
            Ok(Some(path))
        }
    }
}

const RESERVED_META_KEYS: [&str; 5] = ["title", "path", "labels", "created", "updated"];

pub(crate) fn build_note(
    content: &str,
    config: &Config,
    now: DateTime<FixedOffset>,
) -> Option<(String, String)> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }

    let title = note::title_from_content(content);
    let template = config
        .note
        .filename
        .as_deref()
        .unwrap_or(note::DEFAULT_FILENAME);
    let path = note::note_path(template, &title, now);

    let mut extra = BTreeMap::new();
    for (key, value) in &config.note.meta {
        if RESERVED_META_KEYS.contains(&key.as_str()) {
            continue;
        }
        if let Ok(value) = serde_yaml_ng::to_value(value) {
            extra.insert(key.clone(), value);
        }
    }

    let meta = Meta {
        title,
        path: path.clone(),
        labels: Vec::new(),
        created: now,
        updated: now,
        extra,
    };
    let full_note = Note {
        meta,
        content: format!("{content}\n"),
    };
    let raw = note::to_raw(&full_note).ok()?;
    Some((path, raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::note::parse_note;
    use crate::vcs::MemoryBackend;
    use crate::vcs::VersionControl;
    use chrono::DateTime;

    fn now() -> DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339("2026-06-02T10:00:00+01:00").unwrap()
    }

    #[test]
    fn build_note_returns_none_for_empty_content() {
        let config = Config::default();
        assert!(build_note("   \n", &config, now()).is_none());
    }

    #[test]
    fn build_note_produces_path_and_frontmatter() {
        let config = Config::default();
        let (path, raw) = build_note("# My new note\n\nHello, World!", &config, now()).unwrap();
        assert_eq!(path, "2026/06/02/10:00:00-my-new-note.md");
        let note = parse_note(&raw).unwrap();
        assert_eq!(note.meta.title, "My new note");
        assert_eq!(note.content, "# My new note\n\nHello, World!\n");
    }

    #[test]
    fn save_note_writes_to_backend() {
        let config = Config::default();
        let backend = MemoryBackend::new();
        let path = save_note(&backend, &config, "Hello", now())
            .unwrap()
            .unwrap();
        let raw = backend.read_file(&path).unwrap();
        assert!(raw.contains("Hello"));
    }

    #[test]
    fn save_note_skips_empty_content() {
        let config = Config::default();
        let backend = MemoryBackend::new();
        assert!(save_note(&backend, &config, "  ", now()).unwrap().is_none());
        assert!(backend.list_files().unwrap().is_empty());
    }

    #[test]
    fn build_note_merges_static_meta_but_ignores_reserved_keys() {
        let mut config = Config::default();
        config.note.meta.insert(
            "author".to_string(),
            toml::Value::String("Paul".to_string()),
        );
        config.note.meta.insert(
            "title".to_string(),
            toml::Value::String("hacked".to_string()),
        );

        let (_, raw) = build_note("# Real Title\n\nbody", &config, now()).unwrap();
        let note = crate::note::parse_note(&raw).unwrap();

        assert_eq!(note.meta.title, "Real Title");
        assert_eq!(
            note.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
        assert!(!note.meta.extra.contains_key("title"));
    }
}
