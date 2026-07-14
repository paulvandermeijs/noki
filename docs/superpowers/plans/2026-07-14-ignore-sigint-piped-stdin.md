# Ignore SIGINT While Reading Piped Stdin — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `noki --no-edit` (and `--daily --no-edit`) survive Ctrl+C in a pipeline so `yap dictate | noki -n` stores the transcript instead of dying with the producer.

**Architecture:** Ctrl+C sends SIGINT to every process in the foreground pipeline. Producers like `yap dictate` trap it to flush their final output, but noki has default disposition and dies mid-`read_to_string`, losing the note. The fix is one seam: `io::read_stdin()` (`src/io.rs`) — the only stdin entry point, used by both `commands/create.rs:16` and `commands/daily.rs:21`. After confirming stdin is piped (not a terminal), set SIGINT to `SIG_IGN` before the blocking read. Ctrl+C then stops only the producer; the pipe closes, noki reads EOF, and the note is stored and committed as normal. The disposition is deliberately **not** restored: noki is short-lived, and everything after the read (write, commit, push) is exactly the work we want to protect. The interactive-editor path is untouched — `read_stdin` returns `None` on a terminal before the ignore is installed, so Ctrl+C still cancels the editor flow.

**Tech Stack:** Rust 2024, `libc` crate (unix-only target dependency) for `signal(SIGINT, SIG_IGN)`. No new runtime machinery — no handler thread, no `ctrlc` crate. `std`'s `read_to_string` already retries `EINTR`, but `SIG_IGN` avoids interruption entirely and matches the shell-level behavior verified with `trap '' INT`.

## Global Constraints

- Lint gate before every commit: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
- No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code; tests may `unwrap()` freely
- Errors use `anyhow::Result` with `.context(...)` (not needed here — the new code returns nothing)
- Public API at the top of each file, private helpers at the bottom
- The name is `Nōki` in prose, `noki` for the command/binary/crate
- `cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki` — run `cargo build` before manually exercising the binary
- **NEVER stage `docs/superpowers/specs/2026-07-13-global-config-flag-design.md`** — it belongs to a separate in-flight feature. Always `git add` explicit paths; never `git add -A`, `git add .`, or `git add docs/`.

---

### Task 1: `ignore_sigint` helper wired into `read_stdin`

**Files:**
- Modify: `Cargo.toml` (add unix-only `libc` dependency)
- Modify: `src/io.rs` (call site at the top, private helper + test at the bottom)

**Interfaces:**
- Consumes: `libc::signal`, `libc::SIG_IGN`, `libc::SIGINT`
- Produces: `fn ignore_sigint()` — private to `io.rs`, `#[cfg(unix)]` real / `#[cfg(not(unix))]` no-op; called from `read_stdin()` only after the `is_terminal()` early return. No other module sees it.

- [ ] **Step 1: Add the `libc` dependency**

Append to `Cargo.toml` (after the existing `[dependencies]` table, which currently ends with `clap_complete` on line 29):

```toml

[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

- [ ] **Step 2: Write the failing test**

In `src/io.rs`, inside the existing `mod tests`, after `clean_stdin_trims_and_empties_to_none`:

```rust
    #[cfg(unix)]
    #[test]
    fn ignore_sigint_marks_sigint_ignored() {
        ignore_sigint();

        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::sigaction(libc::SIGINT, std::ptr::null(), &mut action) };
        assert_eq!(rc, 0);
        assert_eq!(action.sa_sigaction, libc::SIG_IGN);
    }
```

(Querying `sigaction` with a null new-action pointer only reads the current disposition. Setting `SIG_IGN` is process-global test state, but no other test in the crate sends or observes signals, so it cannot interfere.)

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --lib io::tests::ignore_sigint_marks_sigint_ignored`
Expected: FAIL to compile with `cannot find function `ignore_sigint` in this scope` (compile error counts as the red step here).

- [ ] **Step 4: Write the implementation**

Two edits to `src/io.rs`.

First, the call site — `read_stdin` becomes (only the `ignore_sigint();` line is new):

```rust
pub fn read_stdin() -> Option<String> {
    use std::io::IsTerminal;

    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return None;
    }

    ignore_sigint();

    let mut buffer = String::new();
    if stdin.lock().read_to_string(&mut buffer).is_err() {
        return None;
    }
    clean_stdin(&buffer)
}
```

Second, the private helper at the **bottom** of the file, after `clean_stdin` and before `mod tests` (public API stays at the top):

```rust
/// Ctrl+C delivers SIGINT to every process in the foreground pipeline. A
/// producer like `yap dictate` traps it to flush its final output, but noki
/// would die mid-read and lose the note. Ignoring SIGINT lets the read run
/// to EOF, so the note is stored no matter how the producer is stopped. Not
/// restored afterwards: the process is short-lived and everything after the
/// read is the work being protected.
#[cfg(unix)]
fn ignore_sigint() {
    // SAFETY: `signal` with SIG_IGN installs no user handler and reads no
    // user data; the only effect is process-global disposition, set once
    // before the blocking read.
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }
}

#[cfg(not(unix))]
fn ignore_sigint() {}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --lib io`
Expected: PASS — both `clean_stdin_trims_and_empties_to_none` and `ignore_sigint_marks_sigint_ignored` green.

