use crate::config::Config;
use crate::note::{self, Meta, Note};
use crate::vcs::VersionControl;
use anyhow::Result;
use chrono::{DateTime, FixedOffset, Local};
use std::collections::BTreeMap;

/// Capture note content (from stdin and/or the editor) and store it.
pub fn run(
    vcs: &dyn VersionControl,
    config: &Config,
    no_edit: bool,
    title: Option<&str>,
    labels: &[String],
) -> Result<()> {
    let input = crate::io::read_stdin();
    let content = if no_edit {
        input.unwrap_or_default()
    } else {
        crate::editor::get_content_from_editor(input)?
    };

    let now = Local::now().fixed_offset();
    save_note(vcs, config, &content, title, labels, now)?;
    Ok(())
}

pub(crate) fn save_note(
    vcs: &dyn VersionControl,
    config: &Config,
    content: &str,
    title: Option<&str>,
    labels: &[String],
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    match build_note(content, config, title, labels, now)? {
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
    title: Option<&str>,
    labels: &[String],
    now: DateTime<FixedOffset>,
) -> Result<Option<(String, String)>> {
    let content = content.trim();
    if content.is_empty() {
        return Ok(None);
    }

    let title = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| note::title_from_content(content));
    let template = config
        .note
        .filename
        .as_deref()
        .unwrap_or(note::DEFAULT_FILENAME);
    let path = note::note_path(template, &title, labels, &config.note.meta, now)?;

    let note = assemble_note(path.clone(), title, content, config, labels, now);
    let raw = note::to_raw(&note)?;
    Ok(Some((path, raw)))
}

/// Assemble a `Note` at an explicit `path` with an already-resolved `title`.
/// Merges config static meta (skipping reserved keys), cleans `labels`, sets
/// both `created` and `updated` to `now`, and appends a trailing newline to the
/// (already-trimmed) `content`.
pub(crate) fn assemble_note(
    path: String,
    title: String,
    content: &str,
    config: &Config,
    labels: &[String],
    now: DateTime<FixedOffset>,
) -> Note {
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
        path,
        labels: labels
            .iter()
            .map(|label| label.trim())
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .collect(),
        created: now,
        updated: now,
        extra,
    };
    Note {
        meta,
        content: format!("{content}\n"),
    }
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
        assert!(
            build_note("   \n", &config, None, &[], now())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn build_note_produces_path_and_frontmatter() {
        let config = Config::default();
        let (path, raw) = build_note("# My new note\n\nHello, World!", &config, None, &[], now())
            .unwrap()
            .unwrap();
        assert_eq!(path, "2026/06/02/10:00:00-my-new-note.md");
        let note = parse_note(&raw).unwrap();
        assert_eq!(note.meta.title, "My new note");
        assert_eq!(note.content, "# My new note\n\nHello, World!\n");
    }

    #[test]
    fn build_note_uses_custom_title_over_content() {
        let config = Config::default();
        let (path, raw) = build_note(
            "# Content Heading\n\nbody",
            &config,
            Some("Custom Title"),
            &[],
            now(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(path, "2026/06/02/10:00:00-custom-title.md");
        let note = parse_note(&raw).unwrap();
        assert_eq!(note.meta.title, "Custom Title");
    }

    #[test]
    fn build_note_falls_back_to_content_title_when_none() {
        let config = Config::default();
        let (_, raw) = build_note("# Real Title\n\nbody", &config, None, &[], now())
            .unwrap()
            .unwrap();
        let note = parse_note(&raw).unwrap();
        assert_eq!(note.meta.title, "Real Title");
    }

    #[test]
    fn build_note_falls_back_when_title_is_blank() {
        let config = Config::default();
        let (_, raw) = build_note("# Real Title\n\nbody", &config, Some("   "), &[], now())
            .unwrap()
            .unwrap();
        let note = parse_note(&raw).unwrap();
        assert_eq!(note.meta.title, "Real Title");
    }

    #[test]
    fn build_note_sets_labels_from_arguments() {
        let config = Config::default();
        let labels = vec!["work".to_string(), "urgent".to_string()];
        let (_, raw) = build_note("body", &config, None, &labels, now())
            .unwrap()
            .unwrap();
        let note = parse_note(&raw).unwrap();
        assert_eq!(
            note.meta.labels,
            vec!["work".to_string(), "urgent".to_string()]
        );
    }

    #[test]
    fn build_note_trims_and_drops_blank_labels() {
        let config = Config::default();
        let labels = vec![
            "  work  ".to_string(),
            "".to_string(),
            "   ".to_string(),
            "urgent".to_string(),
        ];
        let (_, raw) = build_note("body", &config, None, &labels, now())
            .unwrap()
            .unwrap();
        let note = parse_note(&raw).unwrap();
        assert_eq!(
            note.meta.labels,
            vec!["work".to_string(), "urgent".to_string()]
        );
    }

    #[test]
    fn save_note_writes_to_backend() {
        let config = Config::default();
        let backend = MemoryBackend::new();
        let path = save_note(&backend, &config, "Hello", None, &[], now())
            .unwrap()
            .unwrap();
        let raw = backend.read_file(&path).unwrap();
        assert!(raw.contains("Hello"));
    }

    #[test]
    fn save_note_skips_empty_content() {
        let config = Config::default();
        let backend = MemoryBackend::new();
        assert!(
            save_note(&backend, &config, "  ", None, &[], now())
                .unwrap()
                .is_none()
        );
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

        let (_, raw) = build_note("# Real Title\n\nbody", &config, None, &[], now())
            .unwrap()
            .unwrap();
        let note = crate::note::parse_note(&raw).unwrap();

        assert_eq!(note.meta.title, "Real Title");
        assert_eq!(
            note.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
        assert!(!note.meta.extra.contains_key("title"));
    }

    #[test]
    fn assemble_note_builds_at_explicit_path_with_given_title() {
        let mut config = Config::default();
        config.note.meta.insert(
            "author".to_string(),
            toml::Value::String("Paul".to_string()),
        );
        let note = assemble_note(
            "2026/07/03.md".to_string(),
            "Daily note for 2026-07-03".to_string(),
            "Body text",
            &config,
            &["work".to_string()],
            now(),
        );
        assert_eq!(note.meta.path, "2026/07/03.md");
        assert_eq!(note.meta.title, "Daily note for 2026-07-03");
        assert_eq!(note.content, "Body text\n");
        assert_eq!(note.meta.labels, vec!["work".to_string()]);
        assert_eq!(note.meta.created, now());
        assert_eq!(note.meta.updated, now());
        assert_eq!(
            note.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
    }
}
