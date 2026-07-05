# Shell Completion with Dynamic Note Paths Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add shell tab-completion to `noki` so that `noki show <TAB>` and `noki edit <TAB>` suggest the repository-relative paths of the user's actual notes (and complete subcommands/flags for free).

**Architecture:** Wire `clap_complete`'s dynamic completion engine (`CompleteEnv`) into `main()` so any shell can request completions. Attach an `ArgValueCandidates` provider to the `path` positional of the `Show` and `Edit` subcommands; the provider lists note files by loading config and opening the *already-cloned* repository (never cloning over the network). All completion logic lives in a new, unit-tested `src/completion.rs`, backed by a new non-cloning `vcs::open_existing` helper.

**Tech Stack:** Rust 2024, `clap` 4.6 (derive), `clap_complete` 4.6 (`unstable-dynamic` feature), `anyhow`.

## Global Constraints

- **The name is stylized `Nōki` in prose; the CLI/binary/crate is always lowercase `noki`.** Use `Nōki` only in documentation prose.
- **Errors use `anyhow::Result` with `.context(...)` throughout, including the library.** Do not introduce `thiserror`.
- **No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code.** Tests may `unwrap()` freely.
- **Public API at the top of each file, private helpers at the bottom.**
- **Lint gate must pass before every commit:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
- **`cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki`.** Run `cargo build` before manually exercising the binary.
- TDD: write the failing test, watch it fail, implement, watch it pass, commit.
- **Keep the agent skills in sync with the CLI** (`skills/capturing-notes`, `skills/retrieving-notes`) when the CLI surface, output shapes, or editor behavior change.

---

## File Structure

- **`Cargo.toml`** (modify) — add the `clap_complete` dependency with `unstable-dynamic`. That feature transitively enables `clap/unstable-ext`, which is what makes the `#[arg(add = …)]` derive attribute available — no change to the `clap` dependency line is needed.
- **`src/main.rs`** (modify) — invoke `CompleteEnv::with_factory(Cli::command).complete()` at the very top of `main()`, before any stdout is written. In completion mode this handles the request and exits the process; otherwise it returns and normal flow continues.
- **`src/vcs/mod.rs`** (modify) — add `open_existing`, a factory that opens the per-URL clone **only if it already exists on disk** (returns `Ok(None)` otherwise), plus a private, tempdir-testable `open_existing_at`. This guarantees completion never triggers a network clone.
- **`src/completion.rs`** (create) — the completion candidate providers. `note_paths()` (public, used by the `add` attribute) is thin glue over the testable private `candidates(&dyn VersionControl)` core.
- **`src/lib.rs`** (modify) — declare `pub mod completion;`.
- **`src/cli.rs`** (modify) — attach `ArgValueCandidates::new(crate::completion::note_paths)` to `Show { path }` and `Edit { path }`.
- **`README.md`** (modify) — add a "Shell completion" section with per-shell setup and the documented limitations.
- **`CLAUDE.md`** (modify) — add a short Architecture paragraph describing the completion seam.

---

### Task 1: Add `clap_complete` and wire the completion engine into `main`

This task delivers working completion for subcommands and flags across all shells. Dynamic note-path suggestions come in Task 4.

**Files:**
- Modify: `Cargo.toml` (dependencies)
- Modify: `src/main.rs:1-6`

**Interfaces:**
- Consumes: `noki::cli::Cli` (already implements `clap::CommandFactory` via `#[derive(Parser)]`).
- Produces: nothing consumed by later tasks; establishes that `clap_complete` is a dependency and that `CompleteEnv` runs before `Cli::parse()`.

- [ ] **Step 1: Add the dependency**

Run:

```bash
cargo add clap_complete@4.6 --features unstable-dynamic
```

Expected: `Cargo.toml` gains a line like `clap_complete = { version = "4.6", features = ["unstable-dynamic"] }` and `Cargo.lock` updates. (If `cargo add` orders it elsewhere, that is fine.)

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: builds successfully (only the new dependency is compiled in; no code uses it yet).

- [ ] **Step 3: Wire `CompleteEnv` into `main`**

Edit `src/main.rs`. Change the top imports and the first line of `main()`.

Replace the existing import block:

```rust
use clap::Parser;
use noki::cli::{Cli, Commands};
use noki::{commands, config, vcs};
```

with:

```rust
use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;
use noki::cli::{Cli, Commands};
use noki::{commands, config, vcs};
```

Then insert the completion call as the first statement of `main()`, before the `Cli::parse()` line:

```rust
fn main() {
    CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();
```

(The rest of `main()` is unchanged. `.complete()` calls `std::process::exit(0)` when the shell invoked noki in completion mode, and returns normally otherwise, so the code below it runs unchanged for real invocations.)

- [ ] **Step 4: Verify the lint gate and build**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build`
Expected: all pass; `target/debug/noki` is rebuilt.

- [ ] **Step 5: Manually verify the registration script is produced**

Run: `COMPLETE=bash ./target/debug/noki`
Expected: prints a bash completion script to stdout (it references `noki` and defines a `complete`-based function). This proves `CompleteEnv` is wired ahead of normal parsing.

- [ ] **Step 6: Verify normal invocation is unaffected**

Run: `./target/debug/noki --help`
Expected: the usual help text prints (completion mode did not swallow a normal run).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat(cli): wire clap_complete dynamic completion engine"
```

---

### Task 2: Add a non-cloning `vcs::open_existing` factory

The completion provider must never clone over the network (that would hang the shell). This task adds a factory that opens the clone only if it is already present.

**Files:**
- Modify: `src/vcs/mod.rs` (imports at line 6; add `open_existing` after `open_backend`; add `open_existing_at` after `clone_dir`; add tests in the `mod tests` block)

**Interfaces:**
- Consumes: existing `Config` (`config.repository()`), private `clone_dir(url)`, and `GitBackend::open_or_clone(url, dest)`.
- Produces: `pub fn open_existing(config: &Config) -> Result<Option<Box<dyn VersionControl>>>` — used by `completion::load_backend` in Task 3. Returns `Ok(None)` when the repository has not been cloned yet.

- [ ] **Step 1: Write the failing tests**

Add these two tests inside the existing `#[cfg(test)] mod tests { ... }` block in `src/vcs/mod.rs` (after the existing `memory_backend_*` tests):

```rust
    #[test]
    fn open_existing_at_returns_none_when_not_cloned() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("repo"); // never created
        assert!(open_existing_at("some-url", &dest).unwrap().is_none());
    }

    #[test]
    fn open_existing_at_returns_backend_when_git_present() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path();
        std::fs::create_dir(dest.join(".git")).unwrap();
        std::fs::write(dest.join("note.md"), "hi").unwrap();

        let backend = open_existing_at("some-url", dest)
            .unwrap()
            .expect("expected a backend when .git is present");
        assert_eq!(backend.list_files().unwrap(), vec!["note.md".to_string()]);
    }
```