- [ ] **Step 6: Run the full suite and the lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass, no formatting diff, no clippy warnings. (If clippy flags the `unsafe` block, it will be `clippy::missing_safety_doc`-adjacent noise only — the `// SAFETY:` comment above is already in place; do not silence lints with `#[allow]` without checking the message first.)

- [ ] **Step 7: Commit (explicit paths only)**

```bash
git add Cargo.toml Cargo.lock src/io.rs
git commit -m "feat(io): ignore SIGINT while reading piped stdin"
```

`Cargo.lock` changes because of the new `libc` entry; include it. Do NOT add anything under `docs/superpowers/specs/`.

---

### Task 2: End-to-end verification in a real pty + README note

**Files:**
- Modify: `README.md:58-62` (piped-input section)
- No source changes — this task proves the behavior against the real binary and documents it.

**Interfaces:**
- Consumes: the built `target/debug/noki` from Task 1; `/usr/bin/expect` (ships with macOS) to press Ctrl+C in a real pseudo-TTY; a throwaway local bare repo so the user's real notes are never touched.
- Produces: nothing for later tasks — this is the final task.

- [ ] **Step 1: Build the binary**

Run: `cargo build`
Expected: clean build. (`cargo test` does not produce `target/debug/noki` — this step is required, per the repo gotcha.)

- [ ] **Step 2: Create a throwaway notes remote**

```bash
rm -rf /tmp/noki-sigint-e2e && mkdir -p /tmp/noki-sigint-e2e
git init --bare --initial-branch=master /tmp/noki-sigint-e2e/notes.git
```

Expected: `Initialized empty Git repository in /tmp/noki-sigint-e2e/notes.git/`

- [ ] **Step 3: Write the pty test script**

Create `/tmp/noki-sigint-e2e/ctrl_c.exp`:

```tcl
#!/usr/bin/expect -f
# Producer mimics `yap dictate`: traps INT, flushes final text, exits.
# Ctrl+C hits the whole pipeline; noki must survive and store the note.
set noki [lindex $argv 0]
spawn zsh -c "zsh -c 'trap \"print the dictated note; exit 0\" INT; sleep 30' | $noki --no-edit --repository /tmp/noki-sigint-e2e/notes.git"
sleep 2
send "\003"
expect eof
```

- [ ] **Step 4: Run it and verify the note was stored**

```bash
expect /tmp/noki-sigint-e2e/ctrl_c.exp "$PWD/target/debug/noki"
./target/debug/noki ls --repository /tmp/noki-sigint-e2e/notes.git
```

Expected: `ls` shows exactly one note titled `the dictated note`. Then confirm the content survived intact:

```bash
./target/debug/noki show --repository /tmp/noki-sigint-e2e/notes.git --raw \
  "$(./target/debug/noki ls --repository /tmp/noki-sigint-e2e/notes.git --json | jq -r '.[0].path')"
```

Expected: raw note whose body is `the dictated note`.

**Regression check (proves the test can fail):** `git stash` the `src/io.rs` change, `cargo build`, re-run Step 4 against a fresh bare repo — `ls` must show no note. `git stash pop` and `cargo build` again afterwards. Skip this check if the stash would drag in unrelated working-tree changes; note in the task report that it was skipped and why.

- [ ] **Step 5: Clean up the throwaway repo**

```bash
rm -rf /tmp/noki-sigint-e2e
```

Also remove the cloned working copy noki created for that URL under the OS data dir (it is keyed per-URL; find it before deleting):

```bash
find ~/Library/Application\ Support -maxdepth 3 -type d -name '*noki-sigint-e2e*' 2>/dev/null
```

Delete only what that find returns.

- [ ] **Step 6: Document the behavior in the README**

In `README.md`, extend the piped-input section. Replace:

```markdown
Capture piped input without opening the editor:

```sh
echo "A quick note" | noki --no-edit
```
```

with:

```markdown
Capture piped input without opening the editor:

```sh
echo "A quick note" | noki --no-edit
```

Interactive producers work too — noki ignores Ctrl+C while reading piped
input, so stopping a live producer still stores everything it flushed on
the way out:

```sh
yap dictate | noki --no-edit
```
```

- [ ] **Step 7: Lint gate and commit (explicit paths only)**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: clean.

```bash
git add README.md
git commit -m "docs: document Ctrl+C-safe piped capture"
```

Do NOT add anything under `docs/superpowers/specs/`.

---

## Self-Review

- **Spec coverage:** the behavior is fully specified by the goal — piped `--no-edit` capture survives pipeline Ctrl+C. Task 1 implements it at the single stdin seam (covers both `create` and `daily` call sites); Task 2 proves it end-to-end in a real pty and documents it. `skills/capturing-notes/SKILL.md` is intentionally untouched: the CLI surface, output shapes, and editor mechanism are unchanged, and agents never send Ctrl+C — the sync rule in CLAUDE.md is not triggered.
- **Placeholder scan:** none — every step carries the exact code, command, or expected output.
- **Type consistency:** `ignore_sigint()` is defined and consumed only inside `src/io.rs`; the test references it unqualified from the child `tests` module, which resolves via `use super::*` already present.
