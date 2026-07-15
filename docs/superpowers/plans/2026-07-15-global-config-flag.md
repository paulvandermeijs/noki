# Global Config Flag (`--global`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `--global` flag that resolves configuration from the global config file only (skipping folder-level `.noki.toml`), and make `--repository` imply the same behavior.

**Architecture:** Thread a `global_only: bool` through `config::load`/`load_from`. When set — or whenever a `--repository` override is present — the local `.noki.toml` layer is skipped, leaving only the global file plus the override. A new global CLI boolean flag feeds this, and the two existing non-test callers (`main.rs`, `completion.rs`) pass it through.

**Tech Stack:** Rust, `clap` (derive), `anyhow`, `toml`, `directories`, `tempfile` (tests).

## Global Constraints

- Lint gate must pass before every commit: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`.
- No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code. Tests may `unwrap()` freely.
- Errors use `anyhow::Result` with `.context(...)`; do not introduce `thiserror`.
- Public API at the top of each file, private helpers at the bottom.
- The CLI command/binary is always lowercase `noki`; `Nōki` only in prose.
- TDD: write the failing test, watch it fail, make it pass, commit.
- **Gotcha:** `cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki`. Run `cargo build` before manually exercising the binary.

---

### Task 1: Add the `--global` CLI flag

**Files:**
- Modify: `src/cli.rs` (add field to `Cli` after the `repository` arg, ~line 30; add tests in the `tests` module)

**Interfaces:**
- Consumes: nothing (first task).
- Produces: `Cli.global: bool` — a global clap flag (`--global`, short `-g`) available on every subcommand. Later tasks read `cli.global`.

- [ ] **Step 1: Write the failing tests**

Add these two tests to the `#[cfg(test)] mod tests` block in `src/cli.rs`:

```rust
    #[test]
    fn parses_global_flag() {
        let cli = Cli::parse_from(["noki", "--global", "ls"]);
        assert!(cli.global);
        assert!(matches!(cli.command, Some(Commands::List { json: false })));
    }

    #[test]
    fn global_defaults_to_false() {
        let cli = Cli::parse_from(["noki", "ls"]);
        assert!(!cli.global);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib cli::tests::parses_global_flag`
Expected: FAIL to compile — `no field 'global' on type 'Cli'`.

- [ ] **Step 3: Add the flag field**

In `src/cli.rs`, insert the field into `struct Cli` immediately after the `repository` arg (the block ending at `pub repository: Option<String>,`):

```rust
    /// Ignore folder-level .noki.toml files; use only the global config
    #[arg(short = 'g', long, global = true)]
    pub global: bool,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib cli`
Expected: PASS (all `cli::tests`, including the two new ones).

- [ ] **Step 5: Lint gate and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add src/cli.rs
git commit -m "feat(cli): add --global flag"
```

Expected: lint gate clean; commit succeeds.

---

### Task 2: Thread `global_only` through config loading

**Files:**
- Modify: `src/config.rs` (change `load`/`load_from` signatures + body; update existing test call sites; add new tests)
- Modify: `src/main.rs:26` (pass `cli.global`)
- Modify: `src/completion.rs:29` (pass `false`)

**Interfaces:**
- Consumes: `Cli.global: bool` from Task 1.
- Produces:
  - `pub fn load(repository_override: Option<String>, global_only: bool) -> Result<Config>`
  - `pub(crate) fn load_from(global: Option<&Path>, start: &Path, repository_override: Option<String>, global_only: bool) -> Result<Config>`
  - Behavior: local `.noki.toml` layers are skipped when `global_only` is `true` **or** when `repository_override.is_some()`.

- [ ] **Step 1: Write the failing tests**

Add these five tests to the `#[cfg(test)] mod tests` block in `src/config.rs` (they use the new 4-arg `load_from`):

