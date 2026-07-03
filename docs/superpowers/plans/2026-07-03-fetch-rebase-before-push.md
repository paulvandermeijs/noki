# Fetch-then-Rebase Before Push Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `GitBackend`'s push path fetch `origin` and rebase local note commits onto `origin/<branch>` before pushing, so notes no longer fail permanently with `NotFastForward` once the remote diverges.

**Architecture:** After the local commit (unchanged), the push path becomes fetch → rebase-onto-origin → push, all inside the existing non-fatal wrapper. Because notes use unique timestamped paths they almost never textually conflict, so the rebase is normally clean; a genuine conflict aborts the rebase and falls back to the current "committed locally, push failed" warning, preserving the note-is-never-lost guarantee. History stays strictly linear (rebase, never merge).

**Tech Stack:** Rust, `git2` 0.21 (libgit2, vendored), `anyhow`.

## Global Constraints

- No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code (tests may `unwrap()` freely). The single pre-existing justified `expect()` in `collect_notes` stays.
- Errors use `anyhow::Result` with `.context(...)`; do not introduce `thiserror`.
- Public API at the top of each file, private helpers at the bottom. `GitBackend` and its `impl` block stay at the top of `src/vcs/git.rs`; all new functions are private and go in the private-helper section below the `impl`.
- Push (and now fetch/rebase) must be non-fatal: `write_file` always returns `Ok` after the local commit succeeds, even if syncing with `origin` fails — a warning is printed to stderr.
- History must stay strictly linear: integrate with rebase, never `merge`.
- Lint gate that must pass before every commit: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
- **Gotcha:** `cargo test`/`cargo clippy` do not rebuild `target/debug/noki`; that does not matter here because all verification is via `cargo test`, not the binary.

---

### Task 1: Fetch and rebase onto a diverged origin before pushing

Replaces the plain `push` call with a `sync_and_push` that fetches `origin`, rebases the local note commit(s) onto `origin/<branch>`, then pushes. This fixes the reported `NotFastForward` when the remote received unrelated commits (e.g. a wiki page edited via the web UI). Credential handling is extracted so fetch and push share it.

