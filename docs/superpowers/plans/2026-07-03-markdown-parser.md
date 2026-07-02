# Markdown Parser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Noki's hand-rolled frontmatter split and first-line title heuristic with a real CommonMark parser (markdown-rs), so note parsing is robust against body `---` lines, code fences, and setext headings, and can read both YAML and TOML frontmatter.

**Architecture:** `src/note.rs` gains a private `parse_options()` that enables markdown-rs's `frontmatter` construct. `parse_note` parses the raw note to an mdast AST, reads the leading `Yaml`/`Toml` frontmatter node (deserializing its raw text with `serde_yaml_ng`/`toml` into `Meta`), and takes the body as the raw substring after the frontmatter node's byte offset. `title_from_content` parses the body to an AST and returns the first heading's text, else the first paragraph's first line, else `"untitled"`. `to_raw` still writes YAML and is unchanged; Noki writes YAML and reads either format.

**Tech Stack:** Rust (edition 2024), `markdown` 1.0 (markdown-rs, CommonMark + GFM, AST + frontmatter), `serde_yaml_ng` (existing), `toml` (existing), `anyhow`.

## Global Constraints

These apply to every task.

- Rust edition `2024`.
- Add exactly one dependency: `markdown = "1.0"`. `toml` (1.1) and `serde_yaml_ng` (0.10) are already in `Cargo.toml`; reuse them. Do not add `gray_matter`, `pulldown-cmark`, `comrak`, or a terminal renderer (rendering is a separate future plan).
- Every error path uses `anyhow::Result`. `markdown::to_mdast` returns `Result<Node, markdown::message::Message>`; `Message` implements only `Display` (not `std::error::Error`), so convert it with `.map_err(|error| anyhow::anyhow!("Invalid note markdown: {error}"))?` — never `?` it directly.
- No `unwrap()`/`expect()` in non-test code (tests may use them freely). No `unreachable!`/`panic!` either — handle the match's impossible arm by returning an error.
- Code organization: public API at the **top** of the file, private helpers at the **bottom**.
- No em dashes in user-facing strings.
- TDD: write the failing test first, watch it fail, implement the minimum, watch it pass, commit.
- Every task's final verification must pass `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` before committing.
- `cargo test`/`cargo clippy` do NOT rebuild the `target/debug/noki` binary. Any manual CLI check must run `cargo build` (or `cargo run`) first, or it exercises a stale binary.
- Commit after every task with a Conventional Commit message.

## API reference (verified against markdown 1.0.0)

The implementer does not need to rediscover these — they are confirmed working:

- `markdown::to_mdast(input: &str, &markdown::ParseOptions) -> Result<markdown::mdast::Node, markdown::message::Message>`.
- Enable frontmatter: `ParseOptions { constructs: Constructs { frontmatter: true, ..Constructs::gfm() }, ..ParseOptions::gfm() }`.
- `Node::Root(Root { children: Vec<Node>, .. })`.
- Frontmatter nodes: `Node::Yaml(Yaml { value: String, position: Option<Position> })` and `Node::Toml(Toml { value: String, position: Option<Position> })`. `value` is the frontmatter text WITHOUT the `---`/`+++` fences.
- `node.position() -> Option<&markdown::unist::Position>`; `position.end.offset` is a `usize` byte offset into the original input.
- Body nodes: `Node::Heading(Heading { depth: u8, children: Vec<Node>, .. })`, `Node::Paragraph(Paragraph { children: Vec<Node>, .. })`, `Node::Text(Text { value: String, .. })`, `Node::InlineCode(InlineCode { value: String, .. })`.
- `node.children() -> Option<&Vec<Node>>` for walking inline children generically.
- Frontmatter is only recognized at the very start of the document, so a `---` thematic break in the body is never mistaken for a fence.

---

