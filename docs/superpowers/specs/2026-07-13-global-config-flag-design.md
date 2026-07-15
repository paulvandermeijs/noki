# Global Config Flag (`--global`) — Design

**Status:** Draft (brainstorming) — awaiting user review

## Goal

Add a `--global` flag that makes noki resolve configuration from the **global
config file only**, ignoring every folder-based `.noki.toml`. The concrete need
is to switch cleanly between a **global notes repository** and a
**project-specific repository** without having to `cd` out of the project or
delete local config:

```sh
noki ls              # project repo (nearest .noki.toml wins)
noki --global ls     # global notes repo (local .noki.toml ignored)
```

This item is from the noki todo: *"Support a `--global` option to ignore folder
level config and interact with the global/default repo."*

## Decisions (from brainstorming)

1. **Scope split.** `--global` is being built on its own, ahead of any config
   *inspection* command. The originally-discussed `info` command (with
   per-value provenance and git repo-state) was judged too heavy for the actual
   need and is **deferred** — see "Follow-up" below. `--global` is the
   prerequisite behavior change and is small and self-contained.
2. **What `--global` skips.** It skips the local `.noki.toml` layer(s) only. The
   **global config file still loads**, and the `--repository` CLI override still
   applies at highest precedence.
3. **`--repository` implies global-only.** Passing `--repository <url>` also
   ignores the local `.noki.toml` layers (as if `--global` were set), then
   overrides the repository with the given value. Naming a repository explicitly
   steps out of the current folder's project context, so folder config —
   including non-repository settings like the filename template, `meta`, and
   daily config — should not leak in. Effective rule:
   `global_only = --global || --repository is set`.
4. **Flag shape.** A global boolean flag `--global` (short `-g`), declared like
   the existing `--repository` global arg so it composes with any subcommand
   (`noki --global ls`, `noki --global show …`, and — once it exists —
   `noki --global info`).

## Background — how config loads today (ground truth)

Verified against `src/config.rs` and `src/main.rs`.

Precedence, lowest to highest (`config::load` → `config::load_from`):

1. Global config file — `<config dir>/noki/config.toml` (via `directories`).
2. Every `.noki.toml` from the filesystem root down to the current directory,
   nearest last (nearest wins). Collected by `local_config_paths`.
3. The `--repository` CLI override (sets `config.repository`).

`load_from(global, start, repository_override)` reads the global file, folds in
each local file via `Config::merge`, then applies the override. `main.rs::run`
calls `config::load(cli.repository)` and then `vcs::open_backend(&config)`.

## Design

### CLI surface (`src/cli.rs`)

Add a global flag to `Cli`, mirroring `--repository`:

```rust
/// Ignore folder-level .noki.toml files; use only the global config
#[arg(short = 'g', long, global = true)]
pub global: bool,
```

`global = true` makes it available on every subcommand, so `noki --global info`
parses whether the flag precedes or follows the subcommand.

### Config loader (`src/config.rs`)

Thread a `global_only: bool` through the loader. `load_from` gains the
parameter and, when set, skips the local layer entirely:

```rust
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

    let mut config = /* read global file as today */;

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

The `--repository`-implies-global-only rule lives **inside `load_from`** (not in
`main.rs`), so it is exercised by the config unit tests and applies uniformly to
every caller. Everything else — `Config`, `merge`, `local_config_paths`,
`read_config` — is unchanged. The change is purely "skip step 2 when
`global_only`, and treat a repository override as forcing `global_only`".

### Dispatch (`src/main.rs`)

`run` passes the flag through:

```rust
let config = config::load(cli.repository, cli.global)?;
```

No other dispatch change. All subcommands transparently benefit because they
receive the already-resolved `config`/`backend`.

### Precedence with `--repository`

`--repository` **implies** global-only, so the two flags reinforce rather than
conflict. Whenever `--repository` is set (with or without `--global`): the local
`.noki.toml` layers are skipped, the global config file loads, and the given
repository value overrides whatever the global file specified. Passing both
`--global` and `--repository` is therefore equivalent to `--repository` alone
plus the (redundant) explicit `--global`.

## Non-goals / YAGNI

- **No `info`/inspection command in this change.** Verifying which repo
  `--global` selects is deferred (see Follow-up). For now it can be observed by
  running a command that reveals the repository (or via the follow-up).
- No new config *key* to set a default "global mode" — the flag is per-invocation.
- No change to how the global config path is discovered.

## Testing (TDD)

`src/config.rs` unit tests (drive `load_from` with temp dirs, the existing seam):

1. `global_flag_ignores_local_files` — a local `.noki.toml` sets
   `repository = "local"`; with `global_only = true` and a global file setting
   `repository = "global"`, the resolved repository is `"global"`.
2. `global_flag_still_reads_global_file` — no local files; `global_only = true`
   still loads values from the global file.
3. `repository_override_wins_under_global` — `global_only = true` plus a
   `--repository` override yields the override.
4. `repository_override_implies_global_only` — `global_only = false` but a
   `--repository` override is given, *and* a local `.noki.toml` sets a
   non-repository value (e.g. `note.filename` or `note.meta`); the resolved
   config uses the override repository **and** ignores the local
   non-repository value (proving the local layer was skipped, not just the
   repository field overridden).
5. `without_global_flag_local_still_wins` — regression: `global_only = false`
   and **no** repository override keeps today's nearest-local-wins behavior.

Existing `load_from` call sites in tests get the new `false` argument.

`src/cli.rs`:

6. `parses_global_flag` — `noki --global ls` sets `cli.global` and parses the
   subcommand; `noki` alone leaves `cli.global == false`.

## Files touched

- `src/cli.rs` — add `global` field; add parse test.
- `src/config.rs` — add `global_only` param to `load`/`load_from`; add tests;
  update existing test call sites.
- `src/main.rs` — pass `cli.global` to `config::load`.
- `src/completion.rs` — the `config::load(None)` call site (line ~29) passes
  `false`; note-path completion always uses full (folder-aware) resolution.
- `README.md` / `CLAUDE.md` — document the flag (README usage; CLAUDE.md config
  section note that `--global` skips the local layer).

No `skills/` change: `--global` is a config-resolution flag, not a change to the
capture/retrieval surfaces the shipped skills document.

## Follow-up (deferred, separate spec)

A minimal `noki info` command to *verify* config resolution — scoped to the
actual need, **not** the heavy provenance/repo-state design first explored:

- Show the **resolved repository**, the **local clone path**
  (`vcs::clone_dir`, to be exposed), and the **ordered list of config files
  that were loaded**.
- Under `--global` the local `.noki.toml` files are simply absent from that
  loaded-files list — which is the confirmation the user wants.
- Human-readable only, rendered with `tabled` to match `show`.

This is intentionally out of scope here and will get its own spec once
`--global` lands.
