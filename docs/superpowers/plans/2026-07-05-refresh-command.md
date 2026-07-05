# Refresh Command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `noki refresh` command that fetches from the remote and rebases the local clone onto it, so `ls`/`show`/`edit` see notes created or edited elsewhere without having to write a note first.

**Architecture:** Reads go through `std::fs` on the local working tree, which today only syncs during `write_file` (commit → fetch → rebase → push). `refresh` exposes the *fetch + rebase* half of that sequence as a standalone operation on the `VersionControl` trait: a no-op for the in-memory test backend, and fetch-then-rebase (fast-forward when the clone is strictly behind) for `GitBackend`, leaving the working tree consistent so subsequent reads are current. Unlike push, a refresh failure is surfaced as an error — there is no un-pushed note at risk, so an offline or conflicting refresh should tell the user rather than fail silently.

**Tech Stack:** Rust, `clap` (CLI), `git2` (libgit2, for fetch/rebase/checkout), `anyhow` (errors), `tempfile` + `git2` (tests).

## Global Constraints

- **Lint gate (must pass before every commit):** `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
- **No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code.** Tests may `unwrap()` freely.
- **Errors use `anyhow::Result` with `.context(...)`** throughout, including the library.
- **Public API at the top of each file, private helpers at the bottom.**
- **TDD:** write the failing test, make it pass, then commit. One logical change per commit.
- **`cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki`.** Run `cargo build` before manually exercising the binary.
- **Keep the agent skills in sync with the CLI.** Adding the `refresh` subcommand is a CLI-surface change, so `skills/retrieving-notes/SKILL.md` is updated in this plan (Task 6).

---

### Task 1: Fast-forward `rebase_onto_origin` when the clone is strictly behind

**Why:** `refresh` (Task 3) reuses `rebase_onto_origin`. In the dominant refresh case the local clone has no un-pushed commits and is simply *behind* origin. The current function routes that case into a `git2` rebase with zero operations, whose HEAD/working-tree result is undefined. This task adds an explicit, deterministic fast-forward that moves the branch ref **and** checks out the working tree, so the on-disk notes match origin. The push path never hits this branch (during `write_file` the local clone always carries the just-made commit, so it is ahead or diverged, never strictly behind), so existing push behavior is unchanged.

**Files:**
- Modify: `src/vcs/git.rs:134-174` (function `rebase_onto_origin`)
- Test: `src/vcs/git.rs` (the `#[cfg(test)] mod tests` block, ~line 204 onward)

**Interfaces:**
- Consumes: existing test helpers `origin_with_seed()`, `commit_file()`, `push_master()`, `set_identity()`, and the private `fetch_origin(&git2::Repository, &mut git2::Remote) -> Result<()>` — all in `src/vcs/git.rs`.
- Produces: `rebase_onto_origin(repo: &git2::Repository, branch: &str) -> Result<()>` — unchanged signature; now fast-forwards (ref + working tree) when the local HEAD is a strict ancestor of `origin/<branch>`.

- [ ] **Step 1: Write the failing test**

Add this test inside the `mod tests` block in `src/vcs/git.rs` (e.g. after `push_rebases_onto_diverged_origin`):

```rust
#[test]
fn rebase_onto_origin_fast_forwards_when_behind() {
    let (origin_dir, _seed_dir, seed) = origin_with_seed();
    let origin_url = origin_dir.path().to_str().unwrap();

    // noki's clone starts at origin's current tip, with no local commits.
    let workdir = tempfile::tempdir().unwrap();
    let noki = Repository::clone(origin_url, workdir.path()).unwrap();
    set_identity(&noki);

    // Origin gains a new note behind noki's back.
    commit_file(&seed, "remote.md", "remote\n", "remote note");
    push_master(&seed);

    // Populate refs/remotes/origin/master, then integrate.
    let mut remote = noki.find_remote("origin").unwrap();
    fetch_origin(&noki, &mut remote).unwrap();
    rebase_onto_origin(&noki, "master").unwrap();

    // Local HEAD now matches origin, and the working tree has the new file.
    let local_head = noki.head().unwrap().peel_to_commit().unwrap().id();
    let origin_head = noki
        .find_reference("refs/remotes/origin/master")
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id();
    assert_eq!(local_head, origin_head);
    assert_eq!(
        std::fs::read_to_string(workdir.path().join("remote.md")).unwrap(),
        "remote\n"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib vcs::git::tests::rebase_onto_origin_fast_forwards_when_behind`
