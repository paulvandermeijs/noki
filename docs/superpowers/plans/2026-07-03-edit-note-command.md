# Edit Note Command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `noki edit <path>` command that loads an existing note, opens its body in the editor, and saves the result — refreshing the `updated` timestamp while preserving `created`.

**Architecture:** A new `commands::edit` module holds the command entry point `run` and a reusable `save_edit` core. `run` reads and parses the note (erroring if it does not exist), opens the editor pre-filled with the note **body only** (frontmatter stays machine-managed, matching how notes are created today), then delegates to `save_edit`. `save_edit` sets `updated = now`, leaves `created`, `title`, `path`, `labels`, and extra metadata untouched, re-serializes with `note::to_raw`, and writes back to the same path. `save_edit` is deliberately factored out as `pub(crate)` so the forthcoming `--daily` note flow can reuse the exact same save path.

**Tech Stack:** Rust, `clap` (CLI), `chrono` (timestamps), `anyhow` (errors). Storage goes through the existing `VersionControl` trait; tests drive the in-memory `MemoryBackend`.

## Global Constraints

- Lint gate (must pass before every commit): `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
- Errors use `anyhow::Result` with `.context(...)` throughout, including the library — deliberate, do not introduce `thiserror`.
- No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code. Tests may `unwrap()` freely.
- Public API at the top of each file, private helpers at the bottom.
- TDD: write the failing test, watch it fail, make it pass, commit.
- `cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki` — run `cargo build` before exercising the binary by hand.
- Design decisions (already settled, do not revisit): editor shows the **body only**; the note's **title is kept as-is** on edit (never re-derived from the edited body).

---

## File Structure

- `src/commands/edit.rs` — **new.** The `edit` command: `run` (entry point) and `save_edit` (reusable save core). Owns the timestamp-refresh + re-serialize + write logic.
- `src/commands/mod.rs` — **modify.** Register `pub mod edit;`.
- `src/cli.rs` — **modify.** Add the `Edit { path }` subcommand variant.
- `src/main.rs` — **modify.** Dispatch `Commands::Edit` to `commands::edit::run`.
- `README.md` — **modify.** Document the `edit` command.

Task ordering keeps every commit green against the lint gate: Task 1 adds the fully-implemented library module (compiles and tests as `--lib` only, binary untouched and still exhaustive). Task 2 adds the CLI variant and the `main.rs` match arm together, so the binary's `match` stays exhaustive at all times. Task 3 is docs.

---

### Task 1: The `edit` command module (`run` + reusable `save_edit`)

**Files:**
- Create: `src/commands/edit.rs`
- Modify: `src/commands/mod.rs:1-3` (module declarations)
- Test: inline `#[cfg(test)] mod tests` in `src/commands/edit.rs`

**Interfaces:**
- Consumes:
  - `crate::vcs::VersionControl` — trait with `read_file(&self, path: &str) -> Result<String>` and `write_file(&self, path: &str, contents: &str, message: &str) -> Result<()>`.
  - `crate::note::parse_note(raw: &str) -> Result<Note>`, `crate::note::to_raw(note: &Note) -> Result<String>`.
  - `crate::note::Note { meta: Meta, content: String }` and `Meta { title, path, labels, created, updated, extra }` where `created`/`updated` are `chrono::DateTime<chrono::FixedOffset>`.
  - `crate::editor::get_content_from_editor(input: Option<String>) -> Result<String>`.
  - `crate::vcs::MemoryBackend` (test-only) with `new()`, `with_files(&[(&str, &str)])`, and `read_file`.
- Produces:
  - `pub fn run(vcs: &dyn VersionControl, path: &str) -> Result<()>` — the command entry point (consumed by Task 2).
  - `pub(crate) fn save_edit(vcs: &dyn VersionControl, note: Note, content: &str, now: DateTime<FixedOffset>) -> Result<Option<String>>` — reusable save core returning the written path, or `None` when the content is empty (consumed later by the `--daily` flow).

- [ ] **Step 1: Register the module**