**Files:**
- Modify: `src/vcs/git.rs` (private-helper section, below the `impl VersionControl` block)
- Test: `src/vcs/git.rs` (the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `git2::Repository`, `git2::Remote`, `git2::Config` (0.21 API).
- Produces (all private to `src/vcs/git.rs`):
  - `fn remote_callbacks(config: git2::Config) -> git2::RemoteCallbacks<'static>`
  - `fn fetch_origin(repo: &git2::Repository, remote: &mut git2::Remote) -> anyhow::Result<()>`
  - `fn rebase_onto_origin(repo: &git2::Repository, branch: &str) -> anyhow::Result<()>`
  - `fn sync_and_push(repo: &git2::Repository) -> anyhow::Result<()>`
  - `fn push(repo: &git2::Repository, remote: &mut git2::Remote, branch: &str) -> anyhow::Result<()>` (existing `push` refactored to take `remote` and `branch`)

- [ ] **Step 1: Add test helpers to the test module**

In `src/vcs/git.rs`, inside `mod tests`, add these helpers next to the existing `init_repo_with_commit` (keep `init_repo_with_commit` as-is; the existing tests still use it):

```rust
    // Commit `contents` to `name` in `repo`'s working tree, on top of the current HEAD.
    fn commit_file(repo: &Repository, name: &str, contents: &str, message: &str) -> git2::Oid {
        let workdir = repo.workdir().unwrap();
        let full = workdir.join(name);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full, contents).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("Test", "test@example.com").unwrap();
        let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = head.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap()
    }

    fn set_identity(repo: &Repository) {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }

    // Push `master` to the repo's `origin` (a local bare path needs no credentials).
    fn push_master(repo: &Repository) {
        let mut remote = repo.find_remote("origin").unwrap();
        remote
            .push(&["refs/heads/master:refs/heads/master"], None)
            .unwrap();
    }

    // A bare `origin` seeded with one commit, plus a working "seed" clone wired to it.
    // Returns (origin tempdir, seed tempdir, seed repo). Both tempdirs must stay alive.
    fn origin_with_seed() -> (tempfile::TempDir, tempfile::TempDir, Repository) {
        let origin_dir = tempfile::tempdir().unwrap();
        Repository::init_bare(origin_dir.path()).unwrap();
        let origin_url = origin_dir.path().to_str().unwrap();

        let seed_dir = tempfile::tempdir().unwrap();
        let seed = Repository::init(seed_dir.path()).unwrap();
        set_identity(&seed);
        commit_file(&seed, "seed.txt", "seed\n", "initial");
        seed.remote("origin", origin_url).unwrap();
        push_master(&seed);

        (origin_dir, seed_dir, seed)
    }
```

- [ ] **Step 2: Write the failing test for the diverged-origin case**

Add to `mod tests`:

```rust
    #[test]
    fn push_rebases_onto_diverged_origin() {
        let (origin_dir, _seed_dir, seed) = origin_with_seed();
        let origin_url = origin_dir.path().to_str().unwrap();

        // noki's private clone of origin.
        let workdir = tempfile::tempdir().unwrap();
        let noki = Repository::clone(origin_url, workdir.path()).unwrap();
        set_identity(&noki);

        // Someone advances origin behind noki's back (e.g. a wiki edit via the web UI).
        commit_file(&seed, "other.txt", "web edit\n", "unrelated remote change");
        push_master(&seed);

        // noki captures a note; it must fetch, rebase onto origin, and push.
        let backend = GitBackend {
            workdir: workdir.path().to_path_buf(),
        };
        backend
            .write_file("note.md", "hello\n", "Add note")
            .unwrap();

        // Origin now contains BOTH the note and the unrelated remote change.
        let origin_repo = Repository::open_bare(origin_dir.path()).unwrap();
        let tree = origin_repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_path(Path::new("note.md")).is_ok(), "note pushed");
        assert!(
            tree.get_path(Path::new("other.txt")).is_ok(),
            "remote change preserved"
        );

        // History is linear: the note commit has exactly one parent (the remote change).
        let local = Repository::open(workdir.path()).unwrap();
        assert_eq!(local.head().unwrap().peel_to_commit().unwrap().parent_count(), 1);
    }
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --lib vcs::git::tests::push_rebases_onto_diverged_origin`
Expected: FAIL — with today's plain push, `write_file` prints a `NotFastForward` warning and returns `Ok`, so origin never receives `note.md` and the `"note pushed"` assertion fails.

- [ ] **Step 4: Refactor credential handling into a shared helper**

In `src/vcs/git.rs`, in the private-helper section (below the `impl`), add `remote_callbacks` and replace the inline credential closure currently inside `push`. Add near the other helpers:

```rust
fn remote_callbacks(config: git2::Config) -> git2::RemoteCallbacks<'static> {
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(move |url, username, allowed| {
        if allowed.contains(git2::CredentialType::SSH_KEY) {
            git2::Cred::ssh_key_from_agent(username.unwrap_or("git"))
        } else if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            git2::Cred::credential_helper(&config, url, username)
        } else {
            git2::Cred::default()
        }
    });
    callbacks
}
```

- [ ] **Step 5: Add `fetch_origin`**

Add to the private-helper section:

```rust
fn fetch_origin(repo: &git2::Repository, remote: &mut git2::Remote) -> Result<()> {
    let mut options = git2::FetchOptions::new();
    options.remote_callbacks(remote_callbacks(repo.config()?));
    // Empty refspec list uses origin's configured fetch refspecs, updating refs/remotes/origin/*.
    let refspecs: [&str; 0] = [];
    remote
        .fetch(&refspecs, Some(&mut options), None)
        .context("Failed to fetch from origin")
}
```

- [ ] **Step 6: Add `rebase_onto_origin`**

Add to the private-helper section:

```rust
fn rebase_onto_origin(repo: &git2::Repository, branch: &str) -> Result<()> {
    let upstream_ref = format!("refs/remotes/origin/{branch}");
    let upstream = match repo.find_reference(&upstream_ref) {
        Ok(reference) => reference.peel_to_commit()?,
        Err(_) => return Ok(()), // origin has no matching branch yet: nothing to rebase onto
    };
    let local = repo.head()?.peel_to_commit()?;
    // If local already contains upstream, a plain push fast-forwards; no rebase needed.
    if local.id() == upstream.id() || repo.graph_descendant_of(local.id(), upstream.id())? {
        return Ok(());
    }

    let onto = repo.find_annotated_commit(upstream.id())?;
    let signature = repo.signature()?;
    let mut rebase = repo.rebase(None, Some(&onto), None, None)?;

    let result = (|| -> Result<()> {
        while let Some(operation) = rebase.next() {
            operation.context("Rebase step failed")?;
            if repo.index()?.has_conflicts() {
                anyhow::bail!("Rebase conflict while integrating origin changes");
            }
            rebase.commit(None, &signature, None)?;
        }
        rebase.finish(Some(&signature))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = rebase.abort(); // restore HEAD; the local commit is preserved
    }
    result
}
```

- [ ] **Step 7: Add `sync_and_push` and refactor `push`**

Replace the existing `push` function with a version that takes an already-resolved `remote` and `branch`, and add `sync_and_push` above it:

```rust
fn sync_and_push(repo: &git2::Repository) -> Result<()> {
    let mut remote = match repo.find_remote("origin") {
        Ok(remote) => remote,
        Err(_) => return Ok(()), // no remote: local-only repository
    };
    let branch = repo
        .head()?
        .shorthand()
        .context("Cannot push from a detached HEAD")?
        .to_string();

    fetch_origin(repo, &mut remote)?;
    rebase_onto_origin(repo, &branch)?;
    push(repo, &mut remote, &branch)
}

fn push(repo: &git2::Repository, remote: &mut git2::Remote, branch: &str) -> Result<()> {
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let mut options = git2::PushOptions::new();
    options.remote_callbacks(remote_callbacks(repo.config()?));
    remote
        .push(&[refspec.as_str()], Some(&mut options))
        .context("Failed to push to origin")
}
```

- [ ] **Step 8: Call `sync_and_push` from `commit_and_push`**

In `commit_and_push`, replace the `if let Err(error) = push(&repo)` block with:

```rust
    // The commit is safe on disk; a failed fetch/rebase/push must not lose the note.
    if let Err(error) = sync_and_push(&repo) {
        eprintln!("Warning: committed locally but failed to push to origin: {error:#}");
    }
    Ok(())
```

- [ ] **Step 9: Run the new test to verify it passes**

Run: `cargo test --lib vcs::git::tests::push_rebases_onto_diverged_origin`
Expected: PASS.

- [ ] **Step 10: Run the full suite and lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass (including the existing `write_then_read_and_commit` and `open_existing_repo`), no fmt diff, no clippy warnings.

- [ ] **Step 11: Commit**

```bash
git add src/vcs/git.rs
git commit -m "feat: fetch and rebase onto origin before pushing notes"
```

---

### Task 2: Fall back gracefully on a genuine rebase conflict

If a note collides with a remote change on the same path, the rebase must not leave the repo mid-rebase or lose the note. `rebase_onto_origin` already aborts on conflict (Task 1, Step 6); this task locks that behavior in with a test proving the note survives locally, origin is untouched, and `write_file` still returns `Ok`.

**Files:**
- Test: `src/vcs/git.rs` (the `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: helpers from Task 1 (`origin_with_seed`, `commit_file`, `push_master`, `set_identity`), `GitBackend`.
- Produces: none (test-only).

- [ ] **Step 1: Write the conflict-fallback test**

Add to `mod tests`:

```rust
    #[test]
    fn conflicting_note_is_kept_locally_when_rebase_fails() {
        let (origin_dir, _seed_dir, seed) = origin_with_seed();
        let origin_url = origin_dir.path().to_str().unwrap();

        let workdir = tempfile::tempdir().unwrap();
        let noki = Repository::clone(origin_url, workdir.path()).unwrap();
        set_identity(&noki);

        // Origin gains a file at the SAME path noki is about to write, with different content.
        commit_file(&seed, "note.md", "origin\n", "remote note");
        push_master(&seed);

        let backend = GitBackend {
            workdir: workdir.path().to_path_buf(),
        };
        // Must not panic and must return Ok despite the unresolved rebase.
        backend.write_file("note.md", "local\n", "Add note").unwrap();

        // The local note is preserved (rebase was aborted, not applied).
        assert_eq!(backend.read_file("note.md").unwrap(), "local\n");

        // Origin was not overwritten: it still holds the remote content.
        let origin_repo = Repository::open_bare(origin_dir.path()).unwrap();
        let entry = origin_repo
            .head()
            .unwrap()
            .peel_to_tree()
            .unwrap()
            .get_path(Path::new("note.md"))
            .unwrap();
        let blob = origin_repo.find_blob(entry.id()).unwrap();
        assert_eq!(blob.content(), b"origin\n");
    }
```

- [ ] **Step 2: Run the test to verify it passes**

Run: `cargo test --lib vcs::git::tests::conflicting_note_is_kept_locally_when_rebase_fails`
Expected: PASS (the abort-on-conflict path from Task 1 handles this).

Note: this test guards behavior implemented in Task 1. If it FAILS, the conflict/abort handling in `rebase_onto_origin` (Task 1, Step 6) is wrong — fix it there, do not weaken the test.

- [ ] **Step 3: Run the full suite and lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/vcs/git.rs
git commit -m "test: keep note locally when rebase onto origin conflicts"
```

---

### Task 3: Update the architecture note in CLAUDE.md

The CLAUDE.md architecture section states push commits then "attempts to push". Update it to describe the new fetch → rebase → push flow and the conflict fallback, so the docs match the code.

**Files:**
- Modify: `CLAUDE.md:24`

**Interfaces:**
- Consumes: none.
- Produces: none.

- [ ] **Step 1: Update the push bullet**

Replace the bullet at `CLAUDE.md:24` (currently starting `- **Push is non-fatal**: ...`) with:

```markdown
- **Push is non-fatal, and syncs first**: `write_file` always commits locally, then runs fetch → rebase-onto-`origin/<branch>` → push. Fetching + rebasing avoids the permanent `NotFastForward` failure that happened when the remote diverged (e.g. a wiki page edited elsewhere). Because notes use unique timestamped paths they almost never conflict; a genuine same-path conflict aborts the rebase and falls back to the warning path. Any failure in this sequence (offline, rejected ref, no `origin`, conflict) prints a warning and returns `Ok`, so a note is never lost.
```

- [ ] **Step 2: Verify the change reads correctly**

Run: `git diff CLAUDE.md`
Expected: only the push bullet changed; no other edits.

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: document fetch-then-rebase push flow"
```

---

## Self-Review

**1. Spec coverage:**
- Fetch before push → Task 1, Steps 5, 7. ✓
- Rebase local commits onto `origin/<branch>`, linear history → Task 1, Steps 6, 7; asserted by `parent_count() == 1`. ✓
- Push still non-fatal / note never lost → Task 1, Step 8 keeps the `if let Err` warning wrapper; Task 2 proves the note survives a conflict. ✓
- Genuine-conflict fallback → Task 1, Step 6 (abort) + Task 2 (test). ✓
- Reuse credential logic for fetch and push → Task 1, Step 4 (`remote_callbacks`). ✓
- Docs match code → Task 3. ✓
- Out of scope (documented here so it is a conscious omission): making read paths (`ls`/`show`) fetch fresh content. The bug report is about push; reads remain `std::fs` over the working tree as today. A stale read is self-healing on the next write (which now rebases). Not implemented.

**2. Placeholder scan:** No TBD/TODO/"add error handling"/"similar to Task N". Every code step shows complete code; every run step shows the exact command and expected result. ✓

**3. Type consistency:** `remote_callbacks(git2::Config) -> git2::RemoteCallbacks<'static>`, `fetch_origin(&Repository, &mut Remote)`, `rebase_onto_origin(&Repository, &str)`, `sync_and_push(&Repository)`, and the refactored `push(&Repository, &mut Remote, &str)` are used consistently across Steps 4–8. Test helpers (`commit_file`, `set_identity`, `push_master`, `origin_with_seed`) are defined in Task 1, Step 1 and reused verbatim in Task 2. `GitBackend { workdir: PathBuf }` is constructed directly in tests (same module, private field accessible). ✓
