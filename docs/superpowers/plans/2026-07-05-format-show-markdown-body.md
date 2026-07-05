# Format the Markdown body in `show` human output — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render a note's Markdown body as styled, width-wrapped terminal text in the human output of `noki show`, instead of dumping the raw Markdown source.

**Architecture:** A new self-contained `src/render/` module turns a Markdown string into a terminal string, built by hand on the **markdown-rs AST already in the tree** (no new parser) and reusing **`tabled`** (already a dependency) for Markdown tables. The renderer flattens inline nodes into *plain-text spans that each carry their fully-resolved style*, so wrapping measures plain char widths and ANSI is only emitted at the very end — this structurally eliminates the "inner reset wipes the outer style" nesting bug and the "ANSI codes break wrap width" bug. `output::render_note_human` calls the renderer only when color is on; when color is off (piped / non-terminal) it keeps emitting the **raw Markdown source**, unchanged. `noki show` reads the terminal width via the tiny `terminal_size` crate (falls back to 80).

**Tech Stack:** Rust 2024, `markdown` (markdown-rs, `mdast`), `tabled` (with `ansi` feature), `terminal_size`, `anyhow`.

## Global Constraints

- **No `unwrap()` / `expect()` / `panic!` / `unreachable!` in non-test code.** Use `let ... else`, `map_or`, `saturating_sub`, and graceful fallbacks. Tests may `unwrap()` freely.
- **Public API at the top of each file, private helpers at the bottom.**
- **Errors use `anyhow::Result` with `.context(...)`** where fallible. The renderer itself is infallible (returns `String`, falling back to the raw input on parse failure).
- **TDD:** write the failing test, run it red, implement, run it green, commit.
- **Lint gate before every commit:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings` must pass.
- **`cargo test` / `cargo clippy` do NOT rebuild `target/debug/noki`.** Run `cargo build` before manually exercising the binary.
- **No new dependency beyond `terminal_size`.** Reuse `markdown` and `tabled` (already present; `tabled` already has the `ansi` feature).
- **Keep the agent skills in sync with the CLI** (per CLAUDE.md) — Task 7 covers the doc touch-up.
- **Piped / non-terminal output stays raw Markdown** (the confirmed behavior): the renderer runs only when `color == true`.

## File Structure

- Create `src/render/mod.rs` — public entry `render(markdown, width, color) -> String`; block-level rendering (headings, paragraphs, lists, code, blockquotes, thematic breaks, HTML).
- Create `src/render/inline.rs` — `Style`, `Span`, `spans()` (flatten inline AST → styled plain-text spans), `emit()` (spans → ANSI or plain string).
- Create `src/render/wrap.rs` — `wrap()` (greedy word-wrap a span run to N visible columns; never splits a styled word; honors hard breaks).
- Create `src/render/table.rs` — `render_table()` (mdast `Table` → a `tabled` table, reusing the existing box-drawing style, with per-column alignment).
- Modify `src/lib.rs` — declare `mod render;`.
- Modify `src/output.rs` — `render_note_human` gains a `width` param; renders the body when `color`, keeps raw when not; update existing tests + add two.
- Modify `src/commands/show.rs` — add `terminal_width()`; pass it to `render_note_human`.
- Modify `Cargo.toml` — add `terminal_size`.
- Modify `skills/retrieving-notes/SKILL.md` — note the body is now rendered Markdown.

---

## Task 1: Inline styling (`src/render/inline.rs`)

Flatten the inline AST into a flat list of spans. Each span holds **plain text plus its fully-resolved style** (a `Strong` wrapping an `Emphasis` produces spans that are already `bold && italic`). This is the design that makes everything downstream trivial: wrapping measures `text.chars().count()`, and `emit` wraps each span in a self-contained `SGR … \x1b[0m` pair with no cross-span state to preserve.

**Files:**
- Create: `src/render/inline.rs`
- Also touched: `src/render/mod.rs` (created in Task 3) will declare `mod inline;`. For this task, temporarily declare the module so it compiles/tests — see Step 6.

**Interfaces:**
- Produces:
  - `pub(crate) struct Style { pub bold: bool, pub italic: bool, pub strike: bool, pub underline: bool, pub dim: bool, pub code: bool, pub link: bool }` (derives `Clone, Copy, Default, PartialEq, Eq`)
  - `pub(crate) struct Span { pub text: String, pub style: Style }` (derives `Clone`)
  - `pub(crate) fn spans(nodes: &[markdown::mdast::Node]) -> Vec<Span>`
  - `pub(crate) fn emit(spans: &[Span], color: bool) -> String`

- [ ] **Step 1: Write the failing tests**

Create `src/render/inline.rs` with only the test module (the code under test comes in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use markdown::ParseOptions;
    use markdown::mdast::Node;

    /// Inline children of the first paragraph in `md`.
    fn inline(md: &str) -> Vec<Node> {
        let tree = markdown::to_mdast(md, &ParseOptions::gfm()).unwrap();
        let Node::Root(root) = tree else { panic!("no root") };
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
        assert!(sp.iter().any(|s| s.text.contains("https://e.com") && s.style.dim));
    }

    #[test]
    fn soft_break_becomes_space() {
        let sp = spans(&inline("a\nb"));
        let joined: String = sp.iter().map(|s| s.text.as_str()).collect();
        assert!(!joined.contains('\n'), "soft break should be a space: {joined:?}");
    }

    #[test]
    fn emit_wraps_bold_in_ansi() {
        let out = emit(
            &[Span { text: "hi".into(), style: Style { bold: true, ..Style::default() } }],
            true,
        );
        assert!(out.contains("\x1b[1m"), "expected bold SGR in {out:?}");
        assert!(out.contains("hi"));
        assert!(out.ends_with("\x1b[0m"));
    }

    #[test]
    fn emit_plain_has_no_ansi() {
        let out = emit(
            &[Span { text: "hi".into(), style: Style { bold: true, ..Style::default() } }],
            false,
        );
        assert_eq!(out, "hi");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib render::inline`
Expected: FAIL — the module isn't wired up yet (`error[E0432]`/`cannot find function spans`). (If `render` isn't declared yet, this is a compile error, which counts as red.)

