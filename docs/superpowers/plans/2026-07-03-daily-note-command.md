# Daily Note (`--daily`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `--daily` flag so `noki --daily` opens (or creates) today's daily note at a path derived from `note.daily_filename` (default `%Y/%m/%d`).

**Architecture:** A new `commands::daily` module orchestrates the flow: compute today's path from the template, load the note if it exists, otherwise create it. It reuses the two save primitives already in the codebase — `edit::save_edit` for an existing note (refreshes `updated`, preserves `created`/title/labels) and a newly-extracted `create::assemble_note` builder for a new note. `--daily` is a top-level flag (like `--no-edit`/`--title`/`--label`) that only changes the no-subcommand (create) path. Piped input is appended to an existing daily note (blank-line separated); a new daily note with no `--title` is titled `Daily note for YYYY-MM-DD`.

**Tech Stack:** Rust, `clap` (CLI), `chrono` (dates), `anyhow` (errors), `serde`/`toml` (config). Storage goes through the existing `VersionControl` trait; tests drive the in-memory `MemoryBackend`.

## Global Constraints

- Lint gate (must pass before every commit): `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
- Errors use `anyhow::Result` with `.context(...)` throughout, including the library — deliberate, do not introduce `thiserror`.
- No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code. Tests may `unwrap()` freely.
- Public API at the top of each file, private helpers at the bottom.
- TDD: write the failing test, watch it fail, make it pass, commit.
- `cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki` — run `cargo build` before exercising the binary by hand.
- Settled design (do not revisit): the default new-daily-note title (when `--title` is absent) is `Daily note for %Y-%m-%d`; piped/no-edit input is **appended** to an existing daily note with a blank line between; an existing daily note keeps its own title/labels (edit semantics — `--title`/`--label` apply only when creating a new daily note); `note.daily_filename` must not contain `%title` (the daily path must be stable per day so an existing note can be found).

---

## File Structure

- `src/note.rs` — **modify.** Add `pub const DEFAULT_DAILY_FILENAME: &str = "%Y/%m/%d";` next to `DEFAULT_FILENAME`.
- `src/config.rs` — **modify.** Add `daily_filename: Option<String>` to `NoteConfig` and merge it.
- `src/commands/create.rs` — **modify.** Extract `assemble_note` (build a `Note` at an explicit path); have `build_note` delegate to it. No behavior change to `create`.
- `src/commands/daily.rs` — **new.** The daily-note command: `run` (entry point / IO glue) plus pure, testable helpers (`daily_path`, `default_daily_title`, `append_body`, `load_existing`, `save_new_daily`, `append_and_save`).
- `src/commands/mod.rs` — **modify.** Register `pub mod daily;`.
- `src/cli.rs` — **modify.** Add the `--daily` top-level flag.
- `src/main.rs` — **modify.** In the no-subcommand arm, dispatch to `commands::daily::run` when `--daily` is set.
- `README.md` — **modify.** Document `--daily` and `note.daily_filename`.

Task ordering keeps every commit green against the lint gate. Task 1 (config + const) and Task 2 (create refactor) are independent foundations. Task 3 (daily module) consumes both plus the existing `edit::save_edit`; the module's `pub` entry point means the binary stays untouched and exhaustive. Task 4 wires the CLI flag and the `main.rs` dispatch together. Task 5 is docs.

---

### Task 1: Config field `note.daily_filename` and the default template constant

**Files:**
- Modify: `src/note.rs:60` (add the constant after `DEFAULT_FILENAME`) and its `#[cfg(test)] mod tests`
- Modify: `src/config.rs:17-22` (`NoteConfig`), `src/config.rs:95-110` (`Config::merge`), and its `#[cfg(test)] mod tests`

**Interfaces:**
- Produces:
  - `pub const DEFAULT_DAILY_FILENAME: &str` in `crate::note` (value `"%Y/%m/%d"`).
  - `pub daily_filename: Option<String>` field on `crate::config::NoteConfig`, layered/merged like `filename`.

- [ ] **Step 1: Write the failing tests**

Add this test to the `#[cfg(test)] mod tests` in `src/note.rs` (the `at` helper already exists there):