```rust
    #[test]
    fn global_flag_ignores_local_files() {
        let global_dir = tempfile::tempdir().unwrap();
        let global_path = global_dir.path().join("config.toml");
        fs::write(&global_path, "repository = \"global\"\n").unwrap();

        let local = tempfile::tempdir().unwrap();
        fs::write(local.path().join(".noki.toml"), "repository = \"local\"\n").unwrap();

        let config = load_from(Some(&global_path), local.path(), None, true).unwrap();
        assert_eq!(config.repository().unwrap(), "global");
    }

    #[test]
    fn global_flag_still_reads_global_file() {
        let global_dir = tempfile::tempdir().unwrap();
        let global_path = global_dir.path().join("config.toml");
        fs::write(
            &global_path,
            "repository = \"global\"\n\n[note]\nmax_width = 42\n",
        )
        .unwrap();

        let empty = tempfile::tempdir().unwrap();
        let config = load_from(Some(&global_path), empty.path(), None, true).unwrap();
        assert_eq!(config.repository().unwrap(), "global");
        assert_eq!(config.note.max_width, Some(42));
    }

    #[test]
    fn repository_override_wins_under_global() {
        let global_dir = tempfile::tempdir().unwrap();
        let global_path = global_dir.path().join("config.toml");
        fs::write(&global_path, "repository = \"global\"\n").unwrap();

        let empty = tempfile::tempdir().unwrap();
        let config = load_from(
            Some(&global_path),
            empty.path(),
            Some("from-cli".to_string()),
            true,
        )
        .unwrap();
        assert_eq!(config.repository().unwrap(), "from-cli");
    }

    #[test]
    fn repository_override_implies_global_only() {
        // global_only is false, but a --repository override is given: the local
        // layer must still be skipped (override forces global-only).
        let local = tempfile::tempdir().unwrap();
        fs::write(
            local.path().join(".noki.toml"),
            "repository = \"local\"\n\n[note]\nfilename = \"from-local\"\n",
        )
        .unwrap();

        let config =
            load_from(None, local.path(), Some("from-cli".to_string()), false).unwrap();

        assert_eq!(config.repository().unwrap(), "from-cli");
        // The local non-repository setting was ignored, proving the layer was skipped.
        assert_eq!(config.note.filename, None);
    }

    #[test]
    fn without_global_flag_local_still_wins() {
        let local = tempfile::tempdir().unwrap();
        fs::write(local.path().join(".noki.toml"), "repository = \"local\"\n").unwrap();
        let config = load_from(None, local.path(), None, false).unwrap();
        assert_eq!(config.repository().unwrap(), "local");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib config`
Expected: FAIL to compile — `load_from` takes 3 arguments but 4 were supplied (the new tests use the not-yet-existing 4-arg form).

- [ ] **Step 3: Change `load` and `load_from` signatures and body**

In `src/config.rs`, replace the current `load` and `load_from` (lines ~52-77) with:

```rust
/// Load configuration from the global config file, any `.noki.toml` files from
/// the current directory up to the filesystem root, and a CLI override. When
/// `global_only` is set — or a `repository_override` is given — the local
/// `.noki.toml` layer is skipped and only the global file (plus the override)
/// is used.
pub fn load(repository_override: Option<String>, global_only: bool) -> Result<Config> {
    let global = global_config_path();
    let start = std::env::current_dir()?;
    load_from(global.as_deref(), &start, repository_override, global_only)
}

pub(crate) fn load_from(
    global: Option<&Path>,
    start: &Path,
    repository_override: Option<String>,
    global_only: bool,
) -> Result<Config> {
    // A repository override also means "global config only".
    let global_only = global_only || repository_override.is_some();

    let mut config = match global {
        Some(path) if path.exists() => read_config(path)?,
        _ => Config::default(),
    };

    if !global_only {
        for path in local_config_paths(start) {
            config.merge(read_config(&path)?);
        }
    }

    if repository_override.is_some() {
        config.repository = repository_override;
    }

    Ok(config)
}
```

- [ ] **Step 4: Update the existing test call sites in `src/config.rs`**

Every existing `load_from(...)` call in the `tests` module needs a fourth argument, `false`. There are nine, in these tests:

- `cli_override_wins_over_files`: `load_from(None, dir.path(), Some("from-cli".to_string()), false)`
- `nearest_local_file_wins_over_ancestor`: `load_from(None, &child, None, false)`
- `parses_note_section`: `load_from(None, dir.path(), None, false)`
- `missing_repository_is_an_error`: `load_from(None, dir.path(), None, false)`
- `parses_list_section`: `load_from(None, dir.path(), None, false)`
- `max_visible_labels_defaults_to_three`: `load_from(None, dir.path(), None, false)`
- `parses_note_max_width`: `load_from(None, dir.path(), None, false)`
- `max_width_defaults_to_none`: `load_from(None, dir.path(), None, false)`
- `parses_daily_filename`: `load_from(None, dir.path(), None, false)`
- `parses_daily_title_and_label`: `load_from(None, dir.path(), None, false)`

