use markdown::mdast::Node;

/// A resolved inline style. All flags are cumulative and pre-merged into each
/// span, so downstream code never has to track nesting.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    pub dim: bool,
    pub code: bool,
    pub link: bool,
}

/// A run of plain text with one resolved style. Text is ANSI-free; width is
/// simply `text.chars().count()`.
#[derive(Clone)]
pub(crate) struct Span {
    pub text: String,
    pub style: Style,
}

/// Flatten inline AST nodes into styled plain-text spans. Soft line breaks
/// become spaces; hard breaks (`Node::Break`) become a `"\n"` span.
pub(crate) fn spans(nodes: &[Node]) -> Vec<Span> {
    let mut out = Vec::new();
    collect(nodes, Style::default(), &mut out);
    out
}

/// Render a run of spans to a single string. When `color`, each styled span is
/// wrapped in a self-contained `SGR … reset` pair; otherwise plain text.
pub(crate) fn emit(spans: &[Span], color: bool) -> String {
    let mut out = String::new();
    for span in spans {
        let codes = if color {
            sgr_codes(span.style)
        } else {
            Vec::new()
        };
        if codes.is_empty() {
            out.push_str(&span.text);
        } else {
            out.push_str("\x1b[");
            out.push_str(&codes.join(";"));
            out.push('m');
            out.push_str(&span.text);
            out.push_str("\x1b[0m");
        }
    }
    out
}

fn collect(nodes: &[Node], base: Style, out: &mut Vec<Span>) {
    for node in nodes {
        match node {
            Node::Text(t) => out.push(Span {
                text: t.value.replace('\n', " "),
                style: base,
            }),
            Node::InlineCode(c) => {
                let mut style = base;
                style.code = true;
                out.push(Span {
                    text: c.value.replace('\n', " "),
                    style,
                });
            }
            Node::Emphasis(e) => {
                let mut style = base;
                style.italic = true;
                collect(&e.children, style, out);
            }
            Node::Strong(s) => {
                let mut style = base;
                style.bold = true;
                collect(&s.children, style, out);
            }
            Node::Delete(d) => {
                let mut style = base;
                style.strike = true;
                collect(&d.children, style, out);
            }
            Node::Link(l) => {
                let mut style = base;
                style.link = true;
                collect(&l.children, style, out);
                let mut url_style = base;
                url_style.dim = true;
                out.push(Span {
                    text: format!(" ({})", l.url),
                    style: url_style,
                });
            }
            Node::Image(i) => {
                let mut style = base;
                style.dim = true;
                out.push(Span {
                    text: format!("[image: {}]", i.alt),
                    style,
                });
            }
            Node::Break(_) => out.push(Span {
                text: "\n".to_string(),
                style: base,
            }),
            Node::Html(h) => {
                let mut style = base;
                style.dim = true;
                out.push(Span {
                    text: h.value.replace('\n', " "),
                    style,
                });
            }
            Node::LinkReference(l) => {
                let mut style = base;
                style.link = true;
                collect(&l.children, style, out);
            }
            Node::ImageReference(i) => {
                let mut style = base;
                style.dim = true;
                out.push(Span {
                    text: format!("[image: {}]", i.alt),
                    style,
                });
            }
            Node::FootnoteReference(f) => {
                let mut style = base;
                style.dim = true;
                out.push(Span {
                    text: format!(
                        "[^{}]",
                        f.label.clone().unwrap_or_else(|| f.identifier.clone())
                    ),
                    style,
                });
            }
            other => {
                if let Some(children) = other.children() {
                    collect(children, base, out);
                }
            }
        }
    }
}