```rust
    #[test]
    fn note_path_daily_template_has_no_title() {
        let when = at("2026-07-03T09:00:00+02:00");
        let path = note_path(DEFAULT_DAILY_FILENAME, "", when);
        assert_eq!(path, "2026/07/03.md");
    }
```

Add this test to the `#[cfg(test)] mod tests` in `src/config.rs`:

```rust
    #[test]
    fn parses_daily_filename() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".noki.toml"),
            "repository = \"r\"\n\n[note]\ndaily_filename = \"journal/%Y-%m-%d\"\n",
        )
        .unwrap();
        let config = load_from(None, dir.path(), None).unwrap();
        assert_eq!(
            config.note.daily_filename.as_deref(),
            Some("journal/%Y-%m-%d")
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib note::tests::note_path_daily_template_has_no_title config::tests::parses_daily_filename`
Expected: FAIL — `cannot find value DEFAULT_DAILY_FILENAME` in `note.rs`, and `no field daily_filename on type NoteConfig` in `config.rs`.

- [ ] **Step 3: Add the constant in `src/note.rs`**

Immediately after the existing `pub const DEFAULT_FILENAME` line (currently `src/note.rs:60`), add:

```rust
pub const DEFAULT_DAILY_FILENAME: &str = "%Y/%m/%d";
```

- [ ] **Step 4: Add the field and merge in `src/config.rs`**

Add the `daily_filename` field to `NoteConfig` (between `filename` and `meta`):

```rust
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct NoteConfig {
    pub filename: Option<String>,
    pub daily_filename: Option<String>,
    pub meta: BTreeMap<String, toml::Value>,
}
```

In `Config::merge`, add the merge branch right after the `filename` branch:

```rust
        if other.note.filename.is_some() {
            self.note.filename = other.note.filename;
        }
        if other.note.daily_filename.is_some() {
            self.note.daily_filename = other.note.daily_filename;
        }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib note::tests::note_path_daily_template_has_no_title config::tests::parses_daily_filename`
Expected: PASS (both).

