# CLI Title and Labels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users set a custom note title and attach one or more labels at capture time via `--title` and a repeatable `--label` CLI flag.

**Architecture:** Two new global flags are added to the top-level `Cli` struct (capture is the default, no-subcommand command). The custom title (`Option<&str>`) and labels (`&[String]`) are threaded from `main.rs` through `commands::create::run` → `save_note` → `build_note`, where the title overrides the content-derived title (falling back to it when absent or blank) and the labels populate `Meta.labels`.

**Tech Stack:** Rust, `clap` (derive) for arg parsing, existing `note`/`config`/`vcs` modules.

## Global Constraints

- Errors use `anyhow::Result` with `.context(...)` — no `thiserror`.
- No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code. Tests may `unwrap()` freely.
- Public API at the top of each file, private helpers at the bottom.
- Lint gate must pass before every commit: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
- TDD: write the failing test, run it to confirm it fails, implement, run to confirm it passes, then commit.
- `cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki`; run `cargo build` before manually exercising the binary.

---

### Task 1: Add `--title` and `--label` CLI flags

Add the two flags to the top-level `Cli` struct. `--title`/`-t` takes a single optional value; `--label`/`-l` is repeatable and collects into a `Vec<String>`. Nothing consumes these fields yet — Task 2 wires them in. Public struct fields do not trigger unused-code warnings, so the crate compiles cleanly.