In `src/commands/mod.rs`, add the `edit` declaration alongside the others (keep them alphabetical):

```rust
pub mod create;
pub mod edit;
pub mod list;
pub mod show;
```

- [ ] **Step 2: Write the failing tests**

Create `src/commands/edit.rs` with ONLY the test module for now (the implementation comes next, so this must fail to compile — that is the expected "red"):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::parse_note;
    use crate::vcs::{MemoryBackend, VersionControl};
    use chrono::DateTime;

    const NOTE: &str = "---\ntitle: My note\npath: 2026/06/02/my-note.md\nlabels:\n- work\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:00+01:00\nauthor: Paul\n---\n\nOriginal body\n";

    fn at(s: &str) -> DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn save_edit_updates_body_and_updated_but_keeps_created() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/my-note.md", NOTE)]);
        let note = parse_note(NOTE).unwrap();
        let now = at("2026-06-03T12:00:00+01:00");

        let path = save_edit(&backend, note, "New body", now).unwrap().unwrap();
        assert_eq!(path, "2026/06/02/my-note.md");

        let raw = backend.read_file("2026/06/02/my-note.md").unwrap();
        let saved = parse_note(&raw).unwrap();
        assert_eq!(saved.content, "New body\n");
        assert_eq!(saved.meta.updated, now);
        assert_eq!(saved.meta.created, at("2026-06-02T10:00:00+01:00"));
    }

    #[test]
    fn save_edit_preserves_title_labels_and_extra() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/my-note.md", NOTE)]);
        let note = parse_note(NOTE).unwrap();
        let now = at("2026-06-03T12:00:00+01:00");

        save_edit(&backend, note, "New body", now).unwrap().unwrap();

        let raw = backend.read_file("2026/06/02/my-note.md").unwrap();
        let saved = parse_note(&raw).unwrap();
        assert_eq!(saved.meta.title, "My note");
        assert_eq!(saved.meta.labels, vec!["work".to_string()]);
        assert_eq!(
            saved.meta.extra.get("author"),
            Some(&serde_yaml_ng::to_value("Paul").unwrap())
        );
    }

    #[test]
    fn save_edit_skips_empty_content_and_leaves_note_unchanged() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/my-note.md", NOTE)]);
        let note = parse_note(NOTE).unwrap();
        let now = at("2026-06-03T12:00:00+01:00");

        assert!(save_edit(&backend, note, "   \n", now).unwrap().is_none());

        // The stored note is untouched — no write happened.
        assert_eq!(backend.read_file("2026/06/02/my-note.md").unwrap(), NOTE);
    }

    #[test]
    fn run_errors_when_note_is_missing() {
        let backend = MemoryBackend::new();
        let err = run(&backend, "nope.md").unwrap_err();
        assert_eq!(err.to_string(), "No note at nope.md");
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib commands::edit`
Expected: FAIL — compile error, `cannot find function save_edit` / `cannot find function run` in this scope.

- [ ] **Step 4: Write the implementation**

Prepend the implementation ABOVE the test module in `src/commands/edit.rs` (public API at the top, `pub(crate)` helper below it, tests at the bottom):

```rust
use crate::note::{self, Note};
use crate::vcs::VersionControl;
use anyhow::Result;
use chrono::{DateTime, FixedOffset, Local};

/// Load an existing note, open its body in the editor, and save the result,
/// refreshing the `updated` timestamp while preserving `created`.
pub fn run(vcs: &dyn VersionControl, path: &str) -> Result<()> {
    let raw = vcs.read_file(path)?;
    let note = note::parse_note(&raw)?;

    let edited = crate::editor::get_content_from_editor(Some(note.content.clone()))?;

    let now = Local::now().fixed_offset();
    save_edit(vcs, note, &edited, now)?;
    Ok(())
}

/// Write `content` back into an existing note, setting `updated` to `now` and
/// leaving `created`, `title`, `path`, `labels`, and extra metadata untouched.
/// Returns the note's path, or `None` when the content is empty.
///
/// This is the shared save path the forthcoming `--daily` note flow reuses.
pub(crate) fn save_edit(
    vcs: &dyn VersionControl,
    mut note: Note,
    content: &str,
    now: DateTime<FixedOffset>,
) -> Result<Option<String>> {
    let content = content.trim();
    if content.is_empty() {
        eprintln!("Note is empty; leaving it unchanged.");
        return Ok(None);
    }

    note.content = format!("{content}\n");
    note.meta.updated = now;

    let path = note.meta.path.clone();
    let raw = note::to_raw(&note)?;
    vcs.write_file(&path, &raw, &format!("Update note {path}"))?;
    println!("{path}");
    Ok(Some(path))
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib commands::edit`
Expected: PASS — 4 tests pass (`save_edit_updates_body_and_updated_but_keeps_created`, `save_edit_preserves_title_labels_and_extra`, `save_edit_skips_empty_content_and_leaves_note_unchanged`, `run_errors_when_note_is_missing`).

- [ ] **Step 6: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0. (The binary is unchanged, so its `match` is still exhaustive.)

- [ ] **Step 7: Commit**

```bash
git add src/commands/edit.rs src/commands/mod.rs
git commit -m "feat: add reusable edit-note save core"
```

---

### Task 2: Wire the `edit` subcommand into the CLI

**Files:**
- Modify: `src/cli.rs:31-51` (the `Commands` enum) and its `#[cfg(test)] mod tests`
- Modify: `src/main.rs:26-40` (the command `match`)

**Interfaces:**
- Consumes: `noki::commands::edit::run(backend.as_ref(), &path)` produced in Task 1.
- Produces: `Commands::Edit { path: String }` variant.

- [ ] **Step 1: Write the failing CLI parse test**

Add this test inside the existing `#[cfg(test)] mod tests` in `src/cli.rs`:

```rust
    #[test]
    fn parses_edit_command() {
        let cli = Cli::parse_from(["noki", "edit", "a/b.md"]);
        match cli.command {
            Some(Commands::Edit { path }) => assert_eq!(path, "a/b.md"),
            _ => panic!("expected edit"),
        }
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib cli::tests::parses_edit_command`
Expected: FAIL — compile error, `no variant named Edit found for enum Commands`.

- [ ] **Step 3: Add the `Edit` variant**

In `src/cli.rs`, add the variant to the `Commands` enum, after the `Show { ... }` variant:

```rust
    /// Edit an existing note in your editor
    Edit {
        /// The repository-relative path of the note
        path: String,
    },
```

- [ ] **Step 4: Dispatch the variant in `main.rs`**

In `src/main.rs`, add a match arm for `Edit` before the `None =>` arm:

```rust
        Some(Commands::Edit { path }) => commands::edit::run(backend.as_ref(), &path),
```

The `match` in `run` now reads:

```rust
    match cli.command {
        Some(Commands::List { json }) => {
            commands::list::run(backend.as_ref(), json, config.max_visible_labels())
        }
        Some(Commands::Show { path, json, raw }) => {
            commands::show::run(backend.as_ref(), &path, json, raw)
        }
        Some(Commands::Edit { path }) => commands::edit::run(backend.as_ref(), &path),
        None => commands::create::run(
            backend.as_ref(),
            &config,
            cli.no_edit,
            cli.title.as_deref(),
            &cli.labels,
        ),
    }
```

- [ ] **Step 5: Run the CLI test to verify it passes**

Run: `cargo test --lib cli::tests::parses_edit_command`
Expected: PASS.

- [ ] **Step 6: Run the full test suite and lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass; lint gate produces no output, exit 0. (The `match` is exhaustive because the variant and its arm were added together.)

- [ ] **Step 7: Exercise the binary end-to-end (manual smoke test)**

The editor path cannot be unit-tested, so verify it by hand once against a scratch repository.

```bash
cargo build
# Create a throwaway local notes repo:
REPO="$(mktemp -d)"
git -C "$REPO" init -q
git -C "$REPO" config user.name Test
git -C "$REPO" config user.email test@example.com
git -C "$REPO" commit -q --allow-empty -m init
# Capture a note (write a heading + body in the editor, then save & quit):
EDITOR=vim ./target/debug/noki --repository "$REPO"
# Note the printed path (e.g. 2026/07/03/HH:MM:SS-<title>.md), then edit it:
EDITOR=vim ./target/debug/noki edit <printed-path> --repository "$REPO"
# Confirm: body reflects your edit, `updated` advanced, `created` unchanged:
./target/debug/noki show <printed-path> --repository "$REPO" --raw
```

Expected: after edit, the raw note shows your new body, an `updated` timestamp later than `created`, and an unchanged `created`. Editing a missing path errors with `No note at <path>`.

- [ ] **Step 8: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add noki edit subcommand"
```

---

### Task 3: Document the `edit` command

**Files:**
- Modify: `README.md:70-76` (after the "Show a single note" block)

- [ ] **Step 1: Add the edit section to the README**

In `README.md`, insert this block immediately after the "Show a single note" code block (before the `## License` heading):

```markdown
Edit an existing note (opens your editor with the note's body; on save the
`updated` timestamp is refreshed while `created` is preserved, and the title,
labels, and other frontmatter are kept as-is):

```sh
noki edit 2026/06/02/10:00:00-my-new-note.md
```
```

- [ ] **Step 2: Verify the README renders sensibly**

Run: `git diff README.md`
Expected: the new section appears after the show examples and before `## License`, with the fenced `sh` block intact.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document the noki edit command"
```

---

## Future work (not in this plan): `--daily` notes

This plan intentionally stops at `edit`. When the `--daily` flow is built next, it
will reuse `commands::edit::save_edit` as its save path:

1. Add a `note.daily_filename` config field (default `"%Y/%m/%d"`) to `NoteConfig`.
2. On `noki --daily`, render today's path from that template (via `note::note_path`
   with no `%title`), then check the backend for it.
3. If the file exists, `parse_note` it; otherwise build a fresh `Note` (with
   `created = now`). Either way, open the editor pre-filled with the current body
   and hand the result to `save_edit`, which already sets `updated = now` and
   preserves `created`.

No changes to `save_edit` should be needed — that is the whole point of factoring
it out now.

---

## Self-Review

**Spec coverage:**
- "add the option to edit a note using `noki edit <path>`" → Task 2 (CLI variant + dispatch) + Task 1 (`run`).
- "load the note at given path" → `run` calls `vcs.read_file(path)?` then `parse_note` (Task 1).
- "when the editor is closed update the note" → `run` calls `get_content_from_editor` then `save_edit` (Task 1).
- "`updated` metadata should be updated to the current time" → `save_edit` sets `note.meta.updated = now` (Task 1, test `save_edit_updates_body_and_updated_but_keeps_created`).
- "`created` never updates" → `save_edit` leaves `created` untouched (Task 1, same test asserts `created` unchanged).
- "reuse this later to add the option to easily add/update a daily note" → `save_edit` is `pub(crate)` and note the "Future work" section; test coverage locks its contract.
- Settled design decisions (body-only editor, keep existing title) → `run` pre-fills with `note.content` only; `save_edit` never touches `note.meta.title`, verified by `save_edit_preserves_title_labels_and_extra`.

**Placeholder scan:** No TBD/TODO/"handle edge cases" placeholders; every code step shows complete code; the empty-content and missing-note edge cases have explicit handling and tests.

**Type consistency:** `run(vcs: &dyn VersionControl, path: &str) -> Result<()>` and `save_edit(vcs, note: Note, content: &str, now: DateTime<FixedOffset>) -> Result<Option<String>>` are used identically in the module, the tests, and the `main.rs` dispatch. `save_edit` mirrors the existing `create::save_note` signature shape (`Result<Option<String>>`, `now: DateTime<FixedOffset>`), so the codebase stays uniform.
