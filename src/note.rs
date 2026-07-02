use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, FixedOffset};
use markdown::mdast::Node;
use markdown::{Constructs, ParseOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub title: String,
    pub path: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(with = "rfc3339")]
    pub created: DateTime<FixedOffset>,
    #[serde(with = "rfc3339")]
    pub updated: DateTime<FixedOffset>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_yaml_ng::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub meta: Meta,
    pub content: String,
}

/// Parse a raw note (`---` YAML or `+++` TOML frontmatter followed by a Markdown body).
pub fn parse_note(raw: &str) -> Result<Note> {
    let tree = markdown::to_mdast(raw, &parse_options())
        .map_err(|error| anyhow!("Invalid note markdown: {error}"))?;
    let Node::Root(root) = &tree else {
        return Err(anyhow!("Note is missing frontmatter"));
    };

    let front = root
        .children
        .iter()
        .find(|node| matches!(node, Node::Yaml(_) | Node::Toml(_)))
        .ok_or_else(|| anyhow!("Note is missing frontmatter"))?;

    let meta: Meta = match front {
        Node::Yaml(node) => {
            serde_yaml_ng::from_str(&node.value).context("Invalid note frontmatter")?
        }
        Node::Toml(node) => toml::from_str(&node.value).context("Invalid note frontmatter")?,
        _ => return Err(anyhow!("Note is missing frontmatter")),
    };

    let content = body_after(raw, node_end(front));
    Ok(Note { meta, content })
}

/// Serialize a note back into its raw on-disk representation.
pub fn to_raw(note: &Note) -> Result<String> {
    let yaml = serde_yaml_ng::to_string(&note.meta)?;
    Ok(format!("---\n{yaml}---\n\n{}", note.content))
}

pub const DEFAULT_FILENAME: &str = "%Y/%m/%d/%H:%M:%S-%title";

/// Derive a human title from the content: the first heading, else the first
/// line of the first paragraph, else "untitled".
pub fn title_from_content(content: &str) -> String {
    let Ok(tree) = markdown::to_mdast(content, &parse_options()) else {
        return "untitled".to_string();
    };
    let Node::Root(root) = tree else {
        return "untitled".to_string();
    };

    for node in &root.children {
        match node {
            Node::Heading(heading) => {
                let text = inline_text(&heading.children);
                let text = text.trim();
                if !text.is_empty() {
                    return text.to_string();
                }
            }
            Node::Paragraph(paragraph) => {
                let text = inline_text(&paragraph.children);
                if let Some(line) = text.lines().next() {
                    let line = line.trim();
                    if !line.is_empty() {
                        return line.to_string();
                    }
                }
            }
            _ => {}
        }
    }

    "untitled".to_string()
}

/// Render a note's relative path from a template. `%title` is replaced with a
/// slug of the title; all other `%` specifiers are `chrono` date formats.
/// The `.md` extension is always appended.
pub fn note_path(template: &str, title: &str, when: DateTime<FixedOffset>) -> String {
    let slug = slug::slugify(title);
    let with_title = template.replace("%title", &slug);
    format!("{}.md", when.format(&with_title))
}

/// Parse options with the frontmatter construct enabled (GFM otherwise).
fn parse_options() -> ParseOptions {
    ParseOptions {
        constructs: Constructs {
            frontmatter: true,
            ..Constructs::gfm()
        },
        ..ParseOptions::gfm()
    }
}

/// Byte offset just past the end of a node (0 if it has no position).
fn node_end(node: &Node) -> usize {
    node.position().map_or(0, |position| position.end.offset)
}

/// The raw body text after the frontmatter: drop the newline ending the
/// closing fence line and one optional blank separator line.
fn body_after(raw: &str, frontmatter_end: usize) -> String {
    let rest = &raw[frontmatter_end..];
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    rest.to_string()
}

/// The concatenated plain text of inline nodes, with markup stripped.
fn inline_text(nodes: &[Node]) -> String {
    let mut text = String::new();
    for node in nodes {
        match node {
            Node::Text(value) => text.push_str(&value.value),
            Node::InlineCode(value) => text.push_str(&value.value),
            other => {
                if let Some(children) = other.children() {
                    text.push_str(&inline_text(children));
                }
            }
        }
    }
    text
}