(These call `open_existing_at`, which will be a sibling private fn — resolved via `super::*` already imported at the top of the tests module. `GitBackend::open_or_clone` only checks for `.git` existence and does no Git operations when it is present, so a bare `.git` directory is enough for the test.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib vcs::tests::open_existing`
Expected: FAIL to compile with "cannot find function `open_existing_at`".

- [ ] **Step 3: Import `Path` and add the public factory**

In `src/vcs/mod.rs`, change the imports at the top. Replace:

```rust
use std::path::PathBuf;
```

with:

```rust
use std::path::{Path, PathBuf};
```

Then add `open_existing` immediately after the existing `open_backend` function (keeping the public API grouped at the top):

```rust
/// Open the working clone for the configured repository **only if it already
/// exists on disk**. Returns `Ok(None)` when the repository has not been cloned
/// yet, so callers (e.g. shell completion) never trigger a network clone.
pub fn open_existing(config: &Config) -> Result<Option<Box<dyn VersionControl>>> {
    let url = config.repository()?;
    let dest = clone_dir(url)?;
    open_existing_at(url, &dest)
}
```

- [ ] **Step 4: Add the private helper**

Add `open_existing_at` in the private section, immediately after the `clone_dir` function:

```rust
fn open_existing_at(url: &str, dest: &Path) -> Result<Option<Box<dyn VersionControl>>> {
    if dest.join(".git").exists() {
        Ok(Some(Box::new(GitBackend::open_or_clone(url, dest)?)))
    } else {
        Ok(None)
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib vcs::tests::open_existing`
Expected: PASS (both tests).

- [ ] **Step 6: Verify the lint gate and full test suite**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add src/vcs/mod.rs
git commit -m "feat(vcs): add non-cloning open_existing factory"
```

---

### Task 3: Add the `completion` module with note-path candidates

**Files:**
- Create: `src/completion.rs`
- Modify: `src/lib.rs` (add module declaration)

**Interfaces:**
- Consumes: `config::load(None)` (config resolved from global config + `.noki.toml` chain, no `--repository`), `vcs::open_existing`, `VersionControl::list_files`, and `clap_complete::engine::CompletionCandidate`.
- Produces: `pub fn note_paths() -> Vec<clap_complete::engine::CompletionCandidate>` — passed as the completer to `ArgValueCandidates::new` in Task 4. Its function-pointer type satisfies `ValueCandidates` (`Fn() -> Vec<CompletionCandidate> + Send + Sync`).

- [ ] **Step 1: Declare the module**

In `src/lib.rs`, add the module declaration in alphabetical position (after `pub mod commands;`):

```rust
pub mod cli;
pub mod commands;
pub mod completion;
pub mod config;
```

- [ ] **Step 2: Write the failing tests**

Create `src/completion.rs` with only the test module and imports for now (the real code comes next; this lets the test fail on missing `candidates`):

```rust
use crate::config;
use crate::vcs::{self, VersionControl};
use clap_complete::engine::CompletionCandidate;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::MemoryBackend;

    fn values(candidates: &[CompletionCandidate]) -> Vec<String> {
        candidates
            .iter()
            .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn candidates_are_the_note_paths() {
        let backend = MemoryBackend::with_files(&[("b/c.md", "y"), ("a.md", "x")]);
        assert_eq!(values(&candidates(&backend)), vec!["a.md", "b/c.md"]);
    }

    #[test]
    fn no_notes_yields_no_candidates() {
        let backend = MemoryBackend::new();
        assert!(candidates(&backend).is_empty());
    }
}
```

(`MemoryBackend::list_files` returns its `BTreeMap` keys, which are sorted, so `a.md` precedes `b/c.md`. `MemoryBackend` and `with_files` are `#[cfg(test)] pub(crate)` in `src/vcs/mod.rs` and are reachable from this crate's test build.)

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib completion`
Expected: FAIL to compile — `candidates` and the `config`/`vcs` imports are unused / not found (the imports will warn or the missing `candidates` will error). The key signal is "cannot find function `candidates`".

- [ ] **Step 4: Implement the module**

Replace the entire contents of `src/completion.rs` with the following (public API at the top, private helpers below the public fn, tests at the bottom):

```rust
use crate::config;
use crate::vcs::{self, VersionControl};
use clap_complete::engine::CompletionCandidate;

/// Completion candidates for a note `path` argument: every note file in the
/// configured repository.
///
/// Config is resolved from the global config file and the `.noki.toml` chain
/// only — a `--repository` typed on the command line is not visible during
/// completion. Candidates are produced only when the repository has already
/// been cloned locally; completion never clones over the network. Any error is
/// swallowed so completion never fails or blocks the shell.
pub fn note_paths() -> Vec<CompletionCandidate> {
    load_backend()
        .map(|backend| candidates(backend.as_ref()))
        .unwrap_or_default()
}

fn candidates(backend: &dyn VersionControl) -> Vec<CompletionCandidate> {
    backend
        .list_files()
        .unwrap_or_default()
        .into_iter()
        .map(CompletionCandidate::new)
        .collect()
}

fn load_backend() -> Option<Box<dyn VersionControl>> {
    let config = config::load(None).ok()?;
    vcs::open_existing(&config).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::MemoryBackend;

    fn values(candidates: &[CompletionCandidate]) -> Vec<String> {
        candidates
            .iter()
            .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn candidates_are_the_note_paths() {
        let backend = MemoryBackend::with_files(&[("b/c.md", "y"), ("a.md", "x")]);
        assert_eq!(values(&candidates(&backend)), vec!["a.md", "b/c.md"]);
    }

    #[test]
    fn no_notes_yields_no_candidates() {
        let backend = MemoryBackend::new();
        assert!(candidates(&backend).is_empty());
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib completion`
Expected: PASS (both tests).

- [ ] **Step 6: Verify the lint gate and full test suite**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: all pass. (Confirms `note_paths`/`load_backend` are not flagged as dead code — `note_paths` is `pub`, and `load_backend` is used by it.)

- [ ] **Step 7: Commit**

```bash
git add src/lib.rs src/completion.rs
git commit -m "feat: add completion module for note-path candidates"
```

---

### Task 4: Suggest note paths for `show` and `edit`

**Files:**
- Modify: `src/cli.rs` (imports at line 1; the `Show` and `Edit` variants at lines 44-59)

**Interfaces:**
- Consumes: `crate::completion::note_paths` (Task 3) and `clap_complete::engine::ArgValueCandidates`.
- Produces: no new symbols; attaches the dynamic completer to the two `path` positionals.

- [ ] **Step 1: Add the import**

In `src/cli.rs`, add the `clap_complete` import below the existing clap import at the top:

```rust
use clap::{Parser, Subcommand};
use clap_complete::engine::ArgValueCandidates;
use clap_verbosity_flag::Verbosity;
```

- [ ] **Step 2: Attach the completer to `Show`**

In the `Show` variant, add the `add` attribute to the `path` field. Replace:

```rust
    /// Show a single note by its path
    Show {
        /// The repository-relative path of the note
        path: String,
```

with:

```rust
    /// Show a single note by its path
    Show {
        /// The repository-relative path of the note
        #[arg(add = ArgValueCandidates::new(crate::completion::note_paths))]
        path: String,
```

- [ ] **Step 3: Attach the completer to `Edit`**

In the `Edit` variant, add the same attribute. Replace:

```rust
    /// Edit an existing note in your editor
    Edit {
        /// The repository-relative path of the note
        path: String,
    },
```

with:

```rust
    /// Edit an existing note in your editor
    Edit {
        /// The repository-relative path of the note
        #[arg(add = ArgValueCandidates::new(crate::completion::note_paths))]
        path: String,
    },
```

- [ ] **Step 4: Verify the lint gate, tests, and build**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build`
Expected: all pass; the existing `cli::tests` (`parses_show_with_flags`, `parses_edit_command`, etc.) still pass — the `add` attribute is completion metadata and does not affect parsing. `target/debug/noki` is rebuilt.

- [ ] **Step 5: Manually verify dynamic path candidates**

This end-to-end check requires a configured repository that has already been cloned locally (e.g. run it in a directory whose `.noki.toml`, or your global config, points at your real notes repo). The shell passes the words after `--` and the cursor index via env vars; simulate `noki show <TAB>`:

Run: `COMPLETE=bash _CLAP_COMPLETE_INDEX=2 ./target/debug/noki -- noki show ''`
Expected: your note paths (e.g. `2026/06/02/10-00-00-my-note.md`) are printed, one per entry.

Sanity-check the guardrails:
- Same command with `edit` (`... -- noki edit ''`) lists the same paths.
- With no repository configured / not yet cloned, the command prints no note paths (it must not hang or error).

If you have no notes repo handy, the automated tests from Task 3 cover the candidate logic; at minimum confirm `COMPLETE=bash ./target/debug/noki -- noki sh` still offers the `show` subcommand (proves the pipeline runs end-to-end).

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs
git commit -m "feat(cli): suggest note paths when completing show and edit"
```

---

### Task 5: Document shell completion and reconcile the skills

**Files:**
- Modify: `README.md` (add a "Shell completion" section after the `edit` example, before "## Agent skills" at line 113)
- Modify: `CLAUDE.md` (add an Architecture paragraph)
- Review: `skills/capturing-notes/SKILL.md`, `skills/retrieving-notes/SKILL.md`

**Interfaces:**
- Consumes: nothing. Documentation only.
- Produces: nothing.

- [ ] **Step 1: Add the README section**

In `README.md`, insert this section immediately before the `## Agent skills` heading (currently line 113):

````markdown
## Shell completion

Nōki completes subcommands and flags, and — for `show` and `edit` — the
repository-relative paths of your notes. Completion is dynamic: pressing
<kbd>Tab</kbd> after `noki show ` lists your actual notes.

Enable it by evaluating `noki`'s output for your shell. Add the line to your
shell's startup file to make it permanent:

```sh
# bash — add to ~/.bashrc
source <(COMPLETE=bash noki)

# zsh — add to ~/.zshrc
source <(COMPLETE=zsh noki)

# fish — add to ~/.config/fish/config.fish
COMPLETE=fish noki | source
```

Elvish and PowerShell are supported too; run `COMPLETE=<shell> noki` to print the
registration script for your shell.

Path suggestions come from the repository configured in your global config or a
`.noki.toml` on the path from your current directory (a `--repository` passed on
the command line is not consulted during completion), and only when that
repository has already been cloned locally — completion never clones over the
network.
````

- [ ] **Step 2: Add the CLAUDE.md Architecture paragraph**

In `CLAUDE.md`, add this paragraph at the end of the `## Architecture` section (after the **Output** paragraph, before `## Conventions`):

```markdown
**Shell completion** (`src/completion.rs`): `main.rs` runs `clap_complete`'s `CompleteEnv` before normal parsing; when a shell invokes `noki` in completion mode it handles the request and exits there. The `show`/`edit` `path` positionals carry an `ArgValueCandidates` provider (`completion::note_paths`) for dynamic note-path suggestions. It resolves config from files only (no `--repository`) and uses `vcs::open_existing`, which returns `None` instead of cloning when the repo is not present locally, so completion never blocks on the network. The `unstable-dynamic` feature of `clap_complete` transitively enables `clap/unstable-ext`, which is what makes the `#[arg(add = ...)]` derive attribute available.
```

- [ ] **Step 3: Reconcile the agent skills**

Review both `skills/capturing-notes/SKILL.md` and `skills/retrieving-notes/SKILL.md`. Shell completion adds **no new subcommand**, changes **no flag, no `--json`/`--raw` output shape, and no editor behavior** — the skills document exactly those surfaces for AI agents, which do not use interactive tab-completion. Confirm with a search that neither skill needs an edit:

Run: `grep -rn "completion\|complete\|autocomplete\|COMPLETE" skills/`
Expected: no matches (or only unrelated prose). No skill change is required; this step is the CLAUDE.md-mandated sync review.

- [ ] **Step 4: Verify the docs build cleanly with the rest of the project**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: all pass (no code changed, but this confirms the tree is still green before committing).

- [ ] **Step 5: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: document shell completion and the completion seam"
```

---

## Self-Review

**1. Spec coverage** — "autocomplete for edit and show commands that will suggest paths on tab":
- Completion engine that shells can invoke → Task 1 (`CompleteEnv`).
- Dynamic *path* suggestions for `show` → Task 4 (`ArgValueCandidates` on `Show.path`).
- Dynamic *path* suggestions for `edit` → Task 4 (`ArgValueCandidates` on `Edit.path`).
- The candidate source (the user's notes) → Task 3 (`completion::note_paths`) + Task 2 (`vcs::open_existing`, non-cloning).
- User can turn it on → Task 5 (README setup instructions).
No gaps.

**2. Placeholder scan** — every code step contains complete, copy-pasteable code; no TBD/TODO/"handle edge cases"/"similar to Task N". Error handling is concrete (swallow-to-empty in `note_paths`; `Ok(None)` in `open_existing`).

**3. Type consistency** — names are consistent across tasks: `vcs::open_existing` (Task 2) is called by `completion::load_backend` (Task 3); `completion::note_paths` (Task 3) is referenced by `ArgValueCandidates::new(crate::completion::note_paths)` (Task 4). `open_existing_at(url, dest)` signature matches its tests. `CompletionCandidate::new` / `get_value` and `ArgValueCandidates::new` / `CompleteEnv::with_factory` match the verified `clap_complete` 4.6 API. `note_paths` returns `Vec<CompletionCandidate>`, satisfying `ValueCandidates` (`Fn() -> Vec<CompletionCandidate> + Send + Sync`) as a function-pointer item.

**Known, documented limitation:** during completion the candidate provider cannot see a `--repository` typed on the command line (clap's dynamic candidate closures have no access to sibling parsed args), so it resolves the repo from config files only. This is stated in the code doc-comment, the README, and the CLAUDE.md note.
