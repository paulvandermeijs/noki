use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset};
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

/// Parse a raw note (`---` YAML frontmatter followed by a Markdown body).
pub fn parse_note(raw: &str) -> Result<Note> {
    let body = raw
        .strip_prefix("---\n")
        .context("Note is missing frontmatter")?;
    let marker = body
        .find("\n---\n")
        .context("Note frontmatter is not terminated")?;
    let yaml = &body[..marker];
    let mut content = &body[marker + "\n---\n".len()..];
    if let Some(rest) = content.strip_prefix('\n') {
        content = rest; // drop the single blank separator line
    }
    let meta: Meta = serde_yaml_ng::from_str(yaml).context("Invalid note frontmatter")?;
    Ok(Note {
        meta,
        content: content.to_string(),
    })
}

/// Serialize a note back into its raw on-disk representation.
pub fn to_raw(note: &Note) -> Result<String> {
    let yaml = serde_yaml_ng::to_string(&note.meta)?;
    Ok(format!("---\n{yaml}---\n\n{}", note.content))
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
}
