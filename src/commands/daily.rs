use crate::commands::{create, edit};
use crate::config::Config;
use crate::note::{self, Note};
use crate::vcs::VersionControl;
use anyhow::Result;
use chrono::{DateTime, FixedOffset, Local};

/// Open (or create) today's daily note. The note's path comes from
/// `note.daily_filename` (default `%Y/%m/%d`). An existing note is loaded and
/// updated (its `created`/title/labels preserved); a missing one is created.
/// Piped input is appended to an existing note and used as the body of a new one.
pub fn run(
    vcs: &dyn VersionControl,
    config: &Config,
    no_edit: bool,
    title: Option<&str>,
    labels: &[String],
) -> Result<()> {
    let now = Local::now().fixed_offset();
    let path = daily_path(config, now);
    let input = crate::io::read_stdin();

    if let Some(note) = load_existing(vcs, &path)? {
        if no_edit {
            match input {
                Some(piped) => {
                    append_and_save(vcs, note, &piped, now)?;
                }
                None => eprintln!("Nothing to add to today's note."),
            }
        } else {
            let prefill = match &input {
                Some(piped) => append_body(&note.content, piped),
                None => note.content.clone(),
            };
            let body = crate::editor::get_content_from_editor(Some(prefill))?;
            edit::save_edit(vcs, note, &body, now)?;
        }
        return Ok(());
    }

    let content = if no_edit {
        input.unwrap_or_default()
    } else {
        crate::editor::get_content_from_editor(input)?
    };
    save_new_daily(vcs, config, &path, &content, title, labels, now)?;
    Ok(())
}

/// Today's daily-note path from `note.daily_filename`. The daily template
/// carries no `%title`, so the title argument to `note_path` is unused.
fn daily_path(config: &Config, now: DateTime<FixedOffset>) -> String {
    let template = config
        .note
        .daily_filename
        .as_deref()
        .unwrap_or(note::DEFAULT_DAILY_FILENAME);
    note::note_path(template, "", now)
}

/// Load and parse the note at `path`, or `None` if there is none there.
fn load_existing(vcs: &dyn VersionControl, path: &str) -> Result<Option<Note>> {
    match vcs.read_file(path) {
        Ok(raw) => Ok(Some(note::parse_note(&raw)?)),
        Err(_) => Ok(None),
    }
}

/// Append `addition` below `existing`, separated by a blank line.
fn append_body(existing: &str, addition: &str) -> String {
    format!("{}\n\n{}", existing.trim_end(), addition.trim())
}

/// Append `addition` to an existing daily note and save it (refreshing
/// `updated`, preserving `created`/title/labels). Returns the note's path.
fn append_and_save(
    vcs: &dyn VersionControl,
    note: Note,
    addition: &str,
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    let body = append_body(&note.content, addition);
    edit::save_edit(vcs, note, &body, now)
}

/// Build and store a brand-new daily note at `path`, committing it as an "Add".
/// When `title` is absent, defaults to `Daily note for %Y-%m-%d`. Returns the
/// path, or `None` when the content is empty after trimming.
fn save_new_daily(
    vcs: &dyn VersionControl,
    config: &Config,
    path: &str,
    content: &str,
    title: Option<&str>,
    labels: &[String],
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    let content = content.trim();
    if content.is_empty() {
        eprintln!("Skipping empty note.");
        return Ok(None);
    }

    let title = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_daily_title(now));

    let note = create::assemble_note(path.to_string(), title, content, config, labels, now);
    let raw = note::to_raw(&note)?;
    vcs.write_file(path, &raw, &format!("Add note {path}"))?;
    println!("{path}");
    Ok(Some(path.to_string()))
}