pub(crate) mod rfc3339 {
    use chrono::{DateTime, FixedOffset, SecondsFormat};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(dt: &DateTime<FixedOffset>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&dt.to_rfc3339_opts(SecondsFormat::Secs, false))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<DateTime<FixedOffset>, D::Error> {
        let s = String::deserialize(d)?;
        DateTime::parse_from_rfc3339(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "---\ntitle: My new note\npath: 2026/06/02/10:00:00-my-new-note.md\nlabels:\n- needs-review\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\nHello, World!\n";

    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn parses_frontmatter_and_body() {
        let note = parse_note(RAW).unwrap();
        assert_eq!(note.meta.title, "My new note");
        assert_eq!(note.meta.path, "2026/06/02/10:00:00-my-new-note.md");
        assert_eq!(note.meta.labels, vec!["needs-review".to_string()]);
        assert_eq!(note.meta.created.to_rfc3339(), "2026-06-02T10:00:00+01:00");
        assert_eq!(note.content, "Hello, World!\n");
    }

    #[test]
    fn round_trips_through_to_raw() {
        let note = parse_note(RAW).unwrap();
        let raw = to_raw(&note).unwrap();
        let reparsed = parse_note(&raw).unwrap();
        assert_eq!(reparsed.meta.title, note.meta.title);
        assert_eq!(reparsed.content, note.content);
    }

    #[test]
    fn missing_frontmatter_is_an_error() {
        let err = parse_note("no frontmatter here").unwrap_err();
        assert_eq!(err.to_string(), "Note is missing frontmatter");
    }

    #[test]
    fn title_uses_first_non_empty_line_without_heading_marks() {
        assert_eq!(title_from_content("# My new note\n\nbody"), "My new note");
        assert_eq!(title_from_content("plain title"), "plain title");
        assert_eq!(title_from_content("   "), "untitled");
    }

    #[test]
    fn title_reads_setext_heading() {
        assert_eq!(
            title_from_content("Setext Title\n=====\n\nbody"),
            "Setext Title"
        );
    }

    #[test]
    fn title_ignores_heading_inside_code_fence() {
        let content = "```\n# not a title\n```\n\n# Real Title\n";
        assert_eq!(title_from_content(content), "Real Title");
    }

    #[test]
    fn title_strips_inline_markup_from_heading() {
        assert_eq!(title_from_content("# Hello **world**\n"), "Hello world");
    }

    #[test]
    fn title_uses_first_line_of_leading_paragraph() {
        assert_eq!(
            title_from_content("First line\nsecond line\n"),
            "First line"
        );
    }

    #[test]
    fn note_path_expands_date_and_slugged_title() {
        let when = at("2026-06-02T10:00:00+01:00");
        let path = note_path(DEFAULT_FILENAME, "My new note", when);
        assert_eq!(path, "2026/06/02/10:00:00-my-new-note.md");
    }

    #[test]
    fn reads_toml_frontmatter() {
        let raw = "+++\ntitle = \"T\"\npath = \"p.md\"\nlabels = []\ncreated = \"2026-06-02T10:00:00+01:00\"\nupdated = \"2026-06-02T10:00:02+01:00\"\n+++\n\nBody\n";
        let note = parse_note(raw).unwrap();
        assert_eq!(note.meta.title, "T");
        assert_eq!(note.meta.created.to_rfc3339(), "2026-06-02T10:00:00+01:00");
        assert_eq!(note.content, "Body\n");
    }

    #[test]
    fn body_thematic_break_is_not_frontmatter() {
        let raw = "---\ntitle: T\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\nAbove\n\n---\n\nBelow\n";
        let note = parse_note(raw).unwrap();
        assert_eq!(note.meta.title, "T");
        assert!(
            note.content.contains("Above"),
            "content was: {:?}",
            note.content
        );
        assert!(
            note.content.contains("---"),
            "content was: {:?}",
            note.content
        );
        assert!(
            note.content.contains("Below"),
            "content was: {:?}",
            note.content
        );
    }

    #[test]
    fn round_trips_content_starting_with_blank_line() {
        let mut note = parse_note(RAW).unwrap();
        note.content = "\nBody after a blank line\n".to_string();
        let raw = to_raw(&note).unwrap();
        let reparsed = parse_note(&raw).unwrap();
        assert_eq!(reparsed.content, "\nBody after a blank line\n");
    }

    #[test]
    fn frontmatter_closed_at_eof_yields_empty_content() {
        let raw = "---\ntitle: T\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---";
        let note = parse_note(raw).unwrap();
        assert_eq!(note.meta.title, "T");
        assert_eq!(note.content, "");
    }
}
