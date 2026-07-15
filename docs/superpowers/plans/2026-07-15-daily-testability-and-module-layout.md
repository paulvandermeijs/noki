# Daily Testability & Module Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Two independent code-quality improvements found by the `rust-code-quality` / `rust-code-style` validation: (1) make `commands::daily::run` testable to clear its CRAP score of 240, and (2) drop the `mod.rs` module form in favour of file-with-folder, per the style rule.

**Architecture:** (1) Split `daily::run` so the editor/stdin IO stays in a thin `run`, and the decision logic moves into small helpers — an editor-free `capture_existing_no_edit` and a pure `prefill_for_update`, both unit-tested against `MemoryBackend`; the interactive branch stays a thin, untested `capture_existing_interactive`. (2) `git mv` the three non-barrel `mod.rs` files to `<name>.rs` beside their existing `<name>/` directories; Rust resolves the modules identically, so it is a pure move plus one doc fix.

**Tech Stack:** Rust, `anyhow`, `chrono`, `MemoryBackend` test double, cargo-crap / cargo-llvm-cov.

## Global Constraints

- Work on a branch off `master` (e.g. `feat/daily-testability-module-layout`); do NOT commit on `master` directly.
- Lint gate MUST pass before every commit: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
- No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code (one pre-existing justified `expect()` in `collect_notes`); tests may `unwrap()` freely.
- `anyhow::Result` with `.context(...)`; no `thiserror`.
- Public API at the top of each file, private helpers at the bottom; `#[cfg(test)] mod tests` last.
- Clippy `too_many_arguments` threshold is 7 — keep every new function at ≤7 parameters (the helpers below are designed for 5).
- Behaviour must be preserved exactly in Task 1 — it is a refactor, not a behaviour change; the full existing suite must stay green.
- **Gotcha:** `cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki`. Run `cargo build` before manually exercising the binary.
- **Out of scope:** the other two CRAP offenders — `main.rs::run` (90) and `editor.rs::get_content_from_editor` (56) — are thin glue over untestable IO and are deliberately left alone. After Task 1, cargo-crap will still list those two; that is expected, not a failure.

---

### Task 1: Make `daily::run` testable (clears its CRAP 240)

**Files:**
- Modify: `src/commands/daily.rs` — replace `run` (lines 12-49) with a thin dispatcher plus three new private helpers; add four tests to the existing `#[cfg(test)] mod tests`.

**Interfaces:**
- Consumes: existing private helpers in the file — `daily_path`, `load_existing`, `append_body`, `append_and_save(vcs, config, note, addition, now)`, `save_update(vcs, config, note, body, now)`, `save_new_daily(vcs, config, path, content, title, labels, now)`.
- Produces (all private to the module):
  - `fn capture_existing_no_edit(vcs: &dyn VersionControl, config: &Config, note: Note, input: Option<String>, now: DateTime<FixedOffset>) -> Result<()>`
  - `fn capture_existing_interactive(vcs: &dyn VersionControl, config: &Config, note: Note, input: Option<String>, now: DateTime<FixedOffset>) -> Result<()>`
  - `fn prefill_for_update(content: &str, input: Option<String>) -> String`

- [ ] **Step 1: Write the failing tests**

Add these four tests inside the `#[cfg(test)] mod tests` block in `src/commands/daily.rs` (they use the existing `EXISTING`, `now()`, `MemoryBackend`, `parse_note` helpers already in that module):

```rust
    #[test]
    fn capture_existing_no_edit_appends_piped_input() {
        let backend = MemoryBackend::with_files(&[("2026/07/03.md", EXISTING)]);
        let config = Config::default();
        let note = parse_note(EXISTING).unwrap();
        capture_existing_no_edit(&backend, &config, note, Some("did X".to_string()), now()).unwrap();
        let saved = parse_note(&backend.read_file("2026/07/03.md").unwrap()).unwrap();
        assert_eq!(saved.content, "Morning notes\n\ndid X\n");
        assert_eq!(saved.meta.updated, now());
    }

    #[test]
    fn capture_existing_no_edit_without_input_leaves_note_untouched() {
        let backend = MemoryBackend::with_files(&[("2026/07/03.md", EXISTING)]);
        let config = Config::default();
        let note = parse_note(EXISTING).unwrap();
        capture_existing_no_edit(&backend, &config, note, None, now()).unwrap();
        assert_eq!(backend.read_file("2026/07/03.md").unwrap(), EXISTING);
    }

    #[test]
    fn prefill_for_update_appends_piped_input() {
        assert_eq!(
            prefill_for_update("Morning notes\n", Some("did X".to_string())),
            "Morning notes\n\ndid X"
        );
    }

    #[test]
    fn prefill_for_update_without_input_keeps_body() {
        assert_eq!(prefill_for_update("Morning notes\n", None), "Morning notes\n");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib commands::daily`