- [ ] **Step 6: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0. (The new `pub` const and `pub` field are part of the library's API, so they raise no dead-code warnings while unused.)

- [ ] **Step 7: Commit**

```bash
git add src/note.rs src/config.rs
git commit -m "feat: add note.daily_filename config and default template"
```

---

### Task 2: Extract `create::assemble_note`

**Files:**
- Modify: `src/commands/create.rs:51-104` (`build_note`) — extract the assembly into `assemble_note`
- Test: inline `#[cfg(test)] mod tests` in `src/commands/create.rs`

**Interfaces:**
- Consumes: `crate::note::{Meta, Note, to_raw, title_from_content, note_path, DEFAULT_FILENAME}`, `crate::config::Config`.
- Produces: `pub(crate) fn assemble_note(path: String, title: String, content: &str, config: &Config, labels: &[String], now: DateTime<FixedOffset>) -> Note` — builds a `Note` at an explicit `path` with an already-resolved `title`, merging config static meta (minus reserved keys) and cleaning `labels`; sets `created` and `updated` both to `now`; appends a trailing newline to `content`. (Consumed by Task 3's daily create branch.)

- [ ] **Step 1: Write the failing test**

Add this test to the `#[cfg(test)] mod tests` in `src/commands/create.rs` (the `now()` helper already exists there):

```rust
    #[test]
    fn assemble_note_builds_at_explicit_path_with_given_title() {
        let mut config = Config::default();
        config.note.meta.insert(
            "author".to_string(),
            toml::Value::String("Paul".to_string()),
        );
        let note = assemble_note(
            "2026/07/03.md".to_string(),
            "Daily note for 2026-07-03".to_string(),
            "Body text",
            &config,
            &["work".to_string()],
            now(),
        );
        assert_eq!(note.meta.path, "2026/07/03.md");
        assert_eq!(note.meta.title, "Daily note for 2026-07-03");
        assert_eq!(note.content, "Body text\n");
        assert_eq!(note.meta.labels, vec!["work".to_string()]);
        assert_eq!(note.meta.created, now());
        assert_eq!(note.meta.updated, now());
        assert_eq!(
            note.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib commands::create::tests::assemble_note_builds_at_explicit_path_with_given_title`
Expected: FAIL — `cannot find function assemble_note in this scope`.

- [ ] **Step 3: Extract `assemble_note` and delegate from `build_note`**

Replace the existing `build_note` function (currently `src/commands/create.rs:51-104`) with the following two functions. `build_note` keeps its exact signature and behavior; only its internals change. Place `assemble_note` directly below `build_note`:

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

/// Assemble a `Note` at an explicit `path` with an already-resolved `title`.
/// Merges config static meta (skipping reserved keys), cleans `labels`, sets
/// both `created` and `updated` to `now`, and appends a trailing newline to the
/// (already-trimmed) `content`.
pub(crate) fn assemble_note(
    path: String,
    title: String,
    content: &str,
    config: &Config,
    labels: &[String],
    now: DateTime<FixedOffset>,
) -> Note {
    let mut extra = BTreeMap::new();
    for (key, value) in &config.note.meta {
        if RESERVED_META_KEYS.contains(&key.as_str()) {
            continue;
        }
        if let Ok(value) = serde_yaml_ng::to_value(value) {
            extra.insert(key.clone(), value);
        }
    }

    let meta = Meta {
        title,
        path,
        labels: labels
            .iter()
            .map(|label| label.trim())
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .collect(),
        created: now,
        updated: now,
        extra,
    };
    Note {
        meta,
        content: format!("{content}\n"),
    }
}
```

- [ ] **Step 4: Run the create tests to verify they all pass**

Run: `cargo test --lib commands::create`
Expected: PASS — the new `assemble_note_builds_at_explicit_path_with_given_title` plus every pre-existing `build_note`/`save_note` test (behavior is unchanged).

- [ ] **Step 5: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0. (`assemble_note` is used by `build_note`, so no dead-code warning.)

- [ ] **Step 6: Commit**

```bash
git add src/commands/create.rs
git commit -m "refactor: extract assemble_note from build_note"
```

---

### Task 3: The `daily` command module

**Files:**
- Create: `src/commands/daily.rs`
- Modify: `src/commands/mod.rs:1-3` (module declarations)
- Test: inline `#[cfg(test)] mod tests` in `src/commands/daily.rs`

**Interfaces:**
- Consumes:
  - `crate::note::{self, Note, parse_note, to_raw, note_path, DEFAULT_DAILY_FILENAME}` (Task 1 added the constant).
  - `crate::config::Config` with `config.note.daily_filename: Option<String>` (Task 1).
  - `crate::commands::create::assemble_note(path: String, title: String, content: &str, config: &Config, labels: &[String], now: DateTime<FixedOffset>) -> Note` (Task 2).
  - `crate::commands::edit::save_edit(vcs: &dyn VersionControl, note: Note, content: &str, now: DateTime<FixedOffset>) -> Result<Option<String>>` (already exists).
  - `crate::vcs::VersionControl`, `crate::io::read_stdin() -> Option<String>`, `crate::editor::get_content_from_editor(Option<String>) -> Result<String>`.
- Produces:
  - `pub fn run(vcs: &dyn VersionControl, config: &Config, no_edit: bool, title: Option<&str>, labels: &[String]) -> Result<()>` — the command entry point (consumed by Task 4).

- [ ] **Step 1: Register the module**

In `src/commands/mod.rs`, add the `daily` declaration alongside the others (keep them alphabetical):

```rust
pub mod create;
pub mod daily;
pub mod edit;
pub mod list;
pub mod show;
```

- [ ] **Step 2: Write the failing tests**

Create `src/commands/daily.rs` with ONLY the test module for now (it must fail to compile — that is the expected "red"):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::note::parse_note;
    use crate::vcs::{MemoryBackend, VersionControl};
    use chrono::DateTime;

    fn at(s: &str) -> DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn now() -> DateTime<chrono::FixedOffset> {
        at("2026-07-03T09:00:00+02:00")
    }

    const EXISTING: &str = "---\ntitle: Daily note for 2026-07-03\npath: 2026/07/03.md\nlabels: []\ncreated: 2026-07-03T08:00:00+02:00\nupdated: 2026-07-03T08:00:00+02:00\n---\n\nMorning notes\n";

    #[test]
    fn daily_path_uses_default_template() {
        let config = Config::default();
        assert_eq!(daily_path(&config, now()), "2026/07/03.md");
    }

    #[test]
    fn daily_path_uses_configured_template() {
        let mut config = Config::default();
        config.note.daily_filename = Some("journal/%Y-%m-%d".to_string());
        assert_eq!(daily_path(&config, now()), "journal/2026-07-03.md");
    }

    #[test]
    fn default_daily_title_formats_the_date() {
        assert_eq!(default_daily_title(now()), "Daily note for 2026-07-03");
    }

    #[test]
    fn append_body_separates_with_a_blank_line() {
        assert_eq!(
            append_body("Morning notes\n", "did X"),
            "Morning notes\n\ndid X"
        );
    }

    #[test]
    fn load_existing_returns_none_when_missing() {
        let backend = MemoryBackend::new();
        assert!(load_existing(&backend, "2026/07/03.md").unwrap().is_none());
    }

    #[test]
    fn load_existing_parses_present_note() {
        let backend = MemoryBackend::with_files(&[("2026/07/03.md", EXISTING)]);
        let note = load_existing(&backend, "2026/07/03.md").unwrap().unwrap();
        assert_eq!(note.meta.title, "Daily note for 2026-07-03");
        assert_eq!(note.content, "Morning notes\n");
    }

    #[test]
    fn save_new_daily_creates_note_with_date_title() {
        let backend = MemoryBackend::new();
        let config = Config::default();
        let path = save_new_daily(&backend, &config, "2026/07/03.md", "Hello", None, &[], now())
            .unwrap()
            .unwrap();
        assert_eq!(path, "2026/07/03.md");
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.meta.title, "Daily note for 2026-07-03");
        assert_eq!(saved.meta.created, now());
        assert_eq!(saved.meta.updated, now());
        assert_eq!(saved.content, "Hello\n");
    }

    #[test]
    fn save_new_daily_uses_explicit_title() {
        let backend = MemoryBackend::new();
        let config = Config::default();
        save_new_daily(
            &backend,
            &config,
            "2026/07/03.md",
            "Hello",
            Some("Standup"),
            &[],
            now(),
        )
        .unwrap()
        .unwrap();
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.meta.title, "Standup");
    }

    #[test]
    fn save_new_daily_skips_empty_content() {
        let backend = MemoryBackend::new();
        let config = Config::default();
        assert!(
            save_new_daily(&backend, &config, "2026/07/03.md", "   \n", None, &[], now())
                .unwrap()
                .is_none()
        );
        assert!(backend.list_files().unwrap().is_empty());
    }

    #[test]
    fn save_new_daily_applies_labels_and_config_meta() {
        let backend = MemoryBackend::new();
        let mut config = Config::default();
        config.note.meta.insert(
            "author".to_string(),
            toml::Value::String("Paul".to_string()),
        );
        save_new_daily(
            &backend,
            &config,
            "2026/07/03.md",
            "Hello",
            None,
            &["work".to_string()],
            now(),
        )
        .unwrap()
        .unwrap();
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.meta.labels, vec!["work".to_string()]);
        assert_eq!(
            saved.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
    }

    #[test]
    fn append_and_save_appends_and_bumps_updated() {
        let backend = MemoryBackend::with_files(&[("2026/07/03.md", EXISTING)]);
        let note = parse_note(EXISTING).unwrap();
        let path = append_and_save(&backend, note, "did X", now())
            .unwrap()
            .unwrap();
        assert_eq!(path, "2026/07/03.md");
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.content, "Morning notes\n\ndid X\n");
        assert_eq!(saved.meta.updated, now());
        assert_eq!(saved.meta.created, at("2026-07-03T08:00:00+02:00"));
        assert_eq!(saved.meta.title, "Daily note for 2026-07-03");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib commands::daily`
Expected: FAIL — compile error, functions (`daily_path`, `default_daily_title`, `append_body`, `load_existing`, `save_new_daily`, `append_and_save`) not found.

- [ ] **Step 4: Write the implementation**

Prepend the implementation ABOVE the test module in `src/commands/daily.rs` (public API `run` at the top, private helpers below it, tests at the bottom):

```rust
use crate::commands::{create, edit};
use crate::config::Config;
use crate::note::{self, Note};
use crate::vcs::VersionControl;
use anyhow::Result;
use chrono::{DateTime, FixedOffset, Local};

