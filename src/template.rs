use anyhow::{Result, bail};
use chrono::DateTime;
use chrono::FixedOffset;
use chrono::format::{Item, StrftimeItems};

/// A value a template field resolves to.
pub(crate) enum Field {
    /// A string; slugified for a path segment or kept verbatim, per [`Sanitize`].
    Text(String),
    /// A timestamp, formatted with the token's `:format` (chrono strftime),
    /// defaulting to `%Y-%m-%d`.
    Date(DateTime<FixedOffset>),
}

/// How a resolved string value is emitted.
#[derive(Clone, Copy)]
pub(crate) enum Sanitize {
    /// Slugify into a single path-safe segment (for filenames).
    Slug,
    /// Emit verbatim (for human text like titles).
    Raw,
}

/// Render a flat template. Tokens are `{field}` or `{field:format}`; `{{` and
/// `}}` are literal braces; everything else is literal text. `resolve` maps a
/// field name to its value; `sanitize` decides how string values are emitted
/// ([`Sanitize::Slug`] for paths, [`Sanitize::Raw`] for titles). A missing
/// value (`None`) or one that renders empty becomes `unknown-<field>`, so a
/// token never yields an empty segment. Date values are chrono-formatted
/// regardless of `sanitize`. Returns an error — never panics — only on template
/// *syntax* mistakes: a `:format` on a text field, a bad date format, or an
/// unterminated `{`.
pub(crate) fn render(
    template: &str,
    resolve: impl Fn(&str) -> Option<Field>,
    sanitize: Sanitize,
) -> Result<String> {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let mut token = String::new();
                let mut closed = false;
                for tc in chars.by_ref() {
                    if tc == '}' {
                        closed = true;
                        break;
                    }
                    token.push(tc);
                }
                if !closed {
                    bail!("unterminated '{{' in template");
                }
                out.push_str(&resolve_token(&token, &resolve, sanitize)?);
            }
            _ => out.push(c),
        }
    }
    Ok(out)
}

fn resolve_token(
    token: &str,
    resolve: &impl Fn(&str) -> Option<Field>,
    sanitize: Sanitize,
) -> Result<String> {
    let (name, format) = match token.split_once(':') {
        Some((name, format)) => (name, Some(format)),
        None => (token, None),
    };
    match resolve(name) {
        None => Ok(placeholder(name, sanitize)),
        Some(Field::Text(value)) => {
            if format.is_some() {
                bail!("template field '{name}' does not take a ':format'");
            }
            let rendered = match sanitize {
                Sanitize::Slug => truncate_slug(slug::slugify(&value), MAX_SLUG_LENGTH),
                Sanitize::Raw => value,
            };
            Ok(if rendered.is_empty() {
                placeholder(name, sanitize)
            } else {
                rendered
            })
        }
        Some(Field::Date(when)) => format_date(when, format.unwrap_or("%Y-%m-%d")),
    }
}

/// The fallback segment for a missing or empty field: `unknown-<name>`,
/// slugified in [`Sanitize::Slug`] mode and verbatim in [`Sanitize::Raw`].
fn placeholder(name: &str, sanitize: Sanitize) -> String {
    let raw = format!("unknown-{name}");
    match sanitize {
        Sanitize::Slug => slug::slugify(raw),
        Sanitize::Raw => raw,
    }
}

fn format_date(when: DateTime<FixedOffset>, format: &str) -> Result<String> {
    let items: Vec<Item> = StrftimeItems::new(format).collect();
    if items.iter().any(|item| matches!(item, Item::Error)) {
        bail!("invalid date format '{format}' in template");
    }
    Ok(when.format_with_items(items.iter()).to_string())
}

/// Path-segment slugs are capped so a paragraph-length title (e.g. a dictated
/// recording) cannot exceed the OS filename-component limit (255 bytes on
/// macOS/Linux). 80 keeps names readable with ample headroom for the
/// timestamp prefix and `.md` suffix.
const MAX_SLUG_LENGTH: usize = 80;