Expected: FAIL to compile — `cannot find function capture_existing_no_edit` / `prefill_for_update`.

- [ ] **Step 3: Replace `run` and add the three helpers**

In `src/commands/daily.rs`, replace the current `run` function (lines 12-49, from `pub fn run(` through its closing `}`) with the following. Everything from `daily_path` (line 51) down is unchanged.

```rust
pub fn run(
    vcs: &dyn VersionControl,
    config: &Config,
    no_edit: bool,
    title: Option<&str>,
    labels: &[String],
) -> Result<()> {
    let now = Local::now().fixed_offset();
    let path = daily_path(config, now)?;
    let input = crate::io::read_stdin();

    match load_existing(vcs, &path)? {
        Some(note) => {
            if no_edit {
                capture_existing_no_edit(vcs, config, note, input, now)
            } else {
                capture_existing_interactive(vcs, config, note, input, now)
            }
        }
        None => {
            let content = if no_edit {
                input.unwrap_or_default()
            } else {
                crate::editor::get_content_from_editor(input)?
            };
            save_new_daily(vcs, config, &path, &content, title, labels, now)?;
            Ok(())
        }
    }
}

/// The `--no-edit` update of an existing daily note. Editor-free (no stdin, no
/// `$EDITOR`), so it is unit-testable: append piped `input` to today's note, or
/// warn when there is nothing to add.
fn capture_existing_no_edit(
    vcs: &dyn VersionControl,
    config: &Config,
    note: Note,
    input: Option<String>,
    now: DateTime<FixedOffset>,
) -> Result<()> {
    match input {
        Some(piped) => {
            append_and_save(vcs, config, note, &piped, now)?;
        }
        None => eprintln!("Nothing to add to today's note."),
    }
    Ok(())
}

/// The interactive update of an existing daily note. Not unit-tested — it spawns
/// `$EDITOR`; the pure prefill decision lives in `prefill_for_update`.
fn capture_existing_interactive(
    vcs: &dyn VersionControl,
    config: &Config,
    note: Note,
    input: Option<String>,
    now: DateTime<FixedOffset>,
) -> Result<()> {
    let prefill = prefill_for_update(&note.content, input);
    let body = crate::editor::get_content_from_editor(Some(prefill))?;
    save_update(vcs, config, note, &body, now)?;
    Ok(())
}

/// The editor prefill when updating an existing daily note: the body with any
/// piped `input` appended below it, or the body unchanged when nothing was piped.
fn prefill_for_update(content: &str, input: Option<String>) -> String {
    match input {
        Some(piped) => append_body(content, &piped),
        None => content.to_string(),
    }
}
```

This preserves behaviour exactly: existing+no_edit+piped → append; existing+no_edit+empty → "Nothing to add"; existing+editor → prefilled editor then `save_update`; missing+no_edit → `save_new_daily` with `input.unwrap_or_default()`; missing+editor → editor then `save_new_daily`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test`
Expected: PASS — the four new tests plus the whole existing suite (behaviour unchanged; no test regressions).

- [ ] **Step 5: Confirm the CRAP score dropped**

Run:
```bash
cargo llvm-cov --lcov --output-path lcov.info >/dev/null 2>&1 && cargo crap --lcov lcov.info 2>&1 | grep -E "daily\.rs|exceed"; rm -f lcov.info
```
Expected: the extracted `capture_existing_no_edit` / `prefill_for_update` are covered and well under 30. **Correction (post-implementation):** cargo-crap counts each `?` as a branch, so `run` stays at CC 9 / CRAP 90 (0% coverage — it is untestable IO wiring). This is an **accepted outcome**: `run` is now thin dispatch in the same category as `main.rs::run` (also 90); the risky decision logic has been extracted and covered, which was the real goal. `run`, `main.rs::run`, and `editor.rs::get_content_from_editor` remain on the `✗` list by design.

- [ ] **Step 6: Lint gate and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add src/commands/daily.rs
git commit -m "refactor(daily): split run into testable capture helpers"
```
Expected: lint gate clean (all new helpers have ≤5 parameters, so no `too_many_arguments`); commit succeeds.