/// Open (or create) today's daily note. The note's path comes from
/// `note.daily_filename` (default `%Y/%m/%d`). An existing note is loaded and
/// updated (its `created`/title/labels preserved); a missing one is created.
/// Piped input is appended to an existing note and used as the body of a new one.
pub fn run(
    vcs: &dyn VersionControl,
    config: &Config,
    no_edit: bool,
    title: Option<&str>,
    labels: &[String],
) -> Result<()> {
    let now = Local::now().fixed_offset();
    let path = daily_path(config, now);
    let input = crate::io::read_stdin();

    if let Some(note) = load_existing(vcs, &path)? {
        if no_edit {
            match input {
                Some(piped) => {
                    append_and_save(vcs, note, &piped, now)?;
                }
                None => eprintln!("Nothing to add to today's note."),
            }
        } else {
            let prefill = match &input {
                Some(piped) => append_body(&note.content, piped),
                None => note.content.clone(),
            };
            let body = crate::editor::get_content_from_editor(Some(prefill))?;
            edit::save_edit(vcs, note, &body, now)?;
        }
        return Ok(());
    }

    let content = if no_edit {
        input.unwrap_or_default()
    } else {
        crate::editor::get_content_from_editor(input)?
    };
    save_new_daily(vcs, config, &path, &content, title, labels, now)?;
    Ok(())
}