Expected: FAIL. Because the "behind" case currently falls into the rebase branch with zero operations, the working tree is not updated and `read_to_string("remote.md")` errors (No such file) — the test panics on that `unwrap()`.

- [ ] **Step 3: Add the fast-forward branch**

In `src/vcs/git.rs`, in `rebase_onto_origin`, insert the fast-forward handling immediately after the existing "up to date / local ahead" early return and before `let onto = repo.find_annotated_commit(...)`. The relevant region becomes:

```rust
    let local = repo.head()?.peel_to_commit()?;
    // If local already contains upstream, a plain push fast-forwards; no rebase needed.
    if local.id() == upstream.id() || repo.graph_descendant_of(local.id(), upstream.id())? {
        return Ok(());
    }

    // If local is strictly behind upstream (no local-only commits), fast-forward
    // the branch ref and check out the working tree so on-disk notes match origin.
    if repo.graph_descendant_of(upstream.id(), local.id())? {
        let refname = format!("refs/heads/{branch}");
        let mut reference = repo.find_reference(&refname)?;
        reference.set_target(upstream.id(), "noki: fast-forward to origin")?;
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force();
        repo.checkout_head(Some(&mut checkout))?;
        return Ok(());
    }

    let onto = repo.find_annotated_commit(upstream.id())?;
```

Leave the rest of the function (the rebase loop and abort handling) unchanged.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --lib vcs::git::tests::rebase_onto_origin_fast_forwards_when_behind`
Expected: PASS.

- [ ] **Step 5: Run the full test suite and the lint gate**

Run: `cargo test`
Expected: PASS (all tests, including the unchanged `push_rebases_onto_diverged_origin` and `conflicting_note_is_kept_locally_when_rebase_fails`).

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 6: Commit**

```bash
git add src/vcs/git.rs
git commit -m "fix(vcs): fast-forward the working tree when the clone is strictly behind origin"
```

---

### Task 2: Extract `sync` (fetch + rebase) from `sync_and_push`

**Why:** `refresh` (Task 3) needs the *fetch + rebase* preamble without the push. Extract it so both paths share one implementation (DRY) rather than duplicating the origin/branch/detached-HEAD resolution. Pure refactor: no behavior change, existing tests are the safety net.

**Files:**
- Modify: `src/vcs/git.rs:176-193` (function `sync_and_push`; add new function `sync`)

**Interfaces:**
- Consumes: existing private `fetch_origin`, `rebase_onto_origin`, `push`.
- Produces: `sync(repo: &git2::Repository) -> Result<()>` — fetch origin then rebase onto `origin/<current-branch>`; returns `Ok(())` early when there is no `origin` remote; bails on detached HEAD. `sync_and_push` now calls `sync(repo)?` then pushes.

- [ ] **Step 1: Add the `sync` function and rewrite `sync_and_push`**

Replace the existing `sync_and_push` function in `src/vcs/git.rs` with the two functions below (keep them in the same private-helpers region, `sync` above `sync_and_push`):

```rust
fn sync(repo: &git2::Repository) -> Result<()> {
    let mut remote = match repo.find_remote("origin") {
        Ok(remote) => remote,
        Err(_) => return Ok(()), // no remote: local-only repository
    };
    let head = repo.head()?;
    if !head.is_branch() {
        anyhow::bail!("Cannot sync from a detached HEAD");
    }
    let branch = head
        .shorthand()
        .context("Cannot determine the current branch name")?
        .to_string();

    fetch_origin(repo, &mut remote)?;
    rebase_onto_origin(repo, &branch)
}