- [ ] **Step 3: Write the implementation**

Prepend this above the test module in `src/render/inline.rs` (public API first, private helpers last, per the repo rule):

```rust
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
        let codes = if color { sgr_codes(span.style) } else { Vec::new() };
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
            Node::Text(t) => out.push(Span { text: t.value.replace('\n', " "), style: base }),
            Node::InlineCode(c) => {
                let mut style = base;
                style.code = true;
                out.push(Span { text: c.value.replace('\n', " "), style });
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
                out.push(Span { text: format!(" ({})", l.url), style: url_style });
            }
            Node::Image(i) => {
                let mut style = base;
                style.dim = true;
                out.push(Span { text: format!("[image: {}]", i.alt), style });
            }
            Node::Break(_) => out.push(Span { text: "\n".to_string(), style: base }),
            Node::Html(h) => {
                let mut style = base;
                style.dim = true;
                out.push(Span { text: h.value.replace('\n', " "), style });
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
```

- [ ] **Step 4: Wire the module so tests can run**

The `render` module doesn't exist yet. Add a minimal declaration to `src/lib.rs` so `inline.rs` compiles now; Task 3 fleshes out `render/mod.rs`.

In `src/lib.rs`, add after line 8 (`pub mod output;`):

```rust
mod render;
```

Create `src/render/mod.rs` with just the submodule declaration for now:

```rust
mod inline;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib render::inline`
Expected: PASS (7 tests).

- [ ] **Step 6: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: clean. (Clippy may warn that `render` items are unused — that resolves once Task 3 uses them. If it errors on dead code now, add `#![allow(dead_code)]` at the top of `src/render/mod.rs` and REMOVE it in Task 4 Step 6. Note this reminder.)

- [ ] **Step 7: Commit**

```bash
git add src/render/inline.rs src/render/mod.rs src/lib.rs
git commit -m "feat(render): flatten inline markdown into styled spans"
```

---

## Task 2: Word wrapping (`src/render/wrap.rs`)