/// Today's daily-note path from `note.daily_filename`. The daily template
/// carries no `%title`, so the title argument to `note_path` is unused.
fn daily_path(config: &Config, now: DateTime<FixedOffset>) -> String {
    let template = config
        .note
        .daily_filename
        .as_deref()
        .unwrap_or(note::DEFAULT_DAILY_FILENAME);
    note::note_path(template, "", now)
}

/// Load and parse the note at `path`, or `None` if there is none there.
fn load_existing(vcs: &dyn VersionControl, path: &str) -> Result<Option<Note>> {
    match vcs.read_file(path) {
        Ok(raw) => Ok(Some(note::parse_note(&raw)?)),
        Err(_) => Ok(None),
    }
}

/// Append `addition` below `existing`, separated by a blank line.
fn append_body(existing: &str, addition: &str) -> String {
    format!("{}\n\n{}", existing.trim_end(), addition.trim())
}

/// Append `addition` to an existing daily note and save it (refreshing
/// `updated`, preserving `created`/title/labels). Returns the note's path.
fn append_and_save(
    vcs: &dyn VersionControl,
    note: Note,
    addition: &str,
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    let body = append_body(&note.content, addition);
    edit::save_edit(vcs, note, &body, now)
}

/// Build and store a brand-new daily note at `path`, committing it as an "Add".
/// When `title` is absent, defaults to `Daily note for %Y-%m-%d`. Returns the
/// path, or `None` when the content is empty after trimming.
fn save_new_daily(
    vcs: &dyn VersionControl,
    config: &Config,
    path: &str,
    content: &str,
    title: Option<&str>,
    labels: &[String],
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    let content = content.trim();
    if content.is_empty() {
        eprintln!("Skipping empty note.");
        return Ok(None);
    }

    let title = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_daily_title(now));

    let note = create::assemble_note(path.to_string(), title, content, config, labels, now);
    let raw = note::to_raw(&note)?;
    vcs.write_file(path, &raw, &format!("Add note {path}"))?;
    println!("{path}");
    Ok(Some(path.to_string()))
}