fn sgr_codes(style: Style) -> Vec<&'static str> {
    let mut codes = Vec::new();
    if style.bold {
        codes.push("1");
    }
    if style.dim {
        codes.push("2");
    }
    if style.italic {
        codes.push("3");
    }
    if style.underline || style.link {
        codes.push("4");
    }
    if style.strike {
        codes.push("9");
    }
    if style.code {
        codes.push("36");
    }
    if style.link {
        codes.push("34");
    }
    codes
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown::ParseOptions;
    use markdown::mdast::Node;

    /// Inline children of the first paragraph in `md`.
    fn inline(md: &str) -> Vec<Node> {
        let tree = markdown::to_mdast(md, &ParseOptions::gfm()).unwrap();
        let Node::Root(root) = tree else {
            panic!("no root")
        };
        for node in root.children {
            if let Node::Paragraph(p) = node {
                return p.children;
            }
        }
        panic!("no paragraph in: {md}");
    }

    #[test]
    fn strong_sets_bold() {
        let sp = spans(&inline("**hi**"));
        assert_eq!(sp.len(), 1);
        assert_eq!(sp[0].text, "hi");
        assert!(sp[0].style.bold);
        assert!(!sp[0].style.italic);
    }

    #[test]
    fn nested_emphasis_merges_styles() {
        let sp = spans(&inline("**_hi_**"));
        assert_eq!(sp.len(), 1);
        assert!(sp[0].style.bold && sp[0].style.italic);
    }

    #[test]
    fn inline_code_sets_code_flag() {
        let sp = spans(&inline("`x`"));
        assert_eq!(sp[0].text, "x");
        assert!(sp[0].style.code);
    }

    #[test]
    fn link_appends_dim_url() {
        let sp = spans(&inline("[text](https://e.com)"));
        // link text carries the link style...
        assert!(sp.iter().any(|s| s.text == "text" && s.style.link));
        // ...followed by a dim " (url)" span.
        assert!(
            sp.iter()
                .any(|s| s.text.contains("https://e.com") && s.style.dim)
        );
    }

    #[test]
    fn link_is_underlined_and_blue() {
        // Underline is reserved for links (browser-like) and explicit markup.
        let link = Span {
            text: "text".to_string(),
            style: Style {
                link: true,
                ..Style::default()
            },
        };
        let out = emit(&[link], true);
        assert!(
            out.contains("\x1b[4;34m"),
            "link should be underlined + blue: {out:?}"
        );
    }

    #[test]
    fn soft_break_becomes_space() {
        let sp = spans(&inline("a\nb"));
        let joined: String = sp.iter().map(|s| s.text.as_str()).collect();
        assert!(
            !joined.contains('\n'),
            "soft break should be a space: {joined:?}"
        );
    }

    #[test]
    fn emit_wraps_bold_in_ansi() {
        let out = emit(
            &[Span {
                text: "hi".into(),
                style: Style {
                    bold: true,
                    ..Style::default()
                },
            }],
            true,
        );
        assert!(out.contains("\x1b[1m"), "expected bold SGR in {out:?}");
        assert!(out.contains("hi"));
        assert!(out.ends_with("\x1b[0m"));
    }

    #[test]
    fn emit_plain_has_no_ansi() {
        let out = emit(
            &[Span {
                text: "hi".into(),
                style: Style {
                    bold: true,
                    ..Style::default()
                },
            }],
            false,
        );
        assert_eq!(out, "hi");
    }

    #[test]
    fn link_reference_gets_link_style() {
        let sp = spans(&inline("[text][ref]\n\n[ref]: https://example.com"));
        assert!(
            sp.iter().any(|s| s.text == "text" && s.style.link),
            "expected link-styled reference text in {:?}",
            sp.iter().map(|s| &s.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn image_reference_shows_dim_placeholder() {
        let sp = spans(&inline("![alt][ref]\n\n[ref]: https://example.com/i.png"));
        assert!(
            sp.iter()
                .any(|s| s.text.contains("[image: alt]") && s.style.dim),
            "expected dim image placeholder in {:?}",
            sp.iter().map(|s| &s.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn footnote_reference_shows_dim_marker() {
        let sp = spans(&inline("a[^1]\n\n[^1]: the note"));
        assert!(
            sp.iter().any(|s| s.text.contains("[^1]") && s.style.dim),
            "expected dim footnote marker in {:?}",
            sp.iter().map(|s| &s.text).collect::<Vec<_>>()
        );
    }
}
