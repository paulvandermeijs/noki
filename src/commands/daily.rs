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
    let path = daily_path(config, now)?;
    let input = crate::io::read_stdin();

    if let Some(note) = load_existing(vcs, &path)? {
        if no_edit {
            match input {
                Some(piped) => {
                    append_and_save(vcs, config, note, &piped, now)?;
                }
                None => eprintln!("Nothing to add to today's note."),
            }
        } else {
            let prefill = match &input {
                Some(piped) => append_body(&note.content, piped),
                None => note.content.clone(),
            };
            let body = crate::editor::get_content_from_editor(Some(prefill))?;
            save_update(vcs, config, note, &body, now)?;
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

/// Today's daily-note path from `note.daily_filename` (default `{created:%Y/%m/%d}`).
/// `{title}` and `{labels}` resolve to the daily note's own title and label —
/// both derived from config + date, so the path stays stable per day regardless
/// of any `--title`/`--label` given (which would otherwise break daily lookup).
fn daily_path(config: &Config, now: DateTime<FixedOffset>) -> Result<String> {
    let template = config
        .note
        .daily_filename
        .as_deref()
        .unwrap_or(note::DEFAULT_DAILY_FILENAME);
    let title = daily_title(config, now)?;
    let labels = [daily_label(config).to_string()];
    note::note_path(template, &title, &labels, &config.note.meta, now)
}

/// Load and parse the note at `path`, or `None` if there is none there. A read
/// or parse failure on a note that *is* present propagates as an error rather
/// than being treated as "missing" (which would clobber it on the create path).
fn load_existing(vcs: &dyn VersionControl, path: &str) -> Result<Option<Note>> {
    if !vcs.list_files()?.iter().any(|listed| listed == path) {
        return Ok(None);
    }
    let raw = vcs.read_file(path)?;
    Ok(Some(note::parse_note(&raw)?))
}

/// Append `addition` below `existing`, separated by a blank line.
fn append_body(existing: &str, addition: &str) -> String {
    format!("{}\n\n{}", existing.trim_end(), addition.trim())
}

/// Append `addition` to an existing daily note and save it (refreshing
/// `updated`, preserving `created`/title). Returns the note's path.
fn append_and_save(
    vcs: &dyn VersionControl,
    config: &Config,
    note: Note,
    addition: &str,
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    let body = append_body(&note.content, addition);
    save_update(vcs, config, note, &body, now)
}

/// Save an updated daily note: ensure the daily label is present, then write
/// via `edit::save_edit` (which refreshes `updated` and preserves `created`).
fn save_update(
    vcs: &dyn VersionControl,
    config: &Config,
    mut note: Note,
    body: &str,
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    add_daily_label(&mut note.meta.labels, daily_label(config));
    edit::save_edit(vcs, note, body, now)
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

    let title = match title.map(str::trim).filter(|title| !title.is_empty()) {
        Some(title) => title.to_string(),
        None => daily_title(config, now)?,
    };

    let mut labels = labels.to_vec();
    add_daily_label(&mut labels, daily_label(config));
    let note = create::assemble_note(path.to_string(), title, content, config, &labels, now);
    let raw = note::to_raw(&note)?;
    vcs.write_file(path, &raw, &format!("Add note {path}"))?;
    println!("{path}");
    Ok(Some(path.to_string()))
}

/// The title for a new daily note, from `note.daily_title` (default
/// `Daily note for %Y-%m-%d`), rendered as a `chrono` date format.
fn daily_title(config: &Config, now: DateTime<FixedOffset>) -> Result<String> {
    let template = config
        .note
        .daily_title
        .as_deref()
        .unwrap_or(note::DEFAULT_DAILY_TITLE);
    note::render_title(template, &config.note.meta, now)
}

/// The label every daily note carries, from `note.daily_label` (default `daily`).
fn daily_label(config: &Config) -> &str {
    config
        .note
        .daily_label
        .as_deref()
        .unwrap_or(note::DEFAULT_DAILY_LABEL)
}

/// Ensure `label` is present in `labels`, without duplicating it.
fn add_daily_label(labels: &mut Vec<String>, label: &str) {
    if !labels.iter().any(|existing| existing.trim() == label) {
        labels.push(label.to_string());
    }
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
        assert_eq!(daily_path(&config, now()).unwrap(), "2026/07/03.md");
    }

    #[test]
    fn daily_path_uses_configured_template() {
        let mut config = Config::default();
        config.note.daily_filename = Some("journal/{created:%Y-%m-%d}".to_string());
        assert_eq!(daily_path(&config, now()).unwrap(), "journal/2026-07-03.md");
    }

    #[test]
    fn daily_path_resolves_title_and_labels_to_daily_values() {
        // `{title}` and `{labels}` must resolve to the daily note's own title
        // and label (defaults: "Daily note for 2026-07-03" and "daily"), not the
        // `unknown-*` placeholder.
        let mut config = Config::default();
        config.note.daily_filename = Some("{labels}/{title}".to_string());
        assert_eq!(
            daily_path(&config, now()).unwrap(),
            "daily/daily-note-for-2026-07-03.md"
        );
    }

    #[test]
    fn daily_title_defaults_to_dated_label() {
        let config = Config::default();
        assert_eq!(
            daily_title(&config, now()).unwrap(),
            "Daily note for 2026-07-03"
        );
    }

    #[test]
    fn daily_title_uses_configured_template() {
        let mut config = Config::default();
        config.note.daily_title = Some("Journal for {created:%d %B %Y}".to_string());
        assert_eq!(
            daily_title(&config, now()).unwrap(),
            "Journal for 03 July 2026"
        );
    }

    #[test]
    fn daily_title_keeps_meta_verbatim() {
        // A meta value in a title is NOT slugified (unlike in a filename).
        let mut config = Config::default();
        config.note.meta.insert(
            "author".to_string(),
            toml::Value::String("Paul van der Meijs".to_string()),
        );
        config.note.daily_title = Some("Journal by {author}".to_string());
        assert_eq!(
            daily_title(&config, now()).unwrap(),
            "Journal by Paul van der Meijs"
        );
    }

    #[test]
    fn daily_label_defaults_to_daily() {
        let config = Config::default();
        assert_eq!(daily_label(&config), "daily");
    }

    #[test]
    fn daily_label_uses_configured_value() {
        let mut config = Config::default();
        config.note.daily_label = Some("journal".to_string());
        assert_eq!(daily_label(&config), "journal");
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
    fn load_existing_propagates_parse_errors() {
        let backend = MemoryBackend::with_files(&[("2026/07/03.md", "no frontmatter here")]);
        assert!(load_existing(&backend, "2026/07/03.md").is_err());
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
        assert_eq!(
            saved.meta.labels,
            vec!["work".to_string(), "daily".to_string()]
        );
        assert_eq!(
            saved.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
    }

    #[test]
    fn save_new_daily_adds_the_daily_label() {
        let backend = MemoryBackend::new();
        let config = Config::default();
        save_new_daily(
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
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.meta.labels, vec!["daily".to_string()]);
    }

    #[test]
    fn save_new_daily_does_not_duplicate_the_daily_label() {
        let backend = MemoryBackend::new();
        let config = Config::default();
        save_new_daily(
            &backend,
            &config,
            "2026/07/03.md",
            "Hello",
            None,
            &["daily".to_string()],
            now(),
        )
        .unwrap()
        .unwrap();
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.meta.labels, vec!["daily".to_string()]);
    }

    #[test]
    fn append_and_save_appends_and_bumps_updated() {
        let backend = MemoryBackend::with_files(&[("2026/07/03.md", EXISTING)]);
        let config = Config::default();
        let note = parse_note(EXISTING).unwrap();
        let path = append_and_save(&backend, &config, note, "did X", now())
            .unwrap()
            .unwrap();
        assert_eq!(path, "2026/07/03.md");
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.content, "Morning notes\n\ndid X\n");
        assert_eq!(saved.meta.updated, now());
        assert_eq!(saved.meta.created, at("2026-07-03T08:00:00+02:00"));
        assert_eq!(saved.meta.title, "Daily note for 2026-07-03");
        assert_eq!(saved.meta.labels, vec!["daily".to_string()]);
    }
}
