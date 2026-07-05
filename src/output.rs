use crate::label::{self, LabelPalette};
use crate::note::{Meta, Note};
use anyhow::Result;
use serde::Serialize;
use tabled::builder::Builder;
use tabled::settings::object::{Columns, Rows};
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Color, Modify, Style, Width};
use tabled::{Table, Tabled};

/// Render a single note as a metadata table followed by its content. `color`
/// enables ANSI-colored label chips and a rendered Markdown body (disable for
/// non-terminal output, where the raw Markdown source is emitted instead).
/// `width` is the column budget for wrapping the rendered body. `table_width`,
/// when `Some`, also caps the metadata table to that many columns (used when a
/// `note.max_width` is configured); `None` leaves the table at its natural width.
pub fn render_note_human(
    note: &Note,
    width: usize,
    table_width: Option<usize>,
    color: bool,
) -> String {
    let mut builder = Builder::default();
    builder.push_record(["title".to_string(), note.meta.title.clone()]);
    builder.push_record(["path".to_string(), note.meta.path.clone()]);
    builder.push_record(["created".to_string(), note.meta.created.to_rfc2822()]);
    builder.push_record(["updated".to_string(), note.meta.updated.to_rfc2822()]);
    if !note.meta.labels.is_empty() {
        let mut palette = LabelPalette::new();
        let labels = label::render_labels(&note.meta.labels, usize::MAX, &mut palette, color);
        builder.push_record(["labels".to_string(), labels]);
    }
    for (key, value) in &note.meta.extra {
        builder.push_record([key.clone(), meta_value_display(value)]);
    }
    let mut table = builder.build();
    apply_meta_style(&mut table, color);
    if let Some(max) = table_width {
        table.with(Width::wrap(max).keep_words(true));
    }
    let body = if color {
        crate::render::render(&note.content, width, true)
    } else {
        note.content.clone()
    };
    format!("{table}\n\n{body}")
}

/// Render a single note as pretty JSON (`{ "meta": {...}, "content": "..." }`).
pub fn render_note_json(note: &Note) -> Result<String> {
    Ok(serde_json::to_string_pretty(note)?)
}

