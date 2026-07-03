use crate::note::{self, Note};
use crate::vcs::VersionControl;
use anyhow::Result;
use chrono::{DateTime, FixedOffset, Local};

/// Load an existing note, open its body in the editor, and save the result,
/// refreshing the `updated` timestamp while preserving `created`.
pub fn run(vcs: &dyn VersionControl, path: &str) -> Result<()> {
    let raw = vcs.read_file(path)?;
    let note = note::parse_note(&raw)?;

    let edited = crate::editor::get_content_from_editor(Some(note.content.clone()))?;

    let now = Local::now().fixed_offset();
    save_edit(vcs, note, &edited, now)?;
    Ok(())
}

/// Write `content` back into an existing note, setting `updated` to `now` and
/// leaving `created`, `title`, `path`, `labels`, and extra metadata untouched.
/// Returns the note's path, or `None` when the content is empty.
///
/// This is the shared save path the forthcoming `--daily` note flow reuses.
pub(crate) fn save_edit(
    vcs: &dyn VersionControl,
    mut note: Note,
    content: &str,
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    let content = content.trim();
    if content.is_empty() {
        eprintln!("Note is empty; leaving it unchanged.");
        return Ok(None);
    }

    note.content = format!("{content}\n");
    note.meta.updated = now;

    let path = note.meta.path.clone();
    let raw = note::to_raw(&note)?;
    vcs.write_file(&path, &raw, &format!("Update note {path}"))?;
    println!("{path}");
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::parse_note;
    use crate::vcs::{MemoryBackend, VersionControl};
    use chrono::DateTime;

    const NOTE: &str = "---\ntitle: My note\npath: 2026/06/02/my-note.md\nlabels:\n- work\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:00+01:00\nauthor: Paul\n---\n\nOriginal body\n";

    fn at(s: &str) -> DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn save_edit_updates_body_and_updated_but_keeps_created() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/my-note.md", NOTE)]);
        let note = parse_note(NOTE).unwrap();
        let now = at("2026-06-03T12:00:00+01:00");

        let path = save_edit(&backend, note, "New body", now).unwrap().unwrap();
        assert_eq!(path, "2026/06/02/my-note.md");

        let raw = backend.read_file("2026/06/02/my-note.md").unwrap();
        let saved = parse_note(&raw).unwrap();
        assert_eq!(saved.content, "New body\n");
        assert_eq!(saved.meta.updated, now);
        assert_eq!(saved.meta.created, at("2026-06-02T10:00:00+01:00"));
    }

    #[test]
    fn save_edit_preserves_title_labels_and_extra() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/my-note.md", NOTE)]);
        let note = parse_note(NOTE).unwrap();
        let now = at("2026-06-03T12:00:00+01:00");

        save_edit(&backend, note, "New body", now).unwrap().unwrap();

        let raw = backend.read_file("2026/06/02/my-note.md").unwrap();
        let saved = parse_note(&raw).unwrap();
        assert_eq!(saved.meta.title, "My note");
        assert_eq!(saved.meta.labels, vec!["work".to_string()]);
        assert_eq!(
            saved.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
    }

    #[test]
    fn save_edit_skips_empty_content_and_leaves_note_unchanged() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/my-note.md", NOTE)]);
        let note = parse_note(NOTE).unwrap();
        let now = at("2026-06-03T12:00:00+01:00");

        assert!(save_edit(&backend, note, "   \n", now).unwrap().is_none());

        // The stored note is untouched — no write happened.
        assert_eq!(backend.read_file("2026/06/02/my-note.md").unwrap(), NOTE);
    }

    #[test]
    fn run_errors_when_note_is_missing() {
        let backend = MemoryBackend::new();
        let err = run(&backend, "nope.md").unwrap_err();
        assert_eq!(err.to_string(), "No note at nope.md");
    }

    #[test]
    fn save_edit_converts_toml_frontmatter_to_yaml_losslessly() {
        let toml_note = "+++\ntitle = \"My note\"\npath = \"2026/06/02/my-note.md\"\nlabels = [\"work\"]\ncreated = \"2026-06-02T10:00:00+01:00\"\nupdated = \"2026-06-02T10:00:00+01:00\"\nauthor = \"Paul\"\n+++\n\nOriginal body\n";
        let backend = MemoryBackend::with_files(&[("2026/06/02/my-note.md", toml_note)]);
        let note = parse_note(toml_note).unwrap();
        let now = at("2026-06-03T12:00:00+01:00");

        save_edit(&backend, note, "New body", now).unwrap().unwrap();

        let raw = backend.read_file("2026/06/02/my-note.md").unwrap();
        assert!(
            raw.starts_with("---"),
            "Expected YAML frontmatter, got: {}",
            raw
        );

        let saved = parse_note(&raw).unwrap();
        assert_eq!(saved.content, "New body\n");
        assert_eq!(saved.meta.updated, now);
        assert_eq!(saved.meta.created, at("2026-06-02T10:00:00+01:00"));
        assert_eq!(saved.meta.title, "My note");
        assert_eq!(saved.meta.labels, vec!["work".to_string()]);
        assert_eq!(
            saved.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
    }
}