Greedy word-wrap a span run to at most `width` visible columns. Widths are plain char counts (spans carry no ANSI). A styled word is never split across lines; a `"\n"` span forces a line break; an over-long word overflows onto its own line rather than being hard-split.

**Files:**
- Create: `src/render/wrap.rs`
- Modify: `src/render/mod.rs` (add `mod wrap;`)

**Interfaces:**
- Consumes: `Span`, `Style` from `super::inline` (Task 1).
- Produces: `pub(crate) fn wrap(spans: &[Span], width: usize) -> Vec<Vec<Span>>` — each inner `Vec<Span>` is one line, words separated by single `" "` spans, no trailing space.

- [ ] **Step 1: Write the failing tests**

Create `src/render/wrap.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::inline::{Span, Style};

    fn plain(text: &str) -> Vec<Span> {
        vec![Span { text: text.into(), style: Style::default() }]
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
            Span { text: "a".into(), style: Style::default() },
            Span { text: "\n".into(), style: Style::default() },
            Span { text: "b".into(), style: Style::default() },
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
            Span { text: "un".into(), style: Style::default() },
            Span { text: "bold".into(), style: Style { bold: true, ..Style::default() } },
        ];
        let lines = wrap(&spans, 3);
        assert_eq!(lines.len(), 1, "no space, so it is one word");
        assert_eq!(line_text(&lines[0]), "unbold");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib render::wrap`
Expected: FAIL — `cannot find function wrap`.

- [ ] **Step 3: Write the implementation**

Prepend above the test module in `src/render/wrap.rs`:

```rust
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
                    place_word(&mut lines, &mut line, &mut line_width, &mut word, &mut word_width, width);
                }
                '\n' => {
                    push_chunk(&mut chunk, style, &mut word, &mut word_width);
                    place_word(&mut lines, &mut line, &mut line_width, &mut word, &mut word_width, width);
                    lines.push(std::mem::take(&mut line));
                    line_width = 0;
                }
                _ => chunk.push(ch),
            }
        }
        push_chunk(&mut chunk, style, &mut word, &mut word_width);
    }
    place_word(&mut lines, &mut line, &mut line_width, &mut word, &mut word_width, width);
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
    word.push(Span { text: std::mem::take(chunk), style });
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
        line.push(Span { text: " ".to_string(), style: Style::default() });
        *line_width += 1;
    }
    line.append(word);
    *line_width += *word_width;
    *word_width = 0;
}
```

- [ ] **Step 4: Register the submodule**

In `src/render/mod.rs`, add under the existing `mod inline;`:

```rust
mod wrap;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib render::wrap`
Expected: PASS (5 tests).

- [ ] **Step 6: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: clean (keep the temporary `#![allow(dead_code)]` from Task 1 if it was added).

- [ ] **Step 7: Commit**

```bash
git add src/render/wrap.rs src/render/mod.rs
git commit -m "feat(render): add ANSI-safe word wrapping over spans"
```

---

## Task 3: Block rendering (`src/render/mod.rs`)

Wire inline + wrap into the public `render()` entry and render every common block type: headings, paragraphs, bullet/ordered/task lists (with nesting), fenced/indented code, blockquotes, thematic breaks, HTML. Tables are handled in Task 4 — for now the `Table` arm renders an empty string (placeholder).

**Files:**
- Modify: `src/render/mod.rs`

**Interfaces:**
- Consumes: `spans`, `emit`, `Span`, `Style` from `inline`; `wrap` from `wrap`.
- Produces: `pub fn render(markdown: &str, width: usize, color: bool) -> String`.

- [ ] **Step 1: Write the failing tests**

Add a test module at the bottom of `src/render/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn heading_is_bold() {
        let out = render("# Hi", 80, true);
        assert!(out.contains("\x1b[1m"), "expected bold in {out:?}");
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib render::tests`
Expected: FAIL — `cannot find function render`.

- [ ] **Step 3: Write the implementation**

Replace the current contents of `src/render/mod.rs` (the two `mod` lines) with the following, keeping the test module at the bottom. Public API (`render`) first, private block helpers below, submodule declarations at the very top:

```rust
mod inline;
mod table;
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
        style: Style { dim: true, ..Style::default() },
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
    decorate_lines(&c.value, color, |line| format!("\x1b[2m    {line}\x1b[0m"))
        .lines()
        .map(|line| {
            if color {
                line.to_string()
            } else {
                format!("    {line}")
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
    let line = "─".repeat(width.min(80).max(1));
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
        .map(|(i, line)| if i == 0 { format!("{first}{line}") } else { format!("{rest}{line}") })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Apply `decorate` to each line when `color`, else return the raw lines joined.
fn decorate_lines(text: &str, color: bool, decorate: impl Fn(&str) -> String) -> String {
    text.split('\n')
        .map(|line| if color { decorate(line) } else { line.to_string() })
        .collect::<Vec<_>>()
        .join("\n")
}
```

> Note: `code()` above is written to indent by four spaces in both color and no-color paths (the colored path folds the indent into the dim decoration; the plain path adds it). If you find the double-pass awkward, the equivalent simpler form is acceptable as long as `code_block_is_indented` passes:
>
> ```rust
> fn code(c: &Code, color: bool) -> String {
>     c.value
>         .split('\n')
>         .map(|line| {
>             let text = format!("    {line}");
>             if color { format!("\x1b[2m{text}\x1b[0m") } else { text }
>         })
>         .collect::<Vec<_>>()
>         .join("\n")
> }
> ```
>
> Prefer this simpler form; delete the `decorate_lines`-based `code` if you use it, but keep `decorate_lines` (it is still used by the `Html` arm).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib render::tests`
Expected: PASS (10 tests).

- [ ] **Step 5: Run the full suite**

Run: `cargo test --lib render`
Expected: PASS (inline + wrap + block tests).

- [ ] **Step 6: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: clean. (If you added `#![allow(dead_code)]` earlier, `render_table` is still unused until Task 4 — leave the allow for now; it is removed in Task 4.)

- [ ] **Step 7: Commit**

```bash
git add src/render/mod.rs
git commit -m "feat(render): render markdown block structure to terminal"
```

---

## Task 4: Markdown tables via `tabled` (`src/render/table.rs`)

Render a GFM table by reusing `tabled` (already in the tree with the `ansi` feature). Each cell's inline content is styled through `emit`; `tabled`'s `ansi` feature measures the *visible* width, so inline styling inside cells never breaks column alignment. Per-column alignment comes from the AST's `align`. Header styling mirrors the existing list/meta tables.

**Files:**
- Create: `src/render/table.rs`
- Modify: `src/render/mod.rs` (swap the `Node::Table` arm to call `table::render_table`; remove any temporary `#![allow(dead_code)]`)

**Interfaces:**
- Consumes: `emit`, `spans` from `super::inline`; `markdown::mdast::{Table, TableRow, TableCell, AlignKind, Node}`.
- Produces: `pub(crate) fn render_table(table: &markdown::mdast::Table, color: bool) -> String`.

- [ ] **Step 1: Write the failing tests**

Create `src/render/table.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use crate::render::render;

    const TABLE: &str = "| Name | Qty |\n| :--- | ---: |\n| Apples | 3 |\n| Pears | 12 |\n";

    #[test]
    fn renders_headers_and_cells() {
        let out = render(TABLE, 80, false);
        assert!(out.contains("Name"), "in {out:?}");
        assert!(out.contains("Qty"), "in {out:?}");
        assert!(out.contains("Apples"), "in {out:?}");
        assert!(out.contains("12"), "in {out:?}");
    }

    #[test]
    fn draws_box_borders() {
        let out = render(TABLE, 80, false);
        assert!(out.contains('│') || out.contains('─'), "expected borders in {out:?}");
    }

    #[test]
    fn bold_header_when_color() {
        let out = render(TABLE, 80, true);
        assert!(out.contains("\x1b[1m"), "expected bold header in {out:?}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib render::table`
Expected: FAIL — the `Node::Table` arm returns `""` (placeholder), so `renders_headers_and_cells` fails on the missing "Name".

- [ ] **Step 3: Write the implementation**

Prepend above the test module in `src/render/table.rs`:

```rust
use markdown::mdast::{AlignKind, Node, Table};
use tabled::Table as TabledTable;
use tabled::builder::Builder;
use tabled::settings::object::{Columns, Rows};
use tabled::settings::style::HorizontalLine;
use tabled::settings::{Alignment, Color, Modify, Style};

use super::inline::{emit, spans};

/// Render a GFM table as a bordered `tabled` table. Cells are inline-styled via
/// `emit`; `tabled`'s `ansi` feature keeps columns aligned despite ANSI codes.
pub(crate) fn render_table(table: &Table, color: bool) -> String {
    let mut builder = Builder::default();
    for row in &table.children {
        let Node::TableRow(row) = row else { continue };
        let cells: Vec<String> = row
            .children
            .iter()
            .map(|cell| match cell {
                Node::TableCell(cell) => emit(&spans(&cell.children), color).replace('\n', " "),
                _ => String::new(),
            })
            .collect();
        builder.push_record(cells);
    }
    let mut out = builder.build();
    apply_style(&mut out, &table.align, color);
    out.to_string()
}

fn apply_style(table: &mut TabledTable, align: &[AlignKind], color: bool) {
    let header_rule = HorizontalLine::inherit(Style::extended())
        .remove_intersection()
        .left('╞')
        .right('╡');
    table.with(
        Style::modern()
            .remove_vertical()
            .horizontals([(1, header_rule)]),
    );
    for (index, kind) in align.iter().enumerate() {
        let alignment = match kind {
            AlignKind::Left => Some(Alignment::left()),
            AlignKind::Center => Some(Alignment::center()),
            AlignKind::Right => Some(Alignment::right()),
            AlignKind::None => None,
        };
        if let Some(alignment) = alignment {
            table.with(Modify::new(Columns::single(index)).with(alignment));
        }
    }
    if color {
        table.with(Modify::new(Rows::first()).with(Color::BOLD));
    }
}
```

- [ ] **Step 4: Swap the block arm and drop the dead-code allow**

In `src/render/mod.rs`, change the placeholder arm:

```rust
        Node::Table(_) => String::new(), // implemented in Task 4
```

to:

```rust
        Node::Table(t) => table::render_table(t, color),
```

If you added `#![allow(dead_code)]` to `src/render/mod.rs` in an earlier task, remove it now.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib render`
Expected: PASS (all render tests, including the 3 new table tests).

- [ ] **Step 6: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: clean, with no `dead_code` allow remaining.

- [ ] **Step 7: Commit**

```bash
git add src/render/table.rs src/render/mod.rs
git commit -m "feat(render): render markdown tables with tabled"
```

---

## Task 5: Render the body in `output::render_note_human`

Call the renderer for the body when `color` is on; keep the raw Markdown source when it's off (piped/non-terminal). This adds a `width` parameter to `render_note_human`. Update the existing call site in `show.rs` to pass a constant `80` for now (Task 6 replaces it with the real terminal width) so the build stays green.

**Files:**
- Modify: `src/output.rs:13` (`render_note_human` signature + body composition), and its tests
- Modify: `src/commands/show.rs:21` (pass `80` temporarily)

**Interfaces:**
- Consumes: `crate::render::render` (Task 3).
- Produces: `pub fn render_note_human(note: &Note, width: usize, color: bool) -> String` (new `width` param, inserted before `color`).

- [ ] **Step 1: Write the failing tests**

In `src/output.rs`, add two tests inside the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn note_human_renders_body_when_color() {
        let raw = "---\ntitle: T\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\n# Heading\n";
        let note = parse_note(raw).unwrap();
        let text = render_note_human(&note, 80, true);
        assert!(text.contains("\x1b[1m"), "expected rendered bold heading in:\n{text}");
        assert!(text.contains("Heading"), "expected heading text in:\n{text}");
    }

    #[test]
    fn note_human_keeps_raw_body_when_no_color() {
        let raw = "---\ntitle: T\npath: p.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\n# Heading\n";
        let note = parse_note(raw).unwrap();
        let text = render_note_human(&note, 80, false);
        assert!(text.contains("# Heading"), "expected literal markdown source in:\n{text}");
    }
```