### Task 1: Parse frontmatter with markdown-rs (YAML and TOML)

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/note.rs` (rewrite `parse_note`; add private `parse_options`, `node_end`, `body_after`)
- Test: `src/note.rs` (in the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `Meta`, `Note` (existing structs, unchanged); `to_raw` (existing, unchanged).
- Produces:
  - `pub fn parse_note(raw: &str) -> anyhow::Result<Note>` (same signature as before; now AST-backed, reads YAML or TOML frontmatter).
  - `fn parse_options() -> markdown::ParseOptions` (private; reused by Task 2).

- [ ] **Step 1: Add the markdown-rs dependency**

Run: `cargo add markdown@1.0`
Expected: `Cargo.toml` gains `markdown = "1.0"` and it resolves to `markdown v1.0.0`.

- [ ] **Step 2: Write the failing tests**

The existing tests `parses_frontmatter_and_body`, `round_trips_through_to_raw`, and `missing_frontmatter_is_an_error` stay as-is (they must keep passing). Add these new tests to the `#[cfg(test)] mod tests` block in `src/note.rs`:

```rust
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
        assert!(note.content.contains("Above"), "content was: {:?}", note.content);
        assert!(note.content.contains("---"), "content was: {:?}", note.content);
        assert!(note.content.contains("Below"), "content was: {:?}", note.content);
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib note`
Expected: `reads_toml_frontmatter` and `body_thematic_break_is_not_frontmatter` FAIL — the old string-splitting `parse_note` does not understand `+++` and would truncate the body at the first `\n---\n`.

- [ ] **Step 4: Rewrite `parse_note` and add the private helpers**

In `src/note.rs`, update the imports at the top of the file to include markdown-rs and `anyhow!`:

```rust
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, FixedOffset};
use markdown::mdast::Node;
use markdown::{Constructs, ParseOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
```

Replace the entire body of `parse_note` (keep its doc comment and signature) with:

```rust
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
```

Then, in the private section at the **bottom** of the file (above `pub(crate) mod rfc3339` and the `#[cfg(test)]` module), add:

```rust
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
```

Delete the old string-splitting logic from the previous `parse_note` (the `strip_prefix("---\n")` / `find("\n---\n")` code).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib note`
Expected: PASS — the two new tests plus all pre-existing note tests (`parses_frontmatter_and_body`, `round_trips_through_to_raw`, `missing_frontmatter_is_an_error`, the title and `note_path` tests) are green.

- [ ] **Step 6: Run the full gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: formatting clean, no clippy warnings, all tests pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/note.rs
git commit -m "feat: parse note frontmatter with markdown-rs (YAML and TOML)"
```

---

### Task 2: Extract the title from the markdown AST

