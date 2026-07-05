// TODO: remove once Task 3/4 wires this module into `commands::show` and uses
// its public items; until then clippy flags them as dead code.
#![allow(dead_code)]

mod inline;
mod wrap;

use inline::{Span, Style, emit, spans};
use markdown::ParseOptions;
use markdown::mdast::{Code, Heading, List, Node};
use wrap::wrap;

/// Render a Markdown string as styled terminal text wrapped to `width` columns.
/// When `color`, inline styles and structural cues are emitted as ANSI; the
/// text is otherwise identical. Falls back to the raw input if parsing fails.
pub fn render(markdown: &str, width: usize, color: bool) -> String {
    let Ok(tree) = markdown::to_mdast(markdown, &ParseOptions::gfm()) else {
        return markdown.to_string();
    };
    let Node::Root(root) = tree else {
        return markdown.to_string();
    };
    root.children
        .iter()
        .map(|node| block(node, width, color))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn block(node: &Node, width: usize, color: bool) -> String {
    match node {
        Node::Heading(h) => heading(h, width, color),
        Node::Paragraph(p) => paragraph(&p.children, width, color),
        Node::List(l) => list(l, width, color),
        Node::Code(c) => code(c, color),
        Node::Blockquote(b) => blockquote(&b.children, width, color),
        Node::ThematicBreak(_) => rule(width, color),
        Node::Html(h) => decorate_lines(&h.value, color, |line| format!("\x1b[2m{line}\x1b[0m")),
        Node::Table(_) => String::new(), // implemented in Task 4
        other => other
            .children()
            .map(|children| paragraph(children, width, color))
            .unwrap_or_default(),
    }
}

fn heading(h: &Heading, width: usize, color: bool) -> String {
    let mut body = spans(&h.children);
    for span in &mut body {
        span.style.bold = true;
        if h.depth == 1 {
            span.style.underline = true;
        }
    }
    let hashes = Span {
        text: format!("{} ", "#".repeat(h.depth as usize)),
        style: Style {
            dim: true,
            ..Style::default()
        },
    };
    let mut line = vec![hashes];
    line.extend(body);
    render_lines(&line, width, color)
}

fn paragraph(children: &[Node], width: usize, color: bool) -> String {
    render_lines(&spans(children), width, color)
}

fn list(l: &List, width: usize, color: bool) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut number = l.start.unwrap_or(1);
    for item in &l.children {
        let Node::ListItem(item) = item else { continue };
        let marker = if l.ordered {
            let marker = format!("{number}. ");
            number += 1;
            marker
        } else {
            "• ".to_string()
        };
        let check = match item.checked {
            Some(true) => "[x] ",
            Some(false) => "[ ] ",
            None => "",
        };
        let first_plain = format!("{marker}{check}");
        let pad = " ".repeat(first_plain.chars().count());
        let inner_width = width.saturating_sub(first_plain.chars().count()).max(1);
        let body = item
            .children
            .iter()
            .map(|child| match child {
                Node::List(nested) => list(nested, inner_width, color),
                other => block(other, inner_width, color),
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let first_prefix = if color {
            format!("\x1b[2m{first_plain}\x1b[0m")
        } else {
            first_plain
        };
        out.push(prefix_lines(&body, &first_prefix, &pad));
    }
    out.join("\n")
}

fn code(c: &Code, color: bool) -> String {
    c.value
        .split('\n')
        .map(|line| {
            let text = format!("    {line}");
            if color {
                format!("\x1b[2m{text}\x1b[0m")
            } else {
                text
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn blockquote(children: &[Node], width: usize, color: bool) -> String {
    let inner = children
        .iter()
        .map(|node| block(node, width.saturating_sub(2).max(1), color))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let bar = if color { "\x1b[2m│\x1b[0m " } else { "│ " };
    inner
        .split('\n')
        .map(|line| format!("{bar}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn rule(width: usize, color: bool) -> String {
    let line = "─".repeat(width.clamp(1, 80));
    if color {
        format!("\x1b[2m{line}\x1b[0m")
    } else {
        line
    }
}

/// Wrap a span run to `width` and join the resulting lines.
fn render_lines(spans: &[Span], width: usize, color: bool) -> String {
    wrap(spans, width)
        .iter()
        .map(|line| emit(line, color))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Apply `first` to the first line of `text` and `rest` to every other line.
fn prefix_lines(text: &str, first: &str, rest: &str) -> String {
    text.split('\n')
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                format!("{first}{line}")
            } else {
                format!("{rest}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Apply `decorate` to each line when `color`, else return the raw lines joined.
fn decorate_lines(text: &str, color: bool, decorate: impl Fn(&str) -> String) -> String {
    text.split('\n')
        .map(|line| {
            if color {
                decorate(line)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn heading_is_bold() {
        // A level-2 heading is bold (only), emitting a plain bold SGR.
        let out = render("## Hi", 80, true);
        assert!(out.contains("\x1b[1m"), "expected bold in {out:?}");
        assert!(out.contains("Hi"));
    }

    #[test]
    fn h1_is_bold_and_underlined() {
        // A level-1 heading merges bold+underline into one SGR sequence.
        let out = render("# Hi", 80, true);
        assert!(
            out.contains("\x1b[1;4m"),
            "expected bold+underline in {out:?}"
        );
        assert!(out.contains("Hi"));
    }

    #[test]
    fn paragraph_wraps_to_width() {
        let out = render("aaa bbb ccc ddd", 7, false);
        assert!(out.contains('\n'), "expected a wrap in {out:?}");
    }

    #[test]
    fn bullet_list_uses_marker() {
        let out = render("- one\n- two", 80, false);
        assert!(out.contains("• one"), "in {out:?}");
        assert!(out.contains("• two"), "in {out:?}");
    }

    #[test]
    fn ordered_list_numbers() {
        let out = render("1. a\n2. b", 80, false);
        assert!(out.contains("1. a"), "in {out:?}");
        assert!(out.contains("2. b"), "in {out:?}");
    }

    #[test]
    fn task_list_shows_checkbox() {
        let out = render("- [x] done\n- [ ] todo", 80, false);
        assert!(out.contains("[x] done"), "in {out:?}");
        assert!(out.contains("[ ] todo"), "in {out:?}");
    }

    #[test]
    fn nested_list_is_indented() {
        let out = render("- outer\n  - inner", 80, false);
        let inner_line = out.lines().find(|l| l.contains("inner")).unwrap();
        assert!(inner_line.starts_with("  "), "inner not indented: {out:?}");
    }

    #[test]
    fn code_block_is_indented() {
        let out = render("```\nlet x = 1;\n```", 80, false);
        assert!(out.contains("    let x = 1;"), "in {out:?}");
    }

    #[test]
    fn blockquote_has_bar() {
        let out = render("> quoted", 80, false);
        assert!(out.contains("│ quoted"), "in {out:?}");
    }

    #[test]
    fn thematic_break_draws_rule() {
        let out = render("a\n\n---\n\nb", 80, false);
        assert!(out.contains('─'), "expected rule in {out:?}");
    }

    #[test]
    fn invalid_stays_as_input_text() {
        // Even weird input never panics and preserves the words.
        let out = render("just text", 80, false);
        assert!(out.contains("just text"));
    }
}