Also update the **existing** `render_note_human` call sites in this test module to pass the new `width` argument (`80`). They are in: `note_human_bold_header_column_when_color`, `note_human_shows_title_and_content`, `note_human_shows_extra_meta`, `note_human_colors_labels`, `note_human_without_color_omits_ansi`. For each, change `render_note_human(&note, true)` → `render_note_human(&note, 80, true)` and `render_note_human(&note, false)` → `render_note_human(&note, 80, false)`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib output`
Expected: FAIL — the two new tests fail (or the whole module fails to compile because of the arity change, which counts as red).

- [ ] **Step 3: Update the implementation**

In `src/output.rs`, change `render_note_human` (currently lines 13–30). Replace the signature line and the final `format!`:

```rust
/// Render a single note as a metadata table followed by its content. `color`
/// enables ANSI-colored label chips and a rendered Markdown body (disable for
/// non-terminal output, where the raw Markdown source is emitted instead).
/// `width` is the column budget for wrapping the rendered body.
pub fn render_note_human(note: &Note, width: usize, color: bool) -> String {
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
    let body = if color {
        crate::render::render(&note.content, width, true)
    } else {
        note.content.clone()
    };
    format!("{table}\n\n{body}")
}
```

- [ ] **Step 4: Fix the `show.rs` call site (temporary width)**

In `src/commands/show.rs`, line 21, change:

```rust
        output::render_note_human(&note, stdout().is_terminal())
```

to:

```rust
        output::render_note_human(&note, 80, stdout().is_terminal())
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib output && cargo test --lib show`
Expected: PASS.

- [ ] **Step 6: Run the full suite + lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all green.

- [ ] **Step 7: Commit**

```bash
git add src/output.rs src/commands/show.rs
git commit -m "feat(output): render markdown body in human show output"
```

---

## Task 6: Real terminal width in `noki show`

Replace the temporary `80` with the actual terminal width via `terminal_size` (adds ~1 crate; reuses `rustix`, already in the tree), falling back to 80 when the width can't be determined (e.g. piped).

**Files:**
- Modify: `Cargo.toml` (add `terminal_size`)
- Modify: `src/commands/show.rs` (add `terminal_width()`, use it)

**Interfaces:**
- Produces: `fn terminal_width() -> usize` (private to `show.rs`).

- [ ] **Step 1: Add the dependency**

In `Cargo.toml`, add under the existing dependencies (after `markdown = "1.0"`):

```toml
terminal_size = "0.4"
```

- [ ] **Step 2: Verify it builds and adds no heavy tree**

Run: `cargo build`
Expected: compiles. Optionally confirm the tree stayed lean:

Run: `cargo tree -i terminal_size`
Expected: `terminal_size` depends on `rustix` (already present) on this platform — no `crossterm`/event stack pulled in.

- [ ] **Step 3: Add the width helper and use it**

In `src/commands/show.rs`, add a private helper at the bottom of the file (above `#[cfg(test)] mod tests`), per the public-API-first rule:

```rust
/// The current terminal width in columns, or 80 when it can't be determined
/// (e.g. output is piped).
fn terminal_width() -> usize {
    terminal_size::terminal_size().map_or(80, |(terminal_size::Width(cols), _)| cols as usize)
}
```

Then change line 21 from:

```rust
        output::render_note_human(&note, 80, stdout().is_terminal())
```

to:

```rust
        output::render_note_human(&note, terminal_width(), stdout().is_terminal())
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib show`
Expected: PASS. (Tests call `run(...)` with a non-terminal stdout, so `color` is false and the body stays raw — `terminal_width()` is still called and must not panic.)

- [ ] **Step 5: Manually exercise the real binary**

Remember `cargo test` does NOT rebuild the binary.

Run:
```bash
cargo build
printf -- '---\ntitle: Demo\npath: demo.md\nlabels: [x]\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:00+01:00\n---\n\n# Hello\n\nSome **bold** and `code` and a [link](https://example.com).\n\n- one\n- two\n\n| A | B |\n| :- | -: |\n| 1 | 2 |\n' > /tmp/noki-demo.md
```
Then eyeball the rendered output through the library path (a quick throwaway, or run the binary against a repo containing such a note). Expected when attached to a terminal: bold "Hello", styled inline spans, bullet list, bordered table. Piped (`| cat`): raw Markdown source unchanged.