**Files:**
- Modify: `src/note.rs` (rewrite `title_from_content`; add private `inline_text`)
- Test: `src/note.rs` (in the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `parse_options` (from Task 1).
- Produces:
  - `pub fn title_from_content(content: &str) -> String` (same signature; now AST-backed: first heading, else first paragraph's first line, else `"untitled"`).
  - `fn inline_text(nodes: &[Node]) -> String` (private; concatenates the plain text of inline nodes, stripping markup).

- [ ] **Step 1: Write the failing tests**

The existing test `title_uses_first_non_empty_line_without_heading_marks` stays and must keep passing. Add these to the `#[cfg(test)] mod tests` block in `src/note.rs`:

```rust
    #[test]
    fn title_reads_setext_heading() {
        assert_eq!(title_from_content("Setext Title\n=====\n\nbody"), "Setext Title");
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
        assert_eq!(title_from_content("First line\nsecond line\n"), "First line");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib note`
Expected: `title_ignores_heading_inside_code_fence` and `title_strips_inline_markup_from_heading` FAIL under the old heuristic (it takes the literal first non-empty line, so it returns `"not a title"` for the code-fence case and `"Hello **world**"` for the markup case).

- [ ] **Step 3: Rewrite `title_from_content` and add `inline_text`**

In `src/note.rs`, replace the body of `title_from_content` (keep its doc comment and signature) with:

```rust
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
```

Add `inline_text` to the private section at the **bottom** of the file (next to `parse_options`):

```rust
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
```

Delete the old `content.lines()...trim_start_matches('#')` implementation.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib note`
Expected: PASS — the four new title tests plus `title_uses_first_non_empty_line_without_heading_marks` (which still holds: `"# My new note\n\nbody"` -> `"My new note"`, `"plain title"` -> `"plain title"`, `"   "` -> `"untitled"`) and every other note test.

- [ ] **Step 5: Run the full gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: clean formatting, no clippy warnings, all tests pass (including the `commands::create` tests that call `title_from_content` via `build_note`).

- [ ] **Step 6: Commit**

```bash
git add src/note.rs
git commit -m "feat: extract note title from the markdown AST"
```

---

### Task 3: Integration verification against the real binary

**Files:**
- None changed (verification only). If a regression is found, fix it in `src/note.rs` and note it in the commit.

**Interfaces:**
- Consumes: `parse_note`, `title_from_content` (Tasks 1-2), and the existing `create`/`list`/`show` commands.
- Produces: nothing new; this task is the end-to-end gate that both parser changes compose correctly in the actual CLI.

- [ ] **Step 1: Rebuild the binary and run the whole suite**

Run: `cargo build && cargo test`
Expected: the binary builds and the full test suite passes (note, config, vcs, output, commands, cli). `cargo build` is required because `cargo test` alone does not refresh `target/debug/noki`.

- [ ] **Step 2: Smoke-test title extraction end-to-end**

Run:

```bash
TMP=$(mktemp -d)
git init "$TMP/remote"
git -C "$TMP/remote" commit --allow-empty -m init
BIN=$(pwd)/target/debug/noki
printf '# My Heading\n\nsome body\n' | "$BIN" -n --repository "$TMP/remote"
printf '```\n# fenced not a title\n```\n\nActual body only\n' | "$BIN" -n --repository "$TMP/remote"
"$BIN" ls --repository "$TMP/remote"
```

Expected: the first note's row shows title `My Heading`; the second note's row shows title `Actual body only` (the fenced `# fenced not a title` is correctly ignored, and no note is titled `fenced not a title`). Both create commands print a dated path and exit 0 (a push warning to stderr is expected against a non-bare local remote). If either title is wrong, that is a real regression — fix `title_from_content` and re-run.

- [ ] **Step 3: Smoke-test `show` on a note with a body `---`**

Run:

```bash
"$BIN" show "$( "$BIN" ls --json --repository "$TMP/remote" | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["meta"]["path"])' )" --repository "$TMP/remote"
```

Expected: `show` renders the note's metadata table and its body without error. (This confirms `parse_note` round-trips a real stored note through the new parser.)

- [ ] **Step 4: Commit (only if a regression fix was needed)**

If Steps 2-3 required a fix:

```bash
git add src/note.rs
git commit -m "fix: correct markdown parser regression found in integration"
```

Otherwise, this task adds no commit; record in the execution notes that integration passed clean.

---

## Self-Review

**Spec coverage:**
- Replace hand-rolled frontmatter split with a real parser: Task 1 (`parse_note` via markdown-rs, body via byte offset).
- Read both YAML and TOML frontmatter: Task 1 (`Node::Yaml` -> `serde_yaml_ng`, `Node::Toml` -> `toml`; `reads_toml_frontmatter` test).
- Robust against body `---`: Task 1 (`body_thematic_break_is_not_frontmatter` test; frontmatter is only recognized at document start).
- Replace the weak title heuristic: Task 2 (AST-based; setext, code-fence, inline-markup, paragraph-fallback tests).
- `to_raw` keeps writing YAML: unchanged in every task; the `round_trips_through_to_raw` test guards it.
- Verified end-to-end in the real binary: Task 3.

**Placeholder scan:** No `TODO`/`implement later`. Every code step contains complete, compile-ready code verified against markdown 1.0.0 in a spike (frontmatter node values, `position.end.offset` body slicing, heading/paragraph/inline-code extraction, `toml` + serde `flatten` deserialization).

**Type consistency:** `parse_note(&str) -> Result<Note>` and `title_from_content(&str) -> String` keep their existing signatures, so all call sites (`commands::create::build_note`, `commands::mod::load_notes`, `commands::show::run`) are unaffected. `parse_options() -> ParseOptions` is defined in Task 1 and consumed in Task 2. `node_end`/`body_after`/`inline_text` are private to `note.rs`. `Meta`/`Note`/`to_raw`/`rfc3339`/`note_path`/`DEFAULT_FILENAME` are untouched.

**Deferred (out of scope, by request):** terminal rendering of the markdown body (bold headers, etc.) via termimad or a custom mdast renderer — a separate future plan. Native TOML datetime values in frontmatter are not supported (dates must be RFC 3339 strings, matching what Noki writes); only relevant to hand-authored TOML notes.