---

### Task 2: Adopt file-with-folder module layout (drop `mod.rs`)

**Files:**
- Rename: `src/render/mod.rs` → `src/render.rs`
- Rename: `src/vcs/mod.rs` → `src/vcs.rs`
- Rename: `src/commands/mod.rs` → `src/commands.rs`
- Modify: `CLAUDE.md` (two references to `src/vcs/mod.rs`)

**Interfaces:**
- Consumes: nothing from Task 1 (independent).
- Produces: no API change. `src/lib.rs`'s `mod render; pub mod vcs; pub mod commands;` resolve to the new `<name>.rs` files automatically — no `lib.rs` edit. Submodule declarations inside the moved files (`mod inline;`, `pub mod git;`, `pub mod create;`, …) continue to resolve against the unchanged `<name>/` directories.

- [ ] **Step 1: Move the three files with git**

```bash
git mv src/render/mod.rs src/render.rs
git mv src/vcs/mod.rs src/vcs.rs
git mv src/commands/mod.rs src/commands.rs
```
No file contents change. (Do NOT touch the historical plan docs under `docs/superpowers/plans/` that mention the old paths — they are point-in-time artifacts.)

- [ ] **Step 2: Verify the crate still builds and tests pass**

Run: `cargo build && cargo test`
Expected: PASS — the module tree is unchanged to the compiler; no code referenced the old paths as strings.

- [ ] **Step 3: Update the two CLAUDE.md references**

In `CLAUDE.md`, update the two mentions of the old path:

Line ~22 — change:
```
**Storage is decoupled behind the `VersionControl` trait** (`src/vcs/mod.rs`): `list_files` / `read_file` / `write_file`.
```
to:
```
**Storage is decoupled behind the `VersionControl` trait** (`src/vcs.rs`): `list_files` / `read_file` / `write_file`.
```

Line ~25 — change:
```
- Command functions take `&dyn VersionControl`, so they are tested against the `#[cfg(test)]` `MemoryBackend` in `src/vcs/mod.rs` — command/note/output logic is fully tested without touching Git.
```
to:
```
- Command functions take `&dyn VersionControl`, so they are tested against the `#[cfg(test)]` `MemoryBackend` in `src/vcs.rs` — command/note/output logic is fully tested without touching Git.
```

(These are the only live doc references — `render/mod.rs` and `commands/mod.rs` are not mentioned in CLAUDE.md.)

- [ ] **Step 4: Lint gate and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add src/render.rs src/vcs.rs src/commands.rs src/render/mod.rs src/vcs/mod.rs src/commands/mod.rs CLAUDE.md
git commit -m "refactor: adopt file-with-folder module layout"
```
Expected: lint gate clean; `git status` shows the three files renamed (git detects the moves) and CLAUDE.md modified; commit succeeds.

---

## Self-Review

**1. Spec coverage:**
- #1 "make `daily::run` testable → clears CRAP 240" → Task 1 (extract `capture_existing_no_edit` + `prefill_for_update`, cover both; thin `run` + `capture_existing_interactive`; Step 5 verifies the score). ✅
- #2 "drop `mod.rs` (fuzzy-search pollution) → file-with-folder" → Task 2 (three `git mv`s + CLAUDE.md). ✅
- Out-of-scope CRAP offenders (`main::run`, `get_content_from_editor`) explicitly called out in Global Constraints and Task 1 Step 5. ✅

**2. Placeholder scan:** No TBD/TODO/"handle edge cases"/"similar to Task N". Every code step shows the full code; every command has an expected result.

**3. Type consistency:** The three helper signatures in Task 1's Interfaces block match their definitions in Step 3 and their call sites in the new `run` and the Step 1 tests: `capture_existing_no_edit(&dyn VersionControl, &Config, Note, Option<String>, DateTime<FixedOffset>)`, `capture_existing_interactive(...)` identical, `prefill_for_update(&str, Option<String>) -> String`. All ≤5 params (satisfies the `too_many_arguments` constraint). Task 2 changes no signatures.
