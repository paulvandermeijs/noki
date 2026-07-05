# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Noki is a CLI that captures Markdown notes into a Git-backed repository. Notes are Markdown files with YAML frontmatter, committed and pushed to a configured remote.

## Commands

- Build: `cargo build`
- Test everything: `cargo test`
- Test one module: `cargo test --lib note` (or `config`, `vcs`, `output`, `commands`, `cli`)
- Test one case: `cargo test --lib note::tests::round_trips_through_to_raw`
- Lint gate (must pass before every commit): `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
- Run the CLI: `cargo run -- ls --repository <url>` (default `cargo run` with no subcommand opens `$EDITOR` to capture a note)

**Gotcha:** `cargo test` and `cargo clippy` do NOT rebuild `target/debug/noki`. Before manually exercising the binary, run `cargo build` (or `cargo run`), or you will test a stale binary.

## Architecture

The crate is a **library (`src/lib.rs`) plus a thin binary (`src/main.rs`)**. `main.rs` only parses the CLI, loads config, opens a storage backend, and dispatches to `commands::{create,list,show}::run`. All real logic lives in the library so it is unit-testable.

**Storage is decoupled behind the `VersionControl` trait** (`src/vcs/mod.rs`): `list_files` / `read_file` / `write_file`. This is the key seam to understand:
- `GitBackend` (`src/vcs/git.rs`) is the only real implementation. It uses **two Git libraries on purpose**: `gix` (gitoxide) for clone/open, and `git2` (libgit2) for stage/commit/push. `gix` is native but cannot push; `git2` has the SSH/HTTPS transports (enabled via vendored features in `Cargo.toml`). Reads bypass both libraries and use `std::fs` on the checked-out working tree.
- **Push is non-fatal, and syncs first**: `write_file` always commits locally, then runs fetch → rebase-onto-`origin/<branch>` → push. Fetching + rebasing avoids the permanent `NotFastForward` failure that happened when the remote diverged (e.g. a wiki page edited elsewhere). Because notes use unique timestamped paths they almost never conflict; a genuine same-path conflict aborts the rebase and falls back to the warning path. Any failure in this sequence (offline, rejected ref, no `origin`, conflict) prints a warning and returns `Ok`, so a note is never lost.
- Command functions take `&dyn VersionControl`, so they are tested against the `#[cfg(test)]` `MemoryBackend` in `src/vcs/mod.rs` — command/note/output logic is fully tested without touching Git. `vcs::open_backend()` is the factory that clones the configured repo into a per-URL directory under the OS data dir.

**Note model** (`src/note.rs`): a note is Markdown with YAML (`---`) or TOML (`+++`) frontmatter, parsed via the **markdown-rs AST** (`parse_note` reads the frontmatter node and slices the body by byte offset; `title_from_content` walks the AST for the first heading or paragraph). `Meta` has fixed fields plus `#[serde(flatten)] extra` for static config meta (e.g. `author`). `to_raw` always writes YAML. Timestamps are RFC 3339 strings via the custom `rfc3339` serde module.

**Config** (`src/config.rs`) is layered, lowest to highest precedence: a global file (`directories` config dir → `noki/config.toml`), then every `.noki.toml` from the current directory up to the filesystem root (nearest wins), then the `--repository` CLI flag. `load_from` is the injectable seam that tests drive with temp dirs.

**Output** (`src/output.rs`) renders a note three ways: human (a `tabled` metadata table + body, including `extra` meta rows), JSON (serde; list output omits `content`), and raw (unmodified file text). `create` derives the filename from a flat template rendered by `src/template.rs`: `{field}` / `{field:format}` tokens projected from the note's frontmatter — `{title}` and `{labels}` (slugified), `{created:%Y/%m/%d}` / `{updated:…}` (chrono-formatted), and any static config meta key (e.g. `{author}`). A missing or empty value renders as `unknown-<field>`; only template *syntax* errors (bad date format, `:format` on a text field, unterminated `{`) return `Err` — it never panics. The default is `{created:%Y/%m/%d/%H-%M-%S}-{title}`; `note_path` appends `.md`. `note.daily_title` still uses chrono `%` directly (it is a title, not a path). `create` also merges config static meta into frontmatter, skipping reserved keys.

## Conventions

- **Errors use `anyhow` throughout, including the library — this is deliberate**, not an oversight. Noki's lib is the binary's internals; nothing branches on error kinds, so `thiserror` earns nothing here. Keep using `anyhow::Result` with `.context(...)`.
- **No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code**, with one justified `expect()` in `collect_notes` (path is provably within the repo root). Tests may `unwrap()` freely.
- **Public API at the top of each file, private helpers at the bottom** (a global rule for this author).
- TDD is the norm: write the failing test, make it pass, then commit. Implementation plans live in `docs/superpowers/plans/`.
- **Keep the agent skills in sync with the CLI.** Two skills ship in `skills/` (`capturing-notes`, `retrieving-notes`) that document the CLI for AI agents, making concrete claims about flags, `--json` output shapes, and the `$VISUAL`/`$EDITOR` edit mechanism. When you change the CLI surface (`src/cli.rs`), the output shapes (`src/output.rs`/`src/note.rs`), or editor behavior (`src/editor.rs`/`src/commands/edit.rs`), update the affected `skills/*/SKILL.md` in the same change so the skills don't drift.
