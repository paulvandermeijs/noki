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
        builder.push_record(["labels".to_string(), note.meta.labels.join(", ")]);
    }
    let mut table = builder.build();
    table.with(Style::modern());
    format!("{table}\n\n{}", note.content)
}

/// Render a single note as pretty JSON (`{ "meta": {...}, "content": "..." }`).
pub fn render_note_json(note: &Note) -> Result<String> {
    Ok(serde_json::to_string_pretty(note)?)
}

/// Render a list of notes as a table (path, title, updated), without content.
pub fn render_list_human(notes: &[Note]) -> String {
    let rows: Vec<ListRow> = notes.iter().map(ListRow::from).collect();
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

#[derive(Serialize)]
struct Summary<'a> {
    meta: &'a Meta,
}

#[derive(Tabled)]
struct ListRow {
    path: String,
    title: String,
    updated: String,
}

impl From<&Note> for ListRow {
    fn from(note: &Note) -> Self {
        ListRow {
            path: note.meta.path.clone(),
            title: note.meta.title.clone(),
            updated: note.meta.updated.to_rfc2822(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::parse_note;

    const RAW: &str = "---\ntitle: My new note\npath: 2026/06/02/10:00:00-my-new-note.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\nHello, World!\n";

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
}