**Files:**
- Modify: `src/cli.rs:11-13` (add fields after `no_edit`)
- Test: `src/cli.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces: `Cli.title: Option<String>` and `Cli.labels: Vec<String>` — read by `main.rs` in Task 2 as `cli.title.as_deref()` and `&cli.labels`.

- [ ] **Step 1: Write the failing tests**

Add these three tests inside the existing `mod tests` block in `src/cli.rs` (after `default_command_is_none`):

```rust
    #[test]
    fn parses_title_and_repeated_labels() {
        let cli = Cli::parse_from([
            "noki", "--title", "My title", "--label", "a", "--label", "b",
        ]);
        assert_eq!(cli.title.as_deref(), Some("My title"));
        assert_eq!(cli.labels, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parses_short_title_and_label_flags() {
        let cli = Cli::parse_from(["noki", "-t", "T", "-l", "x", "-l", "y"]);
        assert_eq!(cli.title.as_deref(), Some("T"));
        assert_eq!(cli.labels, vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn title_and_labels_default_empty() {
        let cli = Cli::parse_from(["noki"]);
        assert!(cli.title.is_none());
        assert!(cli.labels.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib cli`
Expected: FAIL — compile error, `no field 'title' on type 'Cli'` (and `labels`).

- [ ] **Step 3: Add the flags to the `Cli` struct**

In `src/cli.rs`, insert the two fields immediately after the `no_edit` field (currently at lines 11-13), before the `--repository` field:

```rust
    /// Skip the editor and store piped input directly
    #[arg(short = 'n', long)]
    pub no_edit: bool,

    /// Set the note title (overrides the title derived from the content)
    #[arg(short = 't', long)]
    pub title: Option<String>,

    /// Add a label to the note; repeat to add several
    #[arg(short = 'l', long = "label")]
    pub labels: Vec<String>,

    /// The notes repository to use
    #[arg(long, global = true)]
    pub repository: Option<String>,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib cli`
Expected: PASS — all `cli::tests` pass, including the three new tests.

- [ ] **Step 5: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add --title and repeatable --label CLI flags"
```

---

### Task 2: Thread the title and labels into note creation

Thread the custom title and labels from `main.rs` through `create::run` → `save_note` → `build_note`. In `build_note`, use the provided title when it is present and non-blank, otherwise fall back to the content-derived title (preserving current behavior); set `Meta.labels` from the provided slice instead of the empty vector. Because the signatures of `build_note`, `save_note`, and `create::run` all change, their call sites (`main.rs` and the existing `create.rs` tests) are updated in this same task so the crate keeps compiling.

**Files:**
- Modify: `src/commands/create.rs:9-20` (`run`), `:22-39` (`save_note`), `:43-85` (`build_note`)
- Modify: `src/main.rs:26-32` (call site for the default/capture command)
- Modify: `src/commands/create.rs` existing tests (new signatures) + add new tests
- Modify: `README.md:45-49` (document the new flags)

**Interfaces:**
- Consumes: `Cli.title: Option<String>` and `Cli.labels: Vec<String>` from Task 1 (passed as `cli.title.as_deref()` and `&cli.labels`).
- Produces (new signatures — later code and tests must match exactly):
  - `create::run(vcs: &dyn VersionControl, config: &Config, no_edit: bool, title: Option<&str>, labels: &[String]) -> Result<()>`
  - `save_note(vcs: &dyn VersionControl, config: &Config, content: &str, title: Option<&str>, labels: &[String], now: DateTime<FixedOffset>) -> Result<Option<String>>`
  - `build_note(content: &str, config: &Config, title: Option<&str>, labels: &[String], now: DateTime<FixedOffset>) -> Option<(String, String)>`

- [ ] **Step 1: Write the failing tests**

In `src/commands/create.rs`, update the existing tests to the new signatures and add four new tests. Replace the whole `#[cfg(test)] mod tests { ... }` block with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::note::parse_note;
    use crate::vcs::MemoryBackend;
    use crate::vcs::VersionControl;
    use chrono::DateTime;

    fn now() -> DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339("2026-06-02T10:00:00+01:00").unwrap()
    }

    #[test]
    fn build_note_returns_none_for_empty_content() {
        let config = Config::default();
        assert!(build_note("   \n", &config, None, &[], now()).is_none());
    }

    #[test]
    fn build_note_produces_path_and_frontmatter() {
        let config = Config::default();
        let (path, raw) =
            build_note("# My new note\n\nHello, World!", &config, None, &[], now()).unwrap();
        assert_eq!(path, "2026/06/02/10:00:00-my-new-note.md");
        let note = parse_note(&raw).unwrap();
        assert_eq!(note.meta.title, "My new note");
        assert_eq!(note.content, "# My new note\n\nHello, World!\n");
    }

    #[test]
    fn build_note_uses_custom_title_over_content() {
        let config = Config::default();
        let (path, raw) = build_note(
            "# Content Heading\n\nbody",
            &config,
            Some("Custom Title"),
            &[],
            now(),
        )
        .unwrap();
        assert_eq!(path, "2026/06/02/10:00:00-custom-title.md");
        let note = parse_note(&raw).unwrap();
        assert_eq!(note.meta.title, "Custom Title");
    }

    #[test]
    fn build_note_falls_back_to_content_title_when_none() {
        let config = Config::default();
        let (_, raw) = build_note("# Real Title\n\nbody", &config, None, &[], now()).unwrap();
        let note = parse_note(&raw).unwrap();
        assert_eq!(note.meta.title, "Real Title");
    }

    #[test]
    fn build_note_falls_back_when_title_is_blank() {
        let config = Config::default();
        let (_, raw) =
            build_note("# Real Title\n\nbody", &config, Some("   "), &[], now()).unwrap();
        let note = parse_note(&raw).unwrap();
        assert_eq!(note.meta.title, "Real Title");
    }

    #[test]
    fn build_note_sets_labels_from_arguments() {
        let config = Config::default();
        let labels = vec!["work".to_string(), "urgent".to_string()];
        let (_, raw) = build_note("body", &config, None, &labels, now()).unwrap();
        let note = parse_note(&raw).unwrap();
        assert_eq!(
            note.meta.labels,
            vec!["work".to_string(), "urgent".to_string()]
        );
    }

    #[test]
    fn save_note_writes_to_backend() {
        let config = Config::default();
        let backend = MemoryBackend::new();
        let path = save_note(&backend, &config, "Hello", None, &[], now())
            .unwrap()
            .unwrap();
        let raw = backend.read_file(&path).unwrap();
        assert!(raw.contains("Hello"));
    }

    #[test]
    fn save_note_skips_empty_content() {
        let config = Config::default();
        let backend = MemoryBackend::new();
        assert!(
            save_note(&backend, &config, "  ", None, &[], now())
                .unwrap()
                .is_none()
        );
        assert!(backend.list_files().unwrap().is_empty());
    }

    #[test]
    fn build_note_merges_static_meta_but_ignores_reserved_keys() {
        let mut config = Config::default();
        config.note.meta.insert(
            "author".to_string(),
            toml::Value::String("Paul".to_string()),
        );
        config.note.meta.insert(
            "title".to_string(),
            toml::Value::String("hacked".to_string()),
        );

        let (_, raw) = build_note("# Real Title\n\nbody", &config, None, &[], now()).unwrap();
        let note = crate::note::parse_note(&raw).unwrap();

        assert_eq!(note.meta.title, "Real Title");
        assert_eq!(
            note.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
        assert!(!note.meta.extra.contains_key("title"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib commands::create`
Expected: FAIL — compile errors: `build_note`/`save_note` take 3/4 arguments but 5/6 supplied.

- [ ] **Step 3: Update `build_note` to accept and apply the title and labels**

In `src/commands/create.rs`, change the `build_note` signature and the title/labels logic. Replace the function (lines 43-85) with:

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
        path: path.clone(),
        labels: labels.to_vec(),
        created: now,
        updated: now,
        extra,
    };
    let full_note = Note {
        meta,
        content: format!("{content}\n"),
    };
    let raw = note::to_raw(&full_note).ok()?;
    Some((path, raw))
}
```

- [ ] **Step 4: Update `save_note` to forward the title and labels**

In `src/commands/create.rs`, replace `save_note` (lines 22-39) with:

```rust
pub(crate) fn save_note(
    vcs: &dyn VersionControl,
    config: &Config,
    content: &str,
    title: Option<&str>,
    labels: &[String],
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    match build_note(content, config, title, labels, now) {
        None => {
            eprintln!("Skipping empty note.");
            Ok(None)
        }
        Some((path, raw)) => {
            vcs.write_file(&path, &raw, &format!("Add note {path}"))?;
            println!("{path}");
            Ok(Some(path))
        }
    }
}
```

- [ ] **Step 5: Update `create::run` to accept the flags and pass them down**

In `src/commands/create.rs`, replace `run` (lines 9-20) with:

```rust
/// Capture note content (from stdin and/or the editor) and store it.
pub fn run(
    vcs: &dyn VersionControl,
    config: &Config,
    no_edit: bool,
    title: Option<&str>,
    labels: &[String],
) -> Result<()> {
    let input = crate::io::read_stdin();
    let content = if no_edit {
        input.unwrap_or_default()
    } else {
        crate::editor::get_content_from_editor(input)?
    };

    let now = Local::now().fixed_offset();
    save_note(vcs, config, &content, title, labels, now)?;
    Ok(())
}
```

- [ ] **Step 6: Update the `main.rs` call site**

In `src/main.rs`, replace the `None` arm of the `match cli.command` block (line 31) with:

```rust
        None => commands::create::run(
            backend.as_ref(),
            &config,
            cli.no_edit,
            cli.title.as_deref(),
            &cli.labels,
        ),
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — the whole crate compiles and all tests pass, including the new `build_note_uses_custom_title_over_content`, `build_note_falls_back_when_title_is_blank`, and `build_note_sets_labels_from_arguments`.

- [ ] **Step 8: Document the flags in the README**

In `README.md`, replace the "Capture piped input without opening the editor" section (lines 45-49) with:

````markdown
Capture piped input without opening the editor:

```sh
echo "A quick note" | noki --no-edit
```

Set a custom title and attach labels (repeat `--label` for several):

```sh
noki --title "Sprint planning" --label work --label meeting
```
````

- [ ] **Step 9: Manually verify the binary end-to-end**

Run:

```bash
cargo build
echo "A quick note" | ./target/debug/noki --no-edit --title "Sprint planning" --label work --label meeting --repository "$(mktemp -d)"
```

Expected: prints a path like `2026/.../sprint-planning.md`. (The `--repository` points at an empty temp dir so the run touches no real notes repo; a push warning is acceptable and non-fatal.)

- [ ] **Step 10: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 11: Commit**

```bash
git add src/commands/create.rs src/main.rs README.md
git commit -m "feat: apply custom title and labels when capturing a note"
```

---

## Self-Review

**Spec coverage:**
- "CLI argument to set a custom title" → Task 1 adds `--title`/`-t`; Task 2 applies it in `build_note` (overrides content-derived title, drives the path slug).
- "CLI argument to set labels" → Task 1 adds `--label`/`-l`; Task 2 sets `Meta.labels`.
- "`--label` allowed multiple times to add multiple labels" → `Vec<String>` field with `long = "label"`; verified by `parses_title_and_repeated_labels` and `parses_short_title_and_label_flags`.

**Placeholder scan:** No TBD/TODO/"handle edge cases" placeholders; every code step shows complete code. The blank-title fallback is fully specified and tested rather than left as "add validation".

**Type consistency:** The three new signatures in the Task 2 Interfaces block match every call site: `main.rs` passes `cli.title.as_deref()` (`Option<&str>`) and `&cli.labels` (`&[String]`); tests call `build_note(content, &config, None, &[], now())` and `save_note(&backend, &config, content, None, &[], now())`. `Cli.title`/`Cli.labels` from Task 1 are consumed exactly as produced.