- [ ] **Step 6: Run the full suite + lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all green.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/commands/show.rs
git commit -m "feat(show): wrap rendered body to terminal width"
```

---

## Task 7: Keep the docs in sync

Per CLAUDE.md, the `retrieving-notes` skill documents `show`'s output shapes and must not drift. Its existing guidance already steers agents away from scraping the human table ("colored, width-dependent, truncates labels"); extend that warning to cover the now-rendered body so agents keep using `--json`/`--raw`.

**Files:**
- Modify: `skills/retrieving-notes/SKILL.md`

- [ ] **Step 1: Read the relevant lines**

Run: `sed -n '1,45p' skills/retrieving-notes/SKILL.md`
Locate the sentence (around line 8): *"Always drive it **structured** — the human table output is colored, width-dependent, and truncates labels, so parse `--json` instead of scraping the table."*

- [ ] **Step 2: Update the warning**

Edit that sentence so it also mentions the rendered body, e.g.:

> Always drive it **structured** — the human output is colored, width-dependent, truncates labels, and renders the Markdown body (wrapped, ANSI-styled), so parse `--json` (or `--raw` for the exact source) instead of scraping it.

Also review the `## Show one note` section (around lines 26–46): confirm the `--json` and `--raw` examples still describe the correct shapes (they are unchanged by this work — only the default human output changed). If any line implies the default human output is plain Markdown, fix it.

- [ ] **Step 3: Verify no other skill references the human body**

Run: `grep -rniE 'human|rendered|raw markdown|plain' skills/`
Expected: no remaining claim that `show`'s default output is raw/plain Markdown. Fix any that exist.

- [ ] **Step 4: Commit**

```bash
git add skills/retrieving-notes/SKILL.md
git commit -m "docs(skills): note that show renders the markdown body"
```

---

## Self-Review

**1. Spec coverage:**
- "Format the Markdown body for the human-readable `show` output" → Tasks 1–4 build the renderer; Task 5 wires it into `render_note_human`; Task 6 supplies the terminal width. ✅
- Confirmed decision: hand-rolled over markdown-rs AST, no new parser → Tasks 1–4 use `markdown::to_mdast` only. ✅
- Confirmed decision: reuse `tabled` for tables, nested styling safe → Task 4, cells styled via `emit`, `ansi` feature handles width. ✅
- Confirmed decision: single crate, `src/render/` module (`mod.rs`/`inline.rs`/`table.rs`/`wrap.rs`) → Tasks 1–4 create exactly those files. ✅
- Confirmed decision: piped / no-color → raw Markdown → Task 5 (`color` gates rendering). ✅
- CLAUDE.md skills-sync rule → Task 7. ✅

**2. Placeholder scan:** No "TBD"/"handle edge cases"/"write tests for the above" — every code and test step contains complete code. The only deliberate temporary is the `Node::Table => String::new()` placeholder in Task 3, explicitly replaced in Task 4 Step 4, and the constant `80` in Task 5 Step 4, explicitly replaced in Task 6 Step 3. Both are called out. ✅

**3. Type consistency:**
- `Style` / `Span` field names are identical across `inline.rs`, `wrap.rs`, `table.rs`, `mod.rs`. ✅
- `spans(&[Node]) -> Vec<Span>`, `emit(&[Span], bool) -> String`, `wrap(&[Span], usize) -> Vec<Vec<Span>>`, `render(&str, usize, bool) -> String`, `render_table(&Table, bool) -> String` — signatures match every call site. ✅
- `render_note_human(&Note, usize, bool)` — the new `width` param is threaded from `show.rs` (`80` in Task 5, `terminal_width()` in Task 6) and used in all updated tests. ✅
- mdast variant used is `Node::Blockquote` (markdown-rs spelling), matched in `block()`. Confirm against the installed `markdown` crate during Task 3 if the compiler disagrees — the fix is purely the variant name. ✅

---

**Plan complete and saved to `docs/superpowers/plans/2026-07-05-format-show-markdown-body.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

**Which approach?**
