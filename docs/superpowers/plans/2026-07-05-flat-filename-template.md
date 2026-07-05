# Flat Filename Template Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace noki's cryptic, panic-prone `%`-strftime filename templates with a flat, frontmatter-projection syntax — every `{field}` token resolves a meta value (`{title}`, `{author}`, `{created:%Y/%m/%d}`, `{labels}`, any custom key) — which also delivers "use any meta value in the filename" for free.

**Architecture:** A new, self-contained `src/template.rs` engine parses `{field}` / `{field:format}` tokens and resolves them through a caller-supplied closure, slugifying string values and chrono-formatting date values, and returning an `anyhow::Result` (never panicking) on unknown fields or bad formats. `note.rs::note_path` becomes a thin adapter that builds the resolver from a note's title, timestamp, labels, and static config meta. The command layer threads labels + config meta into `note_path` and propagates the `Result`.

**Tech Stack:** Rust 2024, `anyhow`, `chrono` (strftime, used *only* inside date tokens), `slug`, `toml` — all already in the tree. No new dependencies.

## Global Constraints

- **No `unwrap()` / `expect()` / `panic!` / `unreachable!` in non-test code.** The template engine and `note_path` return `anyhow::Result`; bad templates are errors, never panics. Tests may `unwrap()`.
- **Errors use `anyhow::Result` with `.context(...)`** where useful; the engine uses `anyhow::bail!` for template errors.
- **Public API at the top of each file, private helpers at the bottom.**
- **No new dependency** — reuse `slug`, `chrono`, `toml`.
- **TDD:** write the failing test, run it red, implement, run it green, commit.
- **Lint gate before every commit:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings` must pass.
- **`cargo test` / `cargo clippy` do NOT rebuild `target/debug/noki`** — run `cargo build` before manually exercising the binary.
- **Token grammar (flat, frontmatter projection):**
  - `{title}` → the note title (slugified).
  - `{created:<fmt>}` / `{updated:<fmt>}` → the timestamp, chrono-formatted; `<fmt>` optional, default `%Y-%m-%d`.
  - `{labels}` → labels joined and slugified (`work meeting` → `work-meeting`).
  - `{<any-key>}` → a static config meta value (e.g. `{author}`), stringified and slugified.
  - `{{` / `}}` → literal `{` / `}`.
  - A **missing value** — an unknown field name, or a field whose value is empty/unslugifiable (`{author}` with no `author` meta, `{labels}` with no labels) — renders as `unknown-<name>` (slugified), e.g. `unknown-author`. This keeps a token from ever producing an empty path segment.
  - **Errors** (never panics) are reserved for template *syntax* mistakes: a `:format` on a non-date field, an invalid date format, or an unterminated `{`.
  - `note_path` appends `.md`.
- **New defaults:** `DEFAULT_FILENAME = "{created:%Y/%m/%d/%H-%M-%S}-{title}"` (note `-` not `:` in the time — path-safe), `DEFAULT_DAILY_FILENAME = "{created:%Y/%m/%d}"`.
- **Out of scope:** `note.daily_title` stays on chrono `%` (it produces a human title, not a path). Note this in docs; do not change it.

## File Structure

- Create `src/template.rs` — the flat template engine: `Field` enum, `render()`, token parsing, slugify + date-format handling. One responsibility: string → string with `{field}` interpolation.
- Modify `src/lib.rs` — declare `mod template;`.
- Modify `src/note.rs` — rewrite `note_path` as a `Result`-returning adapter over the engine; add `resolve_field`/`meta_value_string`; update `DEFAULT_FILENAME`/`DEFAULT_DAILY_FILENAME`; update tests.
- Modify `src/commands/create.rs` — `build_note` returns `Result<Option<…>>`; thread `labels` + `config.note.meta` into `note_path`; update `save_note` and tests.
- Modify `src/commands/daily.rs` — `daily_path` returns `Result<String>`; thread meta; propagate in `run`; update tests.
- Modify `README.md` and `CLAUDE.md` — document the new token syntax.

---

## Task 1: Template engine (`src/template.rs`)

A standalone, panic-safe `{field}` / `{field:format}` renderer, tested with plain closures (no Note/Config needed).

**Files:**
- Create: `src/template.rs`
- Modify: `src/lib.rs` (add `mod template;`)

**Interfaces:**
- Produces:
  - `pub(crate) enum Field { Text(String), Date(chrono::DateTime<chrono::FixedOffset>) }`
  - `pub(crate) fn render(template: &str, resolve: impl Fn(&str) -> Option<Field>) -> anyhow::Result<String>`

- [ ] **Step 1: Write the failing tests**

Create `src/template.rs` with only the test module:

```rust
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
        let out = render("notes/{title}", |_| Some(Field::Text("hi there".to_string()))).unwrap();
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
        let err = render("{created:%q}", |_| Some(Field::Date(when))).unwrap_err();
        assert!(err.to_string().contains("invalid date format"), "got: {err}");
    }

    #[test]
    fn unterminated_token_is_an_error() {
        let err = render("{title", |_| Some(Field::Text("x".to_string()))).unwrap_err();
        assert!(err.to_string().contains("unterminated"), "got: {err}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib template`
Expected: FAIL — module not wired up / `cannot find function render` (a compile error counts as red).

- [ ] **Step 3: Write the implementation**

Prepend above the test module in `src/template.rs` (public API first, private helpers last):

```rust
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
```

> Note on the invalid-format test: `%q` is not a chrono specifier, so `StrftimeItems` yields an `Item::Error`. If the installed chrono ever accepted `%q`, the test would fail — swap it for another unrecognized specifier (e.g. `%J`). Do not "fix" it by loosening the assertion.

- [ ] **Step 4: Wire the module (with a temporary dead-code allow)**

In `src/lib.rs`, add after `pub mod output;`:

```rust
mod template;
```

The engine isn't called from non-test code until Task 2, so `cargo clippy --all-targets` will flag `render`/`Field` as dead in the lib target. Add a temporary allow at the very top of `src/template.rs` (Task 2 removes it):

```rust
#![allow(dead_code)]
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib template`
Expected: PASS (9 tests).

- [ ] **Step 6: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: clean (the `#![allow(dead_code)]` keeps the yet-unused public items quiet).

- [ ] **Step 7: Commit**

```bash
git add src/template.rs src/lib.rs
git commit -m "feat(template): add flat {field} filename template engine"
```

---

## Task 2: `note_path` over the engine (`src/note.rs`)

Rewrite `note_path` as a `Result`-returning adapter that resolves the flat fields from a note's title, timestamp, labels, and static config meta, and switch the default templates to the new syntax.

**Files:**
- Modify: `src/note.rs` (`note_path`, the two `DEFAULT_*FILENAME` consts, tests)
- Modify: `src/template.rs` (remove the temporary `#![allow(dead_code)]`)

**Interfaces:**
- Consumes: `template::{render, Field}` (Task 1).
- Produces: `pub fn note_path(template: &str, title: &str, labels: &[String], meta: &std::collections::BTreeMap<String, toml::Value>, when: chrono::DateTime<chrono::FixedOffset>) -> anyhow::Result<String>`

- [ ] **Step 1: Update the tests (they define the new behavior)**

In `src/note.rs`, the existing `note_path` tests use the old signature and `:`-time default. Replace `note_path_expands_date_and_slugged_title` and `note_path_daily_template_has_no_title`, and add two new tests. The final set of `note_path` tests is:

```rust
    #[test]
    fn note_path_expands_date_and_slugged_title() {
        let when = at("2026-06-02T10:00:00+01:00");
        let path = note_path(DEFAULT_FILENAME, "My new note", &[], &BTreeMap::new(), when).unwrap();
        assert_eq!(path, "2026/06/02/10-00-00-my-new-note.md");
    }

    #[test]
    fn note_path_daily_template_has_no_title() {
        let when = at("2026-07-03T09:00:00+02:00");
        let path = note_path(DEFAULT_DAILY_FILENAME, "", &[], &BTreeMap::new(), when).unwrap();
        assert_eq!(path, "2026/07/03.md");
    }

    #[test]
    fn note_path_interpolates_meta_and_labels() {
        let when = at("2026-06-02T10:00:00+01:00");
        let mut meta = BTreeMap::new();
        meta.insert(
            "author".to_string(),
            toml::Value::String("Paul van der Meijs".to_string()),
        );
        let labels = vec!["Work".to_string(), "Meeting".to_string()];
        let path = note_path("{author}/{labels}/{title}", "My Note", &labels, &meta, when).unwrap();
        assert_eq!(path, "paul-van-der-meijs/work-meeting/my-note.md");
    }

    #[test]
    fn note_path_missing_meta_defaults_to_unknown() {
        let when = at("2026-06-02T10:00:00+01:00");
        // `author` isn't in meta, and there are no labels → `unknown-<field>`.
        let path =
            note_path("{author}/{labels}/{title}", "My Note", &[], &BTreeMap::new(), when).unwrap();
        assert_eq!(path, "unknown-author/unknown-labels/my-note.md");
    }
```

(The `at` helper and `use super::*;` already exist in the `note.rs` test module; `BTreeMap` is already imported at the top of `note.rs`.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib note::tests::note_path`
Expected: FAIL — arity mismatch / old `:`-time default (compile error or wrong value counts as red).

- [ ] **Step 3: Update the defaults and rewrite `note_path`**

In `src/note.rs`, change the two filename constants (leave `DEFAULT_DAILY_TITLE` and `DEFAULT_DAILY_LABEL` as they are):

```rust
pub const DEFAULT_FILENAME: &str = "{created:%Y/%m/%d/%H-%M-%S}-{title}";
pub const DEFAULT_DAILY_FILENAME: &str = "{created:%Y/%m/%d}";
```

Replace the current `note_path` function body:

```rust
pub fn note_path(template: &str, title: &str, when: DateTime<FixedOffset>) -> String {
    let slug = slug::slugify(title);
    let with_title = template.replace("%title", &slug);
    format!("{}.md", when.format(&with_title))
}
```

with:

```rust
/// Render a relative note path from a flat template (`{field}` / `{field:fmt}`)
/// against the note's `title`, `labels`, static config `meta`, and timestamp
/// `when`; the `.md` extension is appended. Errors on an invalid template.
pub fn note_path(
    template: &str,
    title: &str,
    labels: &[String],
    meta: &BTreeMap<String, toml::Value>,
    when: DateTime<FixedOffset>,
) -> Result<String> {
    let rendered = crate::template::render(template, |name| {
        resolve_field(name, title, labels, meta, when)
    })?;
    Ok(format!("{rendered}.md"))
}
```

Add these private helpers at the **bottom** of `src/note.rs`, just above the `#[cfg(test)]` module (keeping the public-API-first convention):

```rust
/// Resolve a template field for `note_path`: built-in `title`/`created`/
/// `updated`/`labels`, else a static config meta value. An absent key returns
/// `None`, which the engine renders as `unknown-<field>`.
fn resolve_field(
    name: &str,
    title: &str,
    labels: &[String],
    meta: &BTreeMap<String, toml::Value>,
    when: DateTime<FixedOffset>,
) -> Option<crate::template::Field> {
    use crate::template::Field;
    match name {
        "title" => Some(Field::Text(title.to_string())),
        "created" | "updated" => Some(Field::Date(when)),
        "labels" => Some(Field::Text(labels.join(" "))),
        other => meta.get(other).map(|value| Field::Text(meta_value_string(value))),
    }
}

/// Stringify a TOML meta value for use in a path (slugified later by the engine).
fn meta_value_string(value: &toml::Value) -> String {
    match value {
        toml::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
```

`note.rs` already imports `anyhow::Result` and `chrono::{DateTime, FixedOffset}` and `std::collections::BTreeMap`; no new `use` lines are needed. The old `slug::slugify(...)` / `when.format(...)` usage is removed (the engine handles both now).

- [ ] **Step 4: Remove the temporary dead-code allow**

In `src/template.rs`, delete the `#![allow(dead_code)]` line added in Task 1 (the engine is now used from non-test code).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib note`
Expected: PASS (all `note` tests, including the four `note_path` tests).

- [ ] **Step 6: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: clean, no `dead_code` allow remaining.

- [ ] **Step 7: Commit**

```bash
git add src/note.rs src/template.rs
git commit -m "feat(note): render filenames from flat frontmatter templates"
```

---

## Task 3: Thread meta + labels through the callers (`create.rs`, `daily.rs`)

`note_path` now needs `labels` + config `meta` and returns `Result`. Update the two callers and their tests.

**Files:**
- Modify: `src/commands/create.rs` (`build_note`, `save_note`, tests)
- Modify: `src/commands/daily.rs` (`daily_path`, `run`, tests)

**Interfaces:**
- Consumes: `note::note_path(template, title, labels, meta, when) -> Result<String>` (Task 2).
- Produces: `build_note(...) -> anyhow::Result<Option<(String, String)>>`; `daily_path(&Config, DateTime<FixedOffset>) -> anyhow::Result<String>` (private).

- [ ] **Step 1: Update `create.rs` tests to the new signatures/paths**

In `src/commands/create.rs`, `build_note` will return `Result<Option<…>>` and the default path loses its `:`-time. Update each affected test call:

- `build_note_returns_none_for_empty_content`:
  ```rust
  assert!(build_note("   \n", &config, None, &[], now()).unwrap().is_none());
  ```
- `build_note_produces_path_and_frontmatter`:
  ```rust
  let (path, raw) =
      build_note("# My new note\n\nHello, World!", &config, None, &[], now())
          .unwrap()
          .unwrap();
  assert_eq!(path, "2026/06/02/10-00-00-my-new-note.md");
  ```
  (leave the two following `note.meta.title` / `note.content` assertions unchanged)
- `build_note_uses_custom_title_over_content`:
  ```rust
  let (path, raw) = build_note(
      "# Content Heading\n\nbody",
      &config,
      Some("Custom Title"),
      &[],
      now(),
  )
  .unwrap()
  .unwrap();
  assert_eq!(path, "2026/06/02/10-00-00-custom-title.md");
  ```
- `build_note_falls_back_to_content_title_when_none`:
  ```rust
  let (_, raw) = build_note("# Real Title\n\nbody", &config, None, &[], now())
      .unwrap()
      .unwrap();
  ```
- `build_note_falls_back_when_title_is_blank`:
  ```rust
  let (_, raw) =
      build_note("# Real Title\n\nbody", &config, Some("   "), &[], now())
          .unwrap()
          .unwrap();
  ```
- `build_note_sets_labels_from_arguments`:
  ```rust
  let (_, raw) = build_note("body", &config, None, &labels, now())
      .unwrap()
      .unwrap();
  ```
- `build_note_trims_and_drops_blank_labels`:
  ```rust
  let (_, raw) = build_note("body", &config, None, &labels, now())
      .unwrap()
      .unwrap();
  ```
- `build_note_merges_static_meta_but_ignores_reserved_keys` (the call near the end of that test):
  ```rust
  let (_, raw) = build_note("# Real Title\n\nbody", &config, None, &[], now())
      .unwrap()
      .unwrap();
  ```

Leave the `save_note_*` tests unchanged — `save_note` keeps its signature and its existing `.unwrap().unwrap()` / `.unwrap().is_none()` calls.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib commands::create`
Expected: FAIL — `build_note` still returns `Option` (type/arity mismatch is red).

- [ ] **Step 3: Update `build_note` and `save_note`**

In `src/commands/create.rs`, change `build_note`'s signature and body. Replace:

```rust
pub(crate) fn build_note(
    content: &str,
    config: &Config,
    title: Option<&str>,
    labels: &[String],
    now: DateTime<FixedOffset>,
) -> Option<(String, String)> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }

    let title = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| note::title_from_content(content));
    let template = config
        .note
        .filename
        .as_deref()
        .unwrap_or(note::DEFAULT_FILENAME);
    let path = note::note_path(template, &title, now);

    let note = assemble_note(path.clone(), title, content, config, labels, now);
    let raw = note::to_raw(&note).ok()?;
    Some((path, raw))
}
```

with:

```rust
pub(crate) fn build_note(
    content: &str,
    config: &Config,
    title: Option<&str>,
    labels: &[String],
    now: DateTime<FixedOffset>,
) -> Result<Option<(String, String)>> {
    let content = content.trim();
    if content.is_empty() {
        return Ok(None);
    }

    let title = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| note::title_from_content(content));
    let template = config
        .note
        .filename
        .as_deref()
        .unwrap_or(note::DEFAULT_FILENAME);
    let path = note::note_path(template, &title, labels, &config.note.meta, now)?;

    let note = assemble_note(path.clone(), title, content, config, labels, now);
    let raw = note::to_raw(&note)?;
    Ok(Some((path, raw)))
}
```

Then update `save_note` to unwrap the `Result`. Replace:

```rust
    match build_note(content, config, title, labels, now) {
        None => {
            eprintln!("Skipping empty note.");
            Ok(None)
        }
        Some((path, raw)) => {
```

with:

```rust
    match build_note(content, config, title, labels, now)? {
        None => {
            eprintln!("Skipping empty note.");
            Ok(None)
        }
        Some((path, raw)) => {
```

- [ ] **Step 4: Run to verify `create` passes**

Run: `cargo test --lib commands::create`
Expected: PASS.

- [ ] **Step 5: Update `daily.rs` tests**

In `src/commands/daily.rs`:

- `daily_path_uses_default_template`:
  ```rust
  assert_eq!(daily_path(&config, now()).unwrap(), "2026/07/03.md");
  ```
- `daily_path_uses_configured_template` (the old value used `%`-strftime, which is now literal text — switch to a token):
  ```rust
  let mut config = Config::default();
  config.note.daily_filename = Some("journal/{created:%Y-%m-%d}".to_string());
  assert_eq!(daily_path(&config, now()).unwrap(), "journal/2026-07-03.md");
  ```

Leave `daily_title_*` tests unchanged (that template stays chrono-based).

- [ ] **Step 6: Update `daily_path` and its call site**

In `src/commands/daily.rs`, replace `daily_path`:

```rust
fn daily_path(config: &Config, now: DateTime<FixedOffset>) -> String {
    let template = config
        .note
        .daily_filename
        .as_deref()
        .unwrap_or(note::DEFAULT_DAILY_FILENAME);
    note::note_path(template, "", now)
}
```

with (also drop the now-stale `%title` comment):

```rust
/// Today's daily-note path from `note.daily_filename` (default `{created:%Y/%m/%d}`).
fn daily_path(config: &Config, now: DateTime<FixedOffset>) -> Result<String> {
    let template = config
        .note
        .daily_filename
        .as_deref()
        .unwrap_or(note::DEFAULT_DAILY_FILENAME);
    note::note_path(template, "", &[], &config.note.meta, now)
}
```

Then in `run`, propagate the `Result` — change:

```rust
    let path = daily_path(config, now);
```

to:

```rust
    let path = daily_path(config, now)?;
```

- [ ] **Step 7: Run the full suite + lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all green.

- [ ] **Step 8: Manually exercise the real binary**

Remember `cargo test` does not rebuild the binary.

Run:
```bash
cargo build
printf 'A quick idea about templates.' | cargo run -- --no-edit --title "Template idea" --repository /tmp/noki-tpl-repo 2>&1 | tail -1
```
Expected: a printed path like `2026/07/05/14-31-07-template-idea.md` (date hierarchy with `-`-separated time, slugged title, `.md`). (First run clones/creates the repo dir; the exact date/time reflects now.)

- [ ] **Step 9: Commit**

```bash
git add src/commands/create.rs src/commands/daily.rs
git commit -m "feat(commands): pass labels and meta into filename templates"
```

---

## Task 4: Documentation

Update the user- and agent-facing docs that describe the filename template, per the repo's doc-sync rule.

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update the README config example**

In `README.md`, the `[note]` config block currently shows `filename = "%Y/%m/%d/%H:%M:%S-%title"` and `daily_filename = "%Y/%m/%d"`. Replace those two lines with the new syntax and add a one-line pointer to the token list:

```toml
[note]
# Filename templates use {field} tokens drawn from the note's frontmatter:
# {title}, {labels}, {created:%Y/%m/%d}, {updated:...}, and any custom meta key
# (e.g. {author}). Date tokens take a chrono strftime format (default %Y-%m-%d).
# A missing or empty value renders as "unknown-<field>" (e.g. unknown-author).
filename = "{created:%Y/%m/%d/%H-%M-%S}-{title}"
daily_filename = "{created:%Y/%m/%d}"
daily_title = "Daily note for %Y-%m-%d"
daily_label = "daily"
```

Keep the existing `max_width` and `meta` lines in that block as they are. Note in the comment/prose (near this block or the existing template prose, if any) that `daily_title` still uses chrono `%` directly because it is a title, not a path.

- [ ] **Step 2: Update CLAUDE.md's description**

In `CLAUDE.md`, the Output paragraph describes the old scheme: *"`create` derives the filename from a template (`%Y/%m/%d/%H:%M:%S-%title`, `%title` = slugified title) …"*. Replace that clause with an accurate description of the flat engine, e.g.:

> `create` derives the filename from a flat template rendered by `src/template.rs`: `{field}` / `{field:format}` tokens projected from the note's frontmatter — `{title}` and `{labels}` (slugified), `{created:%Y/%m/%d}` / `{updated:…}` (chrono-formatted), and any static config meta key (e.g. `{author}`). A missing or empty value renders as `unknown-<field>`; only template *syntax* errors (bad date format, `:format` on a text field, unterminated `{`) return `Err` — it never panics. The default is `{created:%Y/%m/%d/%H-%M-%S}-{title}`; `note_path` appends `.md`. `note.daily_title` still uses chrono `%` directly (it is a title, not a path).

Match the surrounding wording/length; keep it a factual description of the architecture.

- [ ] **Step 3: Check the bundled skills for drift**

Run: `grep -rniE '%title|%Y|filename template|note_path' skills/`
Expected: no hits that describe the filename template syntax. If any skill documents it, update it to the `{field}` syntax; if there are no hits, the skills need no change (they operate on paths returned by `ls`/`show`, not on templates).

- [ ] **Step 4: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: document the flat {field} filename template syntax"
```

---

## Self-Review

**1. Spec coverage:**
- "Improve filename template format" → Tasks 1–3 replace `%`-strftime with a flat, panic-safe `{field}` engine; Task 2 sets new defaults. ✅
- Confirmed decision: **flat** style (not namespaced) → tokens are bare `{title}`/`{author}`/… resolved as a frontmatter projection (Task 2 `resolve_field`). ✅
- Confirmed decision: **defer `{uuid}`** → no uniqueness helper, no new dependency (Global Constraints; nothing adds a crate). ✅
- Bonus: "allow any meta value in the filename" (the sibling todo) → falls out of `resolve_field`'s `other => meta.get(other)` arm. ✅
- Panic-safety of templates (no-panic rule) → `render`/`note_path` return `Result`; `format_date` validates via `StrftimeItems` (Task 1). ✅
- Confirmed refinement: **missing/empty values render as `unknown-<field>`**, not errors → `resolve_token`'s `None` and empty-slug branches call `placeholder()` (Task 1); covered by `missing_field_defaults_to_unknown_placeholder` + `empty_value_defaults_to_unknown_placeholder` (Task 1) and `note_path_missing_meta_defaults_to_unknown` (Task 2). Errors remain only for syntax mistakes. ✅
- Doc-sync rule → Task 4 (README + CLAUDE.md; skills checked). ✅

**2. Placeholder scan:** No "TBD"/"handle edge cases"/"write tests for the above". Every step has complete code or an exact command. The only temporary is `#![allow(dead_code)]` in `template.rs` (Task 1 Step 4 → removed in Task 2 Step 4), explicitly called out.

**3. Type consistency:**
- `Field { Text(String), Date(DateTime<FixedOffset>) }` and `render(&str, impl Fn(&str) -> Option<Field>) -> Result<String>` are used identically in Task 1 (definition), Task 2 (`resolve_field` returns `Option<Field>`, `note_path` calls `render`). ✅
- `note_path(template, title, labels, meta, when) -> Result<String>` — signature matches every call site: `create.rs` (`&title, labels, &config.note.meta, now`) and `daily.rs` (`"", &[], &config.note.meta, now`). ✅
- `build_note(...) -> Result<Option<(String, String)>>` — matched by `save_note`'s `build_note(...)?` and by every updated test (`.unwrap().unwrap()` / `.unwrap().is_none()`). ✅
- `daily_path(...) -> Result<String>` — matched by `run`'s `daily_path(config, now)?` and the two updated tests' `.unwrap()`. ✅
- `meta` type is `&BTreeMap<String, toml::Value>` in `note_path`, `resolve_field`, and both call sites pass `&config.note.meta` (which is `BTreeMap<String, toml::Value>`, per `config.rs`). ✅

---

**Plan complete and saved to `docs/superpowers/plans/2026-07-05-flat-filename-template.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

**Which approach?**