/// The default title for a new daily note: `Daily note for %Y-%m-%d`.
fn default_daily_title(now: DateTime<FixedOffset>) -> String {
    format!("Daily note for {}", now.format("%Y-%m-%d"))
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib commands::daily`
Expected: PASS — all 10 daily tests pass.

- [ ] **Step 6: Run the full suite and lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass; lint gate produces no output, exit 0. (`daily::run` is `pub`, so it raises no dead-code warning while the binary does not yet call it.)

- [ ] **Step 7: Commit**

```bash
git add src/commands/daily.rs src/commands/mod.rs
git commit -m "feat: add daily-note command module"
```

---

### Task 4: Wire the `--daily` flag into the CLI

**Files:**
- Modify: `src/cli.rs:11-14` (add the flag near `no_edit`) and its `#[cfg(test)] mod tests`
- Modify: `src/main.rs:34-40` (the no-subcommand arm)

**Interfaces:**
- Consumes: `noki::commands::daily::run(backend.as_ref(), &config, cli.no_edit, cli.title.as_deref(), &cli.labels)` (Task 3).
- Produces: `Cli.daily: bool` field.

- [ ] **Step 1: Write the failing CLI parse tests**

Add these tests inside the existing `#[cfg(test)] mod tests` in `src/cli.rs`:

```rust
    #[test]
    fn parses_daily_flag() {
        let cli = Cli::parse_from(["noki", "--daily"]);
        assert!(cli.daily);
        assert!(cli.command.is_none());
    }

    #[test]
    fn daily_defaults_to_false() {
        let cli = Cli::parse_from(["noki"]);
        assert!(!cli.daily);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib cli::tests::parses_daily_flag cli::tests::daily_defaults_to_false`
Expected: FAIL — compile error, `no field daily on type Cli`.

- [ ] **Step 3: Add the `--daily` flag to `src/cli.rs`**

Add the field to the `Cli` struct, immediately after the `no_edit` field:

```rust
    /// Skip the editor and store piped input directly
    #[arg(short = 'n', long)]
    pub no_edit: bool,

    /// Open or create today's daily note (path from note.daily_filename)
    #[arg(short = 'd', long)]
    pub daily: bool,
```

- [ ] **Step 4: Dispatch `--daily` in `src/main.rs`**

Replace the `None =>` arm of the command `match` with one that branches on `cli.daily`:

```rust
        None => {
            if cli.daily {
                commands::daily::run(
                    backend.as_ref(),
                    &config,
                    cli.no_edit,
                    cli.title.as_deref(),
                    &cli.labels,
                )
            } else {
                commands::create::run(
                    backend.as_ref(),
                    &config,
                    cli.no_edit,
                    cli.title.as_deref(),
                    &cli.labels,
                )
            }
        }
```

- [ ] **Step 5: Run the CLI tests to verify they pass**

Run: `cargo test --lib cli::tests::parses_daily_flag cli::tests::daily_defaults_to_false`
Expected: PASS (both).

- [ ] **Step 6: Run the full suite and lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass; lint gate produces no output, exit 0.

- [ ] **Step 7: Exercise the binary end-to-end (manual smoke test)**

The editor and stdin paths cannot be unit-tested, so verify them by hand once against a scratch repository. Use a non-interactive fake editor (noki invokes `$EDITOR`/`$VISUAL` split on spaces with the target file appended as the last argument):

```bash
cargo build
REPO="$(mktemp -d)"
git -C "$REPO" init -q
git -C "$REPO" config user.name Test
git -C "$REPO" config user.email test@example.com
git -C "$REPO" commit -q --allow-empty -m init

FAKE="$(mktemp -d)/fakeeditor"
printf '#!/bin/sh\nprintf "Daily body\\n" > "$1"\n' > "$FAKE"
chmod +x "$FAKE"

# Create today's daily note via the fake editor:
unset VISUAL
EDITOR="$FAKE" ./target/debug/noki --daily --repository "$REPO" < /dev/null
# Note the printed path (today's date, e.g. 2026/07/03.md). Append via pipe:
echo "Second entry" | ./target/debug/noki --daily --no-edit --repository "$REPO"
# Inspect the result:
./target/debug/noki show "$(date +%Y/%m/%d).md" --repository "$REPO" --raw
```

Expected: the note's path is today's date under the default `%Y/%m/%d` template; the title is `Daily note for <today>`; after the pipe the body contains both `Daily body` and `Second entry` separated by a blank line; `updated` is later than `created`, and `created` is unchanged from the first write.

- [ ] **Step 8: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add --daily flag to open or create today's note"
```

---

### Task 5: Document `--daily`

**Files:**
- Modify: `README.md` (the config TOML example around lines 34-36, and the usage section after the labels example around line 59)

- [ ] **Step 1: Add `daily_filename` to the config example**

In `README.md`, in the `[note]` block of the configuration example, add the `daily_filename` line between `filename` and `meta`:

```toml
[note]
filename = "%Y/%m/%d/%H:%M:%S-%title"
daily_filename = "%Y/%m/%d"
meta = { author = "Your Name" }
```

- [ ] **Step 2: Add the `--daily` usage section**

In `README.md`, immediately after the "Set a custom title and attach labels" code block (currently ends near line 59, before the "List notes" section), insert:

```markdown
Open or create today's daily note (its path comes from `note.daily_filename`,
default `%Y/%m/%d`). If today's note already exists it opens pre-filled for you
to update; otherwise it is created with the title `Daily note for <date>`. Piped
input is appended to an existing daily note:

