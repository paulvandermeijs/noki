use anyhow::{Result, bail};
use chrono::DateTime;
use chrono::FixedOffset;
use chrono::format::{Item, StrftimeItems};

/// A value a template field resolves to.
pub(crate) enum Field {
    /// A string, slugified into a single path-safe segment.
    Text(String),
    /// A timestamp, formatted with the token's `:format` (chrono strftime),
    /// defaulting to `%Y-%m-%d`.
    Date(DateTime<FixedOffset>),
}

/// Render a flat template. Tokens are `{field}` or `{field:format}`; `{{` and
/// `}}` are literal braces; everything else is literal text. `resolve` maps a
/// field name to its value. A missing value (`None`) or one that slugifies to
/// empty renders as `unknown-<field>`, so a token never yields an empty path
/// segment. String values are slugified; date values are chrono-formatted.
/// Returns an error — never panics — only on template *syntax* mistakes: a
/// `:format` on a text field, a bad date format, or an unterminated `{`.
pub(crate) fn render(template: &str, resolve: impl Fn(&str) -> Option<Field>) -> Result<String> {
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
                out.push_str(&resolve_token(&token, &resolve)?);
            }
            _ => out.push(c),
        }
    }
    Ok(out)
}

fn resolve_token(token: &str, resolve: &impl Fn(&str) -> Option<Field>) -> Result<String> {
    let (name, format) = match token.split_once(':') {
        Some((name, format)) => (name, Some(format)),
        None => (token, None),
    };
    match resolve(name) {
        None => Ok(placeholder(name)),
        Some(Field::Text(value)) => {
            if format.is_some() {
                bail!("template field '{name}' does not take a ':format'");
            }
            let slug = slug::slugify(value);
            Ok(if slug.is_empty() {
                placeholder(name)
            } else {
                slug
            })
        }
        Some(Field::Date(when)) => format_date(when, format.unwrap_or("%Y-%m-%d")),
    }
}

/// The fallback segment for a missing or empty field: `unknown-<name>`, slugified.
fn placeholder(name: &str) -> String {
    slug::slugify(format!("unknown-{name}"))
}

fn format_date(when: DateTime<FixedOffset>, format: &str) -> Result<String> {
    let items: Vec<Item> = StrftimeItems::new(format).collect();
    if items.iter().any(|item| matches!(item, Item::Error)) {
        bail!("invalid date format '{format}' in template");
    }
    Ok(when.format_with_items(items.iter()).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn text_field_is_slugified() {
        let out = render("{title}", |name| {
            (name == "title").then(|| Field::Text("My Great Note!".to_string()))
        })
        .unwrap();
        assert_eq!(out, "my-great-note");
    }

    #[test]
    fn date_field_uses_its_format() {
        let when = at("2026-06-02T10:00:00+01:00");
        let out = render("{created:%Y/%m/%d}", |name| {
            (name == "created").then_some(Field::Date(when))
        })
        .unwrap();
        assert_eq!(out, "2026/06/02");
    }

    #[test]
    fn date_field_defaults_to_iso_date() {
        let when = at("2026-06-02T10:00:00+01:00");
        let out = render("{created}", |_| Some(Field::Date(when))).unwrap();
        assert_eq!(out, "2026-06-02");
    }

    #[test]
    fn literal_text_and_tokens_combine() {
        let out = render("notes/{title}", |_| {
            Some(Field::Text("hi there".to_string()))
        })
        .unwrap();
        assert_eq!(out, "notes/hi-there");
    }

    #[test]
    fn braces_can_be_escaped() {
        let out = render("{{literal}}", |_| None).unwrap();
        assert_eq!(out, "{literal}");
    }

    #[test]
    fn missing_field_defaults_to_unknown_placeholder() {
        let out = render("{author}", |_| None).unwrap();
        assert_eq!(out, "unknown-author");
    }

    #[test]
    fn empty_value_defaults_to_unknown_placeholder() {
        // An empty (or unslugifiable) value must not leave an empty path segment.
        let out = render("{labels}", |_| Some(Field::Text(String::new()))).unwrap();
        assert_eq!(out, "unknown-labels");
    }

    #[test]
    fn format_on_text_field_is_an_error() {
        let err = render("{title:%Y}", |_| Some(Field::Text("x".to_string()))).unwrap_err();
        assert!(err.to_string().contains("does not take"), "got: {err}");
    }

    #[test]
    fn invalid_date_format_is_an_error_not_a_panic() {
        let when = at("2026-06-02T10:00:00+01:00");
        let err = render("{created:%J}", |_| Some(Field::Date(when))).unwrap_err();
        assert!(
            err.to_string().contains("invalid date format"),
            "got: {err}"
        );
    }

    #[test]
    fn unterminated_token_is_an_error() {
        let err = render("{title", |_| Some(Field::Text("x".to_string()))).unwrap_err();
        assert!(err.to_string().contains("unterminated"), "got: {err}");
    }
}