fn sync_and_push(repo: &git2::Repository) -> Result<()> {
    sync(repo)?;

    let mut remote = match repo.find_remote("origin") {
        Ok(remote) => remote,
        Err(_) => return Ok(()), // no remote: nothing to push
    };
    let branch = repo
        .head()?
        .shorthand()
        .context("Cannot determine the current branch name")?
        .to_string();
    push(repo, &mut remote, &branch)
}
```

- [ ] **Step 2: Update the detached-HEAD test's expectation (message wording)**

The existing test `sync_and_push_rejects_detached_head` asserts the error contains `"detached HEAD"`. The message is now emitted by `sync` as `"Cannot sync from a detached HEAD"`, which still contains `"detached HEAD"`, so **no test edit is required**. Confirm by reading the assertion in `src/vcs/git.rs` (`error.to_string().contains("detached HEAD")`) — leave it as is.

- [ ] **Step 3: Run the full test suite to verify no regression**

Run: `cargo test`
Expected: PASS. In particular `sync_and_push_rejects_detached_head`, `push_rebases_onto_diverged_origin`, `conflicting_note_is_kept_locally_when_rebase_fails`, and `write_then_read_and_commit` all still pass.

- [ ] **Step 4: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 5: Commit**

```bash
git add src/vcs/git.rs
git commit -m "refactor(vcs): extract sync (fetch + rebase) from sync_and_push"
```

---

### Task 3: Add `refresh` to the `VersionControl` trait and both backends

**Why:** Give the command layer a backend-agnostic way to sync. `MemoryBackend` (tests) is an in-memory map with no remote, so refresh is a no-op. `GitBackend` runs `sync`.

**Files:**
- Modify: `src/vcs/mod.rs:9-16` (trait `VersionControl`) — add method
- Modify: `src/vcs/mod.rs:64-86` (`impl VersionControl for MemoryBackend`) — add no-op
- Modify: `src/vcs/mod.rs:88-105` (the `mod tests` block) — add no-op test
- Modify: `src/vcs/git.rs:27-48` (`impl VersionControl for GitBackend`) — add real impl
- Test: `src/vcs/git.rs` (the `mod tests` block) — add an end-to-end refresh test

**Interfaces:**
- Consumes: `sync(repo: &git2::Repository) -> Result<()>` from Task 2; `git2::Repository::open`.
- Produces: `VersionControl::refresh(&self) -> Result<()>` on the trait, implemented by `MemoryBackend` (no-op `Ok(())`) and `GitBackend` (fetch + rebase via `sync`).

- [ ] **Step 1: Write the failing end-to-end test for `GitBackend::refresh`**

Add this test inside the `mod tests` block in `src/vcs/git.rs`:

```rust
#[test]
fn refresh_pulls_remote_changes_into_working_tree() {
    let (origin_dir, _seed_dir, seed) = origin_with_seed();
    let origin_url = origin_dir.path().to_str().unwrap();

    let workdir = tempfile::tempdir().unwrap();
    let noki = Repository::clone(origin_url, workdir.path()).unwrap();
    set_identity(&noki);

    // A note is added on another machine and reaches origin.
    commit_file(&seed, "2026/07/05/note.md", "remote note\n", "remote note");
    push_master(&seed);

    let backend = GitBackend {
        workdir: workdir.path().to_path_buf(),
    };
    backend.refresh().unwrap();

    // The refreshed clone can now read the note that was created elsewhere.
    assert_eq!(
        backend.read_file("2026/07/05/note.md").unwrap(),
        "remote note\n"
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib vcs::git`
Expected: FAIL to compile — `no method named refresh found for struct GitBackend` (the trait has no `refresh` yet).

- [ ] **Step 3: Add `refresh` to the trait**

In `src/vcs/mod.rs`, add the method to the `VersionControl` trait (keep the doc-comment style of the neighbors):

```rust
    /// Write a note file and record it (commit, and push when a remote exists).
    fn write_file(&self, path: &str, contents: &str, message: &str) -> Result<()>;
    /// Bring the local working tree up to date with the remote (fetch + rebase).
    fn refresh(&self) -> Result<()>;
```

- [ ] **Step 4: Implement the `MemoryBackend` no-op**

In `src/vcs/mod.rs`, in `impl VersionControl for MemoryBackend`, add after `write_file`:

```rust
    fn refresh(&self) -> Result<()> {
        Ok(())
    }
```

- [ ] **Step 5: Implement `GitBackend::refresh`**

In `src/vcs/git.rs`, in `impl VersionControl for GitBackend`, add after `write_file`:

```rust
    fn refresh(&self) -> Result<()> {
        let repo = git2::Repository::open(&self.workdir)?;
        sync(&repo)
    }
```

- [ ] **Step 6: Add the `MemoryBackend` no-op test**

In `src/vcs/mod.rs`, in the `mod tests` block, add:

```rust
    #[test]
    fn memory_backend_refresh_is_ok() {
        let backend = MemoryBackend::new();
        assert!(backend.refresh().is_ok());
    }
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test`
Expected: PASS, including `refresh_pulls_remote_changes_into_working_tree` and `memory_backend_refresh_is_ok`.

- [ ] **Step 8: Run the lint gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 9: Commit**

```bash
git add src/vcs/mod.rs src/vcs/git.rs
git commit -m "feat(vcs): add refresh (fetch + rebase) to VersionControl"
```

---

### Task 4: Add the `refresh` command module

**Why:** The command layer wraps `backend.refresh()` and prints a confirmation, mirroring how `list`/`edit` sit between `main.rs` and the trait. Keeping it here means it is unit-tested against `MemoryBackend`.

**Files:**
- Create: `src/commands/refresh.rs`
- Modify: `src/commands/mod.rs:1-5` (module declarations)

**Interfaces:**
- Consumes: `VersionControl::refresh` from Task 3; `crate::vcs::MemoryBackend` (tests).
- Produces: `commands::refresh::run(vcs: &dyn VersionControl) -> Result<()>` — calls `vcs.refresh()?` and prints `Refreshed.` on success.

- [ ] **Step 1: Register the module**

In `src/commands/mod.rs`, add the declaration in alphabetical order with the others:

```rust
pub mod create;
pub mod daily;
pub mod edit;
pub mod list;
pub mod refresh;
pub mod show;
```

- [ ] **Step 2: Write the command module with a failing test**

Create `src/commands/refresh.rs`:

```rust
use crate::vcs::VersionControl;
use anyhow::Result;

/// Fetch the latest notes from the remote and rebase the local clone onto them.
pub fn run(vcs: &dyn VersionControl) -> Result<()> {
    vcs.refresh()?;
    println!("Refreshed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::vcs::MemoryBackend;

    #[test]
    fn run_returns_ok() {
        let backend = MemoryBackend::new();
        assert!(super::run(&backend).is_ok());
    }
}
```

- [ ] **Step 3: Run the test to verify it passes**

Run: `cargo test --lib commands::refresh`
Expected: PASS (`run_returns_ok`).

Note: this is not a red→green cycle in the usual sense — the module and its test are added together because the command is a thin passthrough with no logic to drive out incrementally. The `MemoryBackend` no-op refresh (Task 3) makes the passthrough observable only as "returns Ok"; `GitBackend`'s real behavior is covered by `refresh_pulls_remote_changes_into_working_tree` in Task 3.

- [ ] **Step 4: Run the full suite and the lint gate**

Run: `cargo test`
Expected: PASS.

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 5: Commit**

```bash
git add src/commands/mod.rs src/commands/refresh.rs
git commit -m "feat(commands): add refresh command"
```

---

### Task 5: Wire the `refresh` subcommand into the CLI

**Why:** Expose the command to users as `noki refresh` and dispatch to `commands::refresh::run`.

**Files:**
- Modify: `src/cli.rs:35-60` (the `Commands` enum) — add variant
- Modify: `src/cli.rs` (the `mod tests` block) — add parse test
- Modify: `src/main.rs:26-53` (the `match cli.command` block) — add dispatch arm

**Interfaces:**
- Consumes: `commands::refresh::run(&dyn VersionControl) -> Result<()>` from Task 4; the `backend` and `Commands` enum already in `main.rs`.
- Produces: `Commands::Refresh` variant (`noki refresh`).

- [ ] **Step 1: Write the failing CLI parse test**

In `src/cli.rs`, add to the `mod tests` block:

```rust
    #[test]
    fn parses_refresh_command() {
        let cli = Cli::parse_from(["noki", "refresh"]);
        assert!(matches!(cli.command, Some(Commands::Refresh)));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib cli::tests::parses_refresh_command`
Expected: FAIL to compile — `no variant named Refresh found for enum Commands`.

- [ ] **Step 3: Add the `Refresh` variant**

In `src/cli.rs`, add to the `Commands` enum (after the `Edit` variant):

```rust
    /// Edit an existing note in your editor
    Edit {
        /// The repository-relative path of the note
        path: String,
    },
    /// Fetch the latest notes from the remote and rebase the local clone
    Refresh,
```

- [ ] **Step 4: Run the CLI test to verify it passes**

Run: `cargo test --lib cli::tests::parses_refresh_command`
Expected: PASS.

- [ ] **Step 5: Add the dispatch arm in `main.rs`**

In `src/main.rs`, add an arm to the `match cli.command` block (after the `Edit` arm, before `None =>`):

```rust
        Some(Commands::Edit { path }) => commands::edit::run(backend.as_ref(), &path),
        Some(Commands::Refresh) => commands::refresh::run(backend.as_ref()),
        None => {
```

- [ ] **Step 6: Verify the whole crate builds, tests pass, and the lint gate is clean**

Run: `cargo build`
Expected: builds without warnings (`main.rs` now handles the new variant, so the match is exhaustive).

Run: `cargo test`
Expected: PASS.

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: no output, exit 0.

- [ ] **Step 7: Manually exercise the binary (optional smoke test)**

Run: `cargo run -- refresh --repository <a git repo url you can reach>`
Expected: prints `Refreshed.` and exits 0 when the remote is reachable; prints an error and exits 1 when it is not (e.g. offline or bad URL).

- [ ] **Step 8: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat(cli): add refresh subcommand"
```

---

### Task 6: Document `noki refresh` in the retrieving-notes skill

**Why:** `skills/retrieving-notes/SKILL.md` tells AI agents how to read notes but never mentions that `ls`/`show` read a local clone that only syncs on write. With a `refresh` command now available, the skill must document syncing before reading, per the project rule to keep skills in sync with the CLI surface.

**Files:**
- Modify: `skills/retrieving-notes/SKILL.md` (insert a section after the intro paragraph, before `## List notes`)

**Interfaces:**
- Consumes: the `noki refresh` behavior finalized in Task 5 (prints `Refreshed.`, non-zero exit with a message on failure).
- Produces: documentation only.

- [ ] **Step 1: Insert the "Sync before reading" section**

In `skills/retrieving-notes/SKILL.md`, immediately after the intro paragraph that ends `…instead of scraping it.` (line 8) and before the `## List notes` heading, insert:

````markdown

## Sync before reading

`noki ls` and `noki show` read the **local clone's working tree**, which only syncs with the remote when you *write* a note. If notes may have been added or edited elsewhere (another machine, the web UI), run:

```sh
noki refresh
```

first. It fetches from the remote and rebases the local clone onto it (no push), so subsequent `ls`/`show` see the latest. It prints `Refreshed.` on success and exits non-zero (with a message) if the remote is unreachable or a local change conflicts.

````

- [ ] **Step 2: Verify the edit reads correctly**

Read `skills/retrieving-notes/SKILL.md` and confirm the new section sits between the intro and `## List notes`, and that the surrounding Markdown still renders (no broken code fences).

- [ ] **Step 3: Commit**

```bash
git add skills/retrieving-notes/SKILL.md
git commit -m "docs(skills): document noki refresh in retrieving-notes"
```

---

## Self-Review

**1. Spec coverage** — the request was "add a refresh command (fetch + pull with rebase)":
- Fetch + rebase mechanics with a correct working-tree result → Tasks 1–3.
- Backend-agnostic surface + no-op for the test backend → Task 3.
- User-facing command and CLI wiring → Tasks 4–5.
- Skill kept in sync (project rule) → Task 6.

**2. Placeholder scan** — no `TBD`/`TODO`/"handle edge cases"/"similar to Task N". Every code step shows the actual code; every run step shows the command and expected result.

**3. Type consistency** — `refresh(&self) -> Result<()>` is identical in the trait, `MemoryBackend`, `GitBackend`, and `commands::refresh::run(&dyn VersionControl) -> Result<()>`. `sync(repo: &git2::Repository) -> Result<()>` (Task 2) is the exact name `GitBackend::refresh` calls (Task 3). `Commands::Refresh` (a unit variant) matches the `matches!(…, Some(Commands::Refresh))` test and the `Some(Commands::Refresh) =>` dispatch arm. `rebase_onto_origin(repo, branch)` keeps its signature across Tasks 1–2.

**Design note (verify during Task 1):** the fast-forward path relies on `git2::Reference::set_target(&mut self, Oid, &str)` and `Repository::checkout_head(Option<&mut CheckoutBuilder>)`. If `set_target`'s receiver differs in the installed `git2` version, adjust the `let mut reference` binding accordingly — the test in Task 1 Step 1 is the gate that confirms the fast-forward actually updates HEAD and the working tree.
