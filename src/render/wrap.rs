use crate::render::inline::{Span, Style};

/// Greedy word-wrap a run of spans to at most `width` visible columns per line.
/// Words break only at spaces; a `"\n"` span forces a new line; a word longer
/// than `width` overflows onto its own line rather than being split. Widths are
/// plain char counts — spans carry no ANSI.
pub(crate) fn wrap(spans: &[Span], width: usize) -> Vec<Vec<Span>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut line: Vec<Span> = Vec::new();
    let mut line_width = 0usize;
    let mut word: Vec<Span> = Vec::new();
    let mut word_width = 0usize;

    for span in spans {
        let style = span.style;
        let mut chunk = String::new();
        for ch in span.text.chars() {
            match ch {
                ' ' => {
                    push_chunk(&mut chunk, style, &mut word, &mut word_width);
                    place_word(
                        &mut lines,
                        &mut line,
                        &mut line_width,
                        &mut word,
                        &mut word_width,
                        width,
                    );
                }
                '\n' => {
                    push_chunk(&mut chunk, style, &mut word, &mut word_width);
                    place_word(
                        &mut lines,
                        &mut line,
                        &mut line_width,
                        &mut word,
                        &mut word_width,
                        width,
                    );
                    lines.push(std::mem::take(&mut line));
                    line_width = 0;
                }
                _ => chunk.push(ch),
            }
        }
        push_chunk(&mut chunk, style, &mut word, &mut word_width);
    }
    place_word(
        &mut lines,
        &mut line,
        &mut line_width,
        &mut word,
        &mut word_width,
        width,
    );
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn push_chunk(chunk: &mut String, style: Style, word: &mut Vec<Span>, word_width: &mut usize) {
    if chunk.is_empty() {
        return;
    }
    *word_width += chunk.chars().count();
    word.push(Span {
        text: std::mem::take(chunk),
        style,
    });
}

fn place_word(
    lines: &mut Vec<Vec<Span>>,
    line: &mut Vec<Span>,
    line_width: &mut usize,
    word: &mut Vec<Span>,
    word_width: &mut usize,
    width: usize,
) {
    if *word_width == 0 {
        return;
    }
    if !line.is_empty() && *line_width + 1 + *word_width > width {
        lines.push(std::mem::take(line));
        *line_width = 0;
    }
    if !line.is_empty() {
        line.push(Span {
            text: " ".to_string(),
            style: Style::default(),
        });
        *line_width += 1;
    }
    line.append(word);
    *line_width += *word_width;
    *word_width = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::inline::{Span, Style};

    fn plain(text: &str) -> Vec<Span> {
        vec![Span {
            text: text.into(),
            style: Style::default(),
        }]
    }

    fn line_text(line: &[Span]) -> String {
        line.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn breaks_at_width() {
        let lines = wrap(&plain("aaa bbb ccc"), 7);
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "aaa bbb");
        assert_eq!(line_text(&lines[1]), "ccc");
    }

    #[test]
    fn fits_on_one_line() {
        let lines = wrap(&plain("a b c"), 80);
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "a b c");
    }

    #[test]
    fn hard_break_forces_new_line() {
        let spans = vec![
            Span {
                text: "a".into(),
                style: Style::default(),
            },
            Span {
                text: "\n".into(),
                style: Style::default(),
            },
            Span {
                text: "b".into(),
                style: Style::default(),
            },
        ];
        let lines = wrap(&spans, 80);
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "a");
        assert_eq!(line_text(&lines[1]), "b");
    }

    #[test]
    fn overlong_word_gets_its_own_line() {
        let lines = wrap(&plain("hi supercalifragilistic bye"), 6);
        // "hi" | "supercalifragilistic" | "bye"
        assert_eq!(lines.len(), 3);
        assert_eq!(line_text(&lines[1]), "supercalifragilistic");
    }

    #[test]
    fn keeps_styled_word_intact() {
        // A single styled word must never be split, even mid-style.
        let spans = vec![
            Span {
                text: "un".into(),
                style: Style::default(),
            },
            Span {
                text: "bold".into(),
                style: Style {
                    bold: true,
                    ..Style::default()
                },
            },
        ];
        let lines = wrap(&spans, 3);
        assert_eq!(lines.len(), 1, "no space, so it is one word");
        assert_eq!(line_text(&lines[0]), "unbold");
    }
}