(That is every `load_from(...)` in the file except the definition and the new tests, which already pass four args.)

- [ ] **Step 5: Update the two non-test callers**

In `src/main.rs`, change line ~26 from:

```rust
    let config = config::load(cli.repository)?;
```

to:

```rust
    let config = config::load(cli.repository, cli.global)?;
```

In `src/completion.rs`, change line ~29 from:

```rust
    let config = config::load(None).ok()?;
```

to:

```rust
    let config = config::load(None, false).ok()?;
```

(Completion always uses full, folder-aware resolution.)

- [ ] **Step 6: Run the full test suite to verify it passes**

Run: `cargo test`
Expected: PASS — all `config::tests` (existing + five new) and the rest of the suite. If compilation fails, a `load`/`load_from` call site was missed.

- [ ] **Step 7: Lint gate and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings
git add src/config.rs src/main.rs src/completion.rs
git commit -m "feat(config): --global and --repository skip folder config"
```

Expected: lint gate clean; commit succeeds.

---

### Task 3: Document `--global`

**Files:**
- Modify: `README.md` (after the `.noki.toml` config block, before "Capture a note")
- Modify: `CLAUDE.md` (Config paragraph in the Architecture section)

**Interfaces:**
- Consumes: the finished flag behavior from Tasks 1-2.
- Produces: no code.

- [ ] **Step 1: Add README usage note**

In `README.md`, immediately after the closing ```` ``` ```` of the config TOML block (the line after `max_visible_labels = 3` / its closing fence, ~line 50) and before `Capture a note (opens your editor):`, insert:

````markdown
By default noki merges the global config with any `.noki.toml` from the current
directory upward (nearest wins). To bypass all folder-level config and use only
your global config — handy for switching between a project's repo and your
global notes repo — pass `--global` (short `-g`):

```sh
noki --global ls
```

Passing `--repository <url>` also implies `--global`: it ignores folder-level
config and points noki at the given repository.

````

- [ ] **Step 2: Update CLAUDE.md Config paragraph**

In `CLAUDE.md`, find the sentence ending `then the `--repository` CLI flag. `load_from` is the injectable seam that tests drive with temp dirs.` and append one sentence so it reads:

```markdown
then the `--repository` CLI flag. `load_from` is the injectable seam that tests drive with temp dirs. The `--global` flag (and any `--repository` override, which implies it) skips the local `.noki.toml` layer entirely, resolving from the global file plus the override only.
```

- [ ] **Step 3: Verify docs build/format is unaffected**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: PASS (docs-only change; nothing to break, but confirm the tree is still clean).

- [ ] **Step 4: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: document the --global flag"
```

Expected: commit succeeds.

---

## Self-Review

**1. Spec coverage:**
- Decision #2 (skip local layers under `--global`) → Task 2, `global_flag_ignores_local_files`.
- Decision #3 (`--repository` implies global-only) → Task 2, body `global_only || repository_override.is_some()` + `repository_override_implies_global_only`.
- Decision #4 (global flag `--global`/`-g`, `global = true`) → Task 1.
- CLI surface → Task 1. Config loader → Task 2. Dispatch (main.rs) + completion.rs call site → Task 2. Precedence-with-`--repository` → Task 2 (`repository_override_wins_under_global`). Non-goals (no info command, no default-mode key) → respected; nothing added. Tests 1-6 from the spec → Task 2 (five config tests) + Task 1 (`parses_global_flag`; `global_defaults_to_false` added as the "no flag" half). Files touched (cli/config/main/completion/README/CLAUDE.md) → covered across Tasks 1-3. Skills unchanged → no task, matches spec.

**2. Placeholder scan:** No TBD/TODO/"handle edge cases"/"similar to Task N". Every code step shows full code; every command has expected output.

**3. Type consistency:** `load(Option<String>, bool)` and `load_from(Option<&Path>, &Path, Option<String>, bool)` used identically in the definition (Task 2 Step 3), the new tests (Step 1), the updated existing tests (Step 4), `main.rs` (`config::load(cli.repository, cli.global)`), and `completion.rs` (`config::load(None, false)`). Field `Cli.global: bool` defined in Task 1, read in Task 2. Consistent throughout.