/// The default title for a new daily note: `Daily note for %Y-%m-%d`.
fn default_daily_title(now: DateTime<FixedOffset>) -> String {
    format!("Daily note for {}", now.format("%Y-%m-%d"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::note::parse_note;
    use crate::vcs::{MemoryBackend, VersionControl};
    use chrono::DateTime;

    fn at(s: &str) -> DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn now() -> DateTime<chrono::FixedOffset> {
        at("2026-07-03T09:00:00+02:00")
    }

    const EXISTING: &str = "---\ntitle: Daily note for 2026-07-03\npath: 2026/07/03.md\nlabels: []\ncreated: 2026-07-03T08:00:00+02:00\nupdated: 2026-07-03T08:00:00+02:00\n---\n\nMorning notes\n";

    #[test]
    fn daily_path_uses_default_template() {
        let config = Config::default();
        assert_eq!(daily_path(&config, now()), "2026/07/03.md");
    }

    #[test]
    fn daily_path_uses_configured_template() {
        let mut config = Config::default();
        config.note.daily_filename = Some("journal/%Y-%m-%d".to_string());
        assert_eq!(daily_path(&config, now()), "journal/2026-07-03.md");
    }

    #[test]
    fn default_daily_title_formats_the_date() {
        assert_eq!(default_daily_title(now()), "Daily note for 2026-07-03");
    }

    #[test]
    fn append_body_separates_with_a_blank_line() {
        assert_eq!(
            append_body("Morning notes\n", "did X"),
            "Morning notes\n\ndid X"
        );
    }

    #[test]
    fn load_existing_returns_none_when_missing() {
        let backend = MemoryBackend::new();
        assert!(load_existing(&backend, "2026/07/03.md").unwrap().is_none());
    }

    #[test]
    fn load_existing_parses_present_note() {
        let backend = MemoryBackend::with_files(&[("2026/07/03.md", EXISTING)]);
        let note = load_existing(&backend, "2026/07/03.md").unwrap().unwrap();
        assert_eq!(note.meta.title, "Daily note for 2026-07-03");
        assert_eq!(note.content, "Morning notes\n");
    }

    #[test]
    fn save_new_daily_creates_note_with_date_title() {
        let backend = MemoryBackend::new();
        let config = Config::default();
        let path = save_new_daily(
            &backend,
            &config,
            "2026/07/03.md",
            "Hello",
            None,
            &[],
            now(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(path, "2026/07/03.md");
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.meta.title, "Daily note for 2026-07-03");
        assert_eq!(saved.meta.created, now());
        assert_eq!(saved.meta.updated, now());
        assert_eq!(saved.content, "Hello\n");
    }

    #[test]
    fn save_new_daily_uses_explicit_title() {
        let backend = MemoryBackend::new();
        let config = Config::default();
        save_new_daily(
            &backend,
            &config,
            "2026/07/03.md",
            "Hello",
            Some("Standup"),
            &[],
            now(),
        )
        .unwrap()
        .unwrap();
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.meta.title, "Standup");
    }

    #[test]
    fn save_new_daily_skips_empty_content() {
        let backend = MemoryBackend::new();
        let config = Config::default();
        assert!(
            save_new_daily(
                &backend,
                &config,
                "2026/07/03.md",
                "   \n",
                None,
                &[],
                now()
            )
            .unwrap()
            .is_none()
        );
        assert!(backend.list_files().unwrap().is_empty());
    }

    #[test]
    fn save_new_daily_applies_labels_and_config_meta() {
        let backend = MemoryBackend::new();
        let mut config = Config::default();
        config.note.meta.insert(
            "author".to_string(),
            toml::Value::String("Paul".to_string()),
        );
        save_new_daily(
            &backend,
            &config,
            "2026/07/03.md",
            "Hello",
            None,
            &["work".to_string()],
            now(),
        )
        .unwrap()
        .unwrap();
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.meta.labels, vec!["work".to_string()]);
        assert_eq!(
            saved.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
    }

    #[test]
    fn append_and_save_appends_and_bumps_updated() {
        let backend = MemoryBackend::with_files(&[("2026/07/03.md", EXISTING)]);
        let note = parse_note(EXISTING).unwrap();
        let path = append_and_save(&backend, note, "did X", now())
            .unwrap()
            .unwrap();
        assert_eq!(path, "2026/07/03.md");
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.content, "Morning notes\n\ndid X\n");
        assert_eq!(saved.meta.updated, now());
        assert_eq!(saved.meta.created, at("2026-07-03T08:00:00+02:00"));
        assert_eq!(saved.meta.title, "Daily note for 2026-07-03");
    }
}
