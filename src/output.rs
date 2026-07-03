use crate::label::{self, LabelPalette};
use crate::note::{Meta, Note};
use anyhow::Result;
use serde::Serialize;
use tabled::builder::Builder;
use tabled::settings::Style;
use tabled::{Table, Tabled};

/// Render a single note as a metadata table followed by its content.
pub fn render_note_human(note: &Note) -> String {
    let mut builder = Builder::default();
    builder.push_record(["title".to_string(), note.meta.title.clone()]);
    builder.push_record(["path".to_string(), note.meta.path.clone()]);
    builder.push_record(["created".to_string(), note.meta.created.to_rfc2822()]);
    builder.push_record(["updated".to_string(), note.meta.updated.to_rfc2822()]);
    if !note.meta.labels.is_empty() {
        let mut palette = LabelPalette::new();
        let labels = label::render_labels(&note.meta.labels, usize::MAX, &mut palette);
        builder.push_record(["labels".to_string(), labels]);
    }
    for (key, value) in &note.meta.extra {
        builder.push_record([key.clone(), meta_value_display(value)]);
    }
    let mut table = builder.build();
    table.with(Style::modern());
    format!("{table}\n\n{}", note.content)
}

/// Render a single note as pretty JSON (`{ "meta": {...}, "content": "..." }`).
pub fn render_note_json(note: &Note) -> Result<String> {
    Ok(serde_json::to_string_pretty(note)?)
}

/// Render a list of notes as a table (path, title, labels, updated), without content.
pub fn render_list_human(notes: &[Note], max_visible_labels: usize) -> String {
    let mut palette = LabelPalette::new();
    let rows: Vec<ListRow> = notes
        .iter()
        .map(|note| ListRow::from_note(note, max_visible_labels, &mut palette))
        .collect();
    let mut table = Table::new(rows);
    table.with(Style::modern());
    table.to_string()
}

/// Render a list of notes as pretty JSON, each entry `{ "meta": {...} }`.
pub fn render_list_json(notes: &[Note]) -> Result<String> {
    let summaries: Vec<Summary> = notes
        .iter()
        .map(|note| Summary { meta: &note.meta })
        .collect();
    Ok(serde_json::to_string_pretty(&summaries)?)
}

/// Render a flattened frontmatter value (e.g. static `meta`) as a table cell.
fn meta_value_display(value: &serde_yaml_ng::Value) -> String {
    match value.as_str() {
        Some(text) => text.to_string(),
        None => serde_yaml_ng::to_string(value)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

#[derive(Serialize)]
struct Summary<'a> {
    meta: &'a Meta,
}

#[derive(Tabled)]
struct ListRow {
    path: String,
    title: String,
    labels: String,
    updated: String,
}

impl ListRow {
    fn from_note(note: &Note, max_visible_labels: usize, palette: &mut LabelPalette) -> Self {
        ListRow {
            path: note.meta.path.clone(),
            title: note.meta.title.clone(),
            labels: label::render_labels(&note.meta.labels, max_visible_labels, palette),
            updated: note.meta.updated.to_rfc2822(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::parse_note;

    const RAW: &str = "---\ntitle: My new note\npath: 2026/06/02/10:00:00-my-new-note.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\nHello, World!\n";

    const RAW_LABELS: &str = "---\ntitle: A note\npath: 2026/06/02/a.md\nlabels:\n- feature\n- backend\n- priority\n- ops\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\nBody\n";

    #[test]
    fn list_human_shows_colored_labels_truncated() {
        let note = parse_note(RAW_LABELS).unwrap();
        let text = render_list_human(&[note], 3);
        assert!(
            text.contains("\x1b["),
            "expected ANSI color codes in:\n{text}"
        );
        assert!(text.contains("feature"), "expected first label in:\n{text}");
        assert!(
            text.contains("+1 more"),
            "expected overflow marker in:\n{text}"
        );
    }

    #[test]
    fn note_json_has_meta_and_content() {
        let note = parse_note(RAW).unwrap();
        let json = render_note_json(&note).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["meta"]["title"], "My new note");
        assert_eq!(value["content"], "Hello, World!\n");
    }

    #[test]
    fn list_json_omits_content() {
        let note = parse_note(RAW).unwrap();
        let json = render_list_json(&[note]).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value[0].get("content").is_none());
        assert_eq!(value[0]["meta"]["title"], "My new note");
    }

    #[test]
    fn note_human_shows_title_and_content() {
        let note = parse_note(RAW).unwrap();
        let text = render_note_human(&note);
        assert!(text.contains("My new note"));
        assert!(text.contains("Hello, World!"));
    }

    #[test]
    fn note_human_shows_extra_meta() {
        let raw = "---\ntitle: T\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\nauthor: Paul van der Meijs\n---\n\nBody\n";
        let note = parse_note(raw).unwrap();
        let text = render_note_human(&note);
        assert!(text.contains("author"), "expected author key in:\n{text}");
        assert!(
            text.contains("Paul van der Meijs"),
            "expected author value in:\n{text}"
        );
    }

    #[test]
    fn note_human_colors_labels() {
        let note = parse_note(RAW_LABELS).unwrap();
        let text = render_note_human(&note);
        assert!(text.contains("labels"), "expected labels row in:\n{text}");
        assert!(
            text.contains("\x1b["),
            "expected ANSI color codes in:\n{text}"
        );
        assert!(text.contains("feature"), "expected label text in:\n{text}");
        assert!(
            text.contains("ops"),
            "single-note view shows all labels:\n{text}"
        );
    }
}