​```sh
noki --daily
echo "Shipped the release" | noki --daily --no-edit
​```
```

(Remove the zero-width `​` characters shown above around the fences — they are only here to keep this plan's own code block from closing early. Write plain triple-backtick ```sh fences.)

- [ ] **Step 3: Verify the README renders sensibly**

Run: `git diff README.md`
Expected: `daily_filename` appears in the `[note]` example; the new usage section sits between the labels example and "List notes", with an intact ```sh block.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document --daily and note.daily_filename"
```

---

## Self-Review

**Spec coverage:**
- "add an option to create a daily note using the argument `--daily`" → Task 4 (CLI flag + dispatch) + Task 3 (`daily::run`).
- "check if there already is a note that matches the note.daily_filename (default `%Y/%m/%d`)" → `daily_path` (Task 3) uses `config.note.daily_filename` / `note::DEFAULT_DAILY_FILENAME` (Task 1); `load_existing` (Task 3) checks presence. Tests: `daily_path_uses_default_template`, `daily_path_uses_configured_template`, `note_path_daily_template_has_no_title`, `load_existing_*`.
- "If the file already exists it should load its content and update that on save" → `run`'s existing branch pre-fills the editor with the current body and saves via `edit::save_edit`; `append_and_save` handles piped input. Tests: `append_and_save_appends_and_bumps_updated`, `append_body_separates_with_a_blank_line`.
- "If the file doesn't exist it should be created" → `run`'s create branch → `save_new_daily`. Tests: `save_new_daily_*`.
- "reuse [the edit capability]" → daily's existing-note branch calls `edit::save_edit`; the create branch reuses `create::assemble_note` (extracted in Task 2). No duplicate save/build logic.
- Settled decisions: default title `Daily note for %Y-%m-%d` (`default_daily_title`, tested); append semantics (`append_body`, tested); new-note `--title`/`--label` applied only on create (`save_new_daily` params); existing note keeps title/labels (via `save_edit`, which never touches them).

**Placeholder scan:** No TBD/TODO/"handle edge cases". Every code step shows complete code; empty-content, missing-note, no-input-on-existing, and piped-append cases all have explicit handling and (for the non-IO paths) tests. The one narrative instruction (Task 5's zero-width-space note) is an explicit, actionable formatting directive, not a deferral.

**Type consistency:**
- `create::assemble_note(path: String, title: String, content: &str, config: &Config, labels: &[String], now: DateTime<FixedOffset>) -> Note` — identical signature in Task 2's definition, Task 2's test, and Task 3's `save_new_daily` call.
- `edit::save_edit(vcs, note: Note, content: &str, now: DateTime<FixedOffset>) -> Result<Option<String>>` — matches the merged `edit.rs`; called by `append_and_save` and `run`.
- `daily::run(vcs, config, no_edit, title: Option<&str>, labels: &[String]) -> Result<()>` — identical in Task 3's definition and Task 4's `main.rs` dispatch, and mirrors `create::run`'s signature exactly so the two branches of the `None` arm are call-compatible.
- `config.note.daily_filename: Option<String>` (Task 1) — read via `.as_deref()` in `daily_path` (Task 3), same pattern as `filename`.
- `note::DEFAULT_DAILY_FILENAME: &str` (Task 1) — consumed by `daily_path` (Task 3) and the `note_path` test (Task 1).