/// Cap `slug` at `max` characters, flooring to a whole word. `slug::slugify`
/// output is lowercase ASCII with single `-` separators and no leading or
/// trailing dash, so byte indexing is char-safe and every `-` is a word
/// boundary. A single word longer than `max` is hard-cut so the result is
/// never empty.
fn truncate_slug(slug: String, max: usize) -> String {
    if slug.len() <= max {
        return slug;
    }
    match slug[..=max].rfind('-') {
        Some(cut) => slug[..cut].to_string(),
        None => slug[..max].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn text_field_is_slugified() {
        let out = render(
            "{title}",
            |name| (name == "title").then(|| Field::Text("My Great Note!".to_string())),
            Sanitize::Slug,
        )
        .unwrap();
        assert_eq!(out, "my-great-note");
    }

    #[test]
    fn date_field_uses_its_format() {
        let when = at("2026-06-02T10:00:00+01:00");
        let out = render(
            "{created:%Y/%m/%d}",
            |name| (name == "created").then_some(Field::Date(when)),
            Sanitize::Slug,
        )
        .unwrap();
        assert_eq!(out, "2026/06/02");
    }

    #[test]
    fn date_field_defaults_to_iso_date() {
        let when = at("2026-06-02T10:00:00+01:00");
        let out = render("{created}", |_| Some(Field::Date(when)), Sanitize::Slug).unwrap();
        assert_eq!(out, "2026-06-02");
    }

    #[test]
    fn literal_text_and_tokens_combine() {
        let out = render(
            "notes/{title}",
            |_| Some(Field::Text("hi there".to_string())),
            Sanitize::Slug,
        )
        .unwrap();
        assert_eq!(out, "notes/hi-there");
    }

    #[test]
    fn braces_can_be_escaped() {
        let out = render("{{literal}}", |_| None, Sanitize::Slug).unwrap();
        assert_eq!(out, "{literal}");
    }

    #[test]
    fn missing_field_defaults_to_unknown_placeholder() {
        let out = render("{author}", |_| None, Sanitize::Slug).unwrap();
        assert_eq!(out, "unknown-author");
    }

    #[test]
    fn empty_value_defaults_to_unknown_placeholder() {
        // An empty (or unslugifiable) value must not leave an empty path segment.
        let out = render(
            "{labels}",
            |_| Some(Field::Text(String::new())),
            Sanitize::Slug,
        )
        .unwrap();
        assert_eq!(out, "unknown-labels");
    }

    #[test]
    fn overlong_slug_cuts_exactly_on_a_word_boundary() {
        // "sentence" slugs to 8 chars; n words join to 9n-1 chars, so 9 words
        // is exactly 80 — the boundary sits precisely at the cap.
        let value = "sentence ".repeat(20);
        let out = render(
            "{title}",
            |_| Some(Field::Text(value.clone())),
            Sanitize::Slug,
        )
        .unwrap();
        let nine_words = ["sentence"; 9].join("-");
        assert_eq!(out, nine_words);
        assert_eq!(out.len(), 80);
    }

    #[test]
    fn overlong_slug_floors_to_the_last_whole_word() {
        // "abcdefghij" slugs to 10 chars; 7 words = 76 chars, 8 words = 87.
        // The cap must floor to 7 whole words, never cut mid-word.
        let value = "abcdefghij ".repeat(20);
        let out = render(
            "{title}",
            |_| Some(Field::Text(value.clone())),
            Sanitize::Slug,
        )
        .unwrap();
        let seven_words = ["abcdefghij"; 7].join("-");
        assert_eq!(out, seven_words);
        assert!(!out.ends_with('-'));
    }

    #[test]
    fn single_giant_word_is_hard_cut_never_empty() {
        let value = "a".repeat(200);
        let out = render(
            "{title}",
            |_| Some(Field::Text(value.clone())),
            Sanitize::Slug,
        )
        .unwrap();
        assert_eq!(out, "a".repeat(80));
    }

    #[test]
    fn slug_of_exactly_max_length_is_unchanged() {
        // 9 "sentence" words slug to exactly 80 chars — at the cap, not over it.
        let value = "sentence ".repeat(9);
        let out = render(
            "{title}",
            |_| Some(Field::Text(value.clone())),
            Sanitize::Slug,
        )
        .unwrap();
        assert_eq!(out.len(), 80);
        assert_eq!(out, ["sentence"; 9].join("-"));
    }

    #[test]
    fn raw_mode_keeps_text_verbatim() {
        // Titles must preserve human formatting — no slugification.
        let out = render(
            "Journal by {author}",
            |_| Some(Field::Text("Paul van der Meijs".to_string())),
            Sanitize::Raw,
        )
        .unwrap();
        assert_eq!(out, "Journal by Paul van der Meijs");
    }

    #[test]
    fn raw_mode_dates_still_format() {
        let when = at("2026-06-02T10:00:00+01:00");
        let out = render(
            "Daily note for {created:%Y-%m-%d}",
            |_| Some(Field::Date(when)),
            Sanitize::Raw,
        )
        .unwrap();
        assert_eq!(out, "Daily note for 2026-06-02");
    }

    #[test]
    fn raw_mode_missing_field_is_unslugified_placeholder() {
        let out = render("by {author}", |_| None, Sanitize::Raw).unwrap();
        assert_eq!(out, "by unknown-author");
    }

    #[test]
    fn format_on_text_field_is_an_error() {
        let err = render(
            "{title:%Y}",
            |_| Some(Field::Text("x".to_string())),
            Sanitize::Slug,
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not take"), "got: {err}");
    }

    #[test]
    fn invalid_date_format_is_an_error_not_a_panic() {
        let when = at("2026-06-02T10:00:00+01:00");
        let err = render("{created:%J}", |_| Some(Field::Date(when)), Sanitize::Slug).unwrap_err();
        assert!(
            err.to_string().contains("invalid date format"),
            "got: {err}"
        );
    }

    #[test]
    fn unterminated_token_is_an_error() {
        let err = render(
            "{title",
            |_| Some(Field::Text("x".to_string())),
            Sanitize::Slug,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unterminated"), "got: {err}");
    }
}