/// Render a list of notes as a table (path, title, labels, updated), without
/// content. `color` enables ANSI-colored label chips (disable for non-terminal
/// output).
pub fn render_list_human(notes: &[Note], max_visible_labels: usize, color: bool) -> String {
    let mut palette = LabelPalette::new();
    let rows: Vec<ListRow> = notes
        .iter()
        .map(|note| ListRow::from_note(note, max_visible_labels, &mut palette, color))
        .collect();
    let mut table = Table::new(rows);
    apply_list_style(&mut table, color);
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

/// Style the list table: an outer frame with a double-line rule under the
/// header and horizontal rules between rows, but no vertical column separators —
/// columns are spaced apart by their padding. Headers are bold when `color`. The
/// rule uses `╞`/`╡` ends so it joins the single-line frame flush.
fn apply_list_style(table: &mut Table, color: bool) {
    let header_rule = HorizontalLine::inherit(Style::extended())
        .remove_intersection()
        .left('╞')
        .right('╡');
    table.with(
        Style::modern()
            .remove_vertical()
            .horizontals([(1, header_rule)]),
    );
    if color {
        table.with(Modify::new(Rows::first()).with(Color::BOLD));
    }
}

/// Style the metadata table: an outer frame with a double-line divider between
/// the header column (keys) and the values, but no horizontal rules between
/// rows. Keys are bold when `color`. The divider uses `╥`/`╨` ends so it joins
/// the single-line frame flush.
fn apply_meta_style(table: &mut Table, color: bool) {
    let divider = VerticalLine::inherit(Style::extended())
        .remove_intersection()
        .top('╥')
        .bottom('╨');
    table.with(
        Style::modern()
            .remove_horizontal()
            .verticals([(1, divider)]),
    );
    if color {
        table.with(Modify::new(Columns::first()).with(Color::BOLD));
    }
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
    fn from_note(
        note: &Note,
        max_visible_labels: usize,
        palette: &mut LabelPalette,
        color: bool,
    ) -> Self {
        ListRow {
            path: note.meta.path.clone(),
            title: note.meta.title.clone(),
            labels: label::render_labels(&note.meta.labels, max_visible_labels, palette, color),
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
        let text = render_list_human(&[note], 3, true);
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
    fn list_human_without_color_omits_ansi() {
        let note = parse_note(RAW_LABELS).unwrap();
        let text = render_list_human(&[note], 3, false);
        assert!(!text.contains('\x1b'), "expected no ANSI codes in:\n{text}");
        assert!(text.contains("feature"), "expected first label in:\n{text}");
        assert!(
            text.contains("+1 more"),
            "expected overflow marker in:\n{text}"
        );
    }

    #[test]
    fn list_human_bold_headers_when_color() {
        let note = parse_note(RAW).unwrap();
        let text = render_list_human(&[note], 3, true);
        assert!(
            text.contains("\x1b[1m"),
            "expected bold header ANSI in:\n{text}"
        );
    }

    #[test]
    fn note_human_bold_header_column_when_color() {
        let note = parse_note(RAW).unwrap();
        let text = render_note_human(&note, 80, None, true);
        assert!(
            text.contains("\x1b[1m"),
            "expected bold header-column ANSI in:\n{text}"
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
        let text = render_note_human(&note, 80, None, true);
        assert!(text.contains("My new note"));
        assert!(text.contains("Hello, World!"));
    }

    #[test]
    fn note_human_shows_extra_meta() {
        let raw = "---\ntitle: T\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\nauthor: Paul van der Meijs\n---\n\nBody\n";
        let note = parse_note(raw).unwrap();
        let text = render_note_human(&note, 80, None, true);
        assert!(text.contains("author"), "expected author key in:\n{text}");
        assert!(
            text.contains("Paul van der Meijs"),
            "expected author value in:\n{text}"
        );
    }

    #[test]
    fn note_human_colors_labels() {
        let note = parse_note(RAW_LABELS).unwrap();
        let text = render_note_human(&note, 80, None, true);
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

    #[test]
    fn note_human_without_color_omits_ansi() {
        let note = parse_note(RAW_LABELS).unwrap();
        let text = render_note_human(&note, 80, None, false);
        assert!(!text.contains('\x1b'), "expected no ANSI codes in:\n{text}");
        assert!(text.contains("feature"), "expected label text in:\n{text}");
        assert!(
            text.contains("ops"),
            "single-note view shows all labels:\n{text}"
        );
    }

    #[test]
    fn note_human_renders_body_when_color() {
        let raw = "---\ntitle: T\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\n# Heading\n";
        let note = parse_note(raw).unwrap();
        let text = render_note_human(&note, 80, None, true);
        assert!(
            text.contains("\x1b[1m"),
            "expected rendered bold heading in:\n{text}"
        );
        assert!(
            text.contains("Heading"),
            "expected heading text in:\n{text}"
        );
    }

    #[test]
    fn note_human_keeps_raw_body_when_no_color() {
        let raw = "---\ntitle: T\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\n# Heading\n";
        let note = parse_note(raw).unwrap();
        let text = render_note_human(&note, 80, None, false);
        assert!(
            text.contains("# Heading"),
            "expected literal markdown source in:\n{text}"
        );
    }

    #[test]
    fn note_human_caps_meta_table_to_table_width() {
        // A title far wider than the cap must wrap so no table line exceeds it.
        let raw = "---\ntitle: An extremely long note title that clearly exceeds the configured maximum width\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\nok\n";
        let note = parse_note(raw).unwrap();
        let text = render_note_human(&note, 30, Some(30), false);
        for line in text.lines() {
            assert!(
                line.chars().count() <= 30,
                "line exceeds cap (30): {line:?} in:\n{text}"
            );
        }
        // The full title text is preserved (wrapped, not truncated).
        assert!(
            text.contains("An extremely"),
            "title start missing in:\n{text}"
        );
        assert!(
            text.contains("maximum width"),
            "title end missing in:\n{text}"
        );
    }

    #[test]
    fn note_human_leaves_meta_table_natural_when_no_cap() {
        // With table_width None, a long value is not wrapped by us.
        let raw = "---\ntitle: An extremely long note title that clearly exceeds any small width\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\nok\n";
        let note = parse_note(raw).unwrap();
        let text = render_note_human(&note, 30, None, false);
        assert!(
            text.lines().any(|line| line.chars().count() > 30),
            "expected an unwrapped (wide) table line with no cap in:\n{text}"
        );
    }
}
