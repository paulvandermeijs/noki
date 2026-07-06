# Nōki

A version control powered, AI friendly note taking app.

## About

Nōki stores your notes as plain files under version control, so every change
is tracked, diffable, and recoverable. An agent-friendly CLI, with piped
input and structured output, makes your notes easy for AI tools to drive,
search, and reason over.

## Installation

Install from [crates.io](https://crates.io/crates/noki) (requires a Rust toolchain):

```sh
cargo install noki
```

This puts the `noki` binary on your `PATH`. Upgrade later with `cargo install noki --force`.

## Usage

Set your notes repository once in `.noki.toml` (or pass `--repository`):

```toml
repository = "git@github.com:you/notes.git"

[note]
# Filename templates use {field} tokens drawn from the note's frontmatter:
# {title}, {labels}, {created:%Y/%m/%d}, {updated:...}, and any custom meta key
# (e.g. {author}). Date tokens take a chrono strftime format (default %Y-%m-%d).
# A missing or empty value renders as "unknown-<field>" (e.g. unknown-author).
filename = "{created:%Y/%m/%d/%H:%M:%S}-{title}"
daily_filename = "{created:%Y/%m/%d}"
# daily_title uses the same {field} tokens but rendered verbatim (not slugified,
# since a title is human-readable). Only {created}/{updated} and meta keys resolve
# here — not the path-only {title}/{labels}.
daily_title = "Daily note for {created:%Y-%m-%d}"
daily_label = "daily"
# Render `show` at a fixed width, in columns: the body wraps at it and the
# metadata table is sized to it (clamped down to the terminal width when the
# terminal is narrower). Default: unset — adapt to content up to terminal width.
max_width = 100
meta = { author = "Your Name" }

[list]
# Maximum number of labels shown per note in `ls` before "+N more" (default: 3)
max_visible_labels = 3
```

Capture a note (opens your editor):

```sh
noki
```

Capture piped input without opening the editor:

```sh
echo "A quick note" | noki --no-edit
```

Set a custom title and attach labels (repeat `--label` for several):

```sh
noki --title "Sprint planning" --label work --label meeting
```

Open or create today's daily note (its path comes from `note.daily_filename`,
default `{created:%Y/%m/%d}`). If today's note already exists it opens pre-filled for you
to update; otherwise it is created with a title from `note.daily_title` (default
`Daily note for {created:%Y-%m-%d}`). Piped input is appended to an existing daily note.
Every daily note is tagged with `note.daily_label` (default `daily`):

```sh
noki --daily
echo "Shipped the release" | noki --daily --no-edit
```

List notes:

```sh
noki ls
noki ls --json
```

Labels render as color-coded chips in the human (non-JSON) output of both `ls` and `show` when writing to a terminal; piped or redirected output falls back to plain comma-separated text. In the list view, labels are truncated to `max_visible_labels` (configurable in `[list]`, default 3) with a `+N more` marker; the single-note `show` view lists all labels.

Show a single note:

```sh
noki show 2026/06/02/10:00:00-my-new-note.md
noki show 2026/06/02/10:00:00-my-new-note.md --json
noki show 2026/06/02/10:00:00-my-new-note.md --raw
```

In a terminal, `show` renders the note's Markdown body as styled text. Without
`note.max_width` it wraps to the terminal width and the metadata table adapts to
its content; with `note.max_width` set it renders at that fixed width instead —
the body wraps at it and the metadata table is sized to it — clamped down to the
terminal width when the terminal is narrower. Piped or redirected output emits
the raw Markdown source unchanged; `--json` and `--raw` are always unformatted.

Edit an existing note (opens your editor with the note's body; on save the
`updated` timestamp is refreshed while `created` is preserved, and the title,
labels, and other frontmatter are kept as-is):

```sh
noki edit 2026/06/02/10:00:00-my-new-note.md
```

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

## Agent skills

Nōki ships with two [agent skills](skills/) that teach an AI coding agent (Claude
Code or compatible) how to drive the CLI:

- [`capturing-notes`](skills/capturing-notes/SKILL.md) — capturing notes
  non-interactively (`--no-edit` with piped input, titles, labels, daily notes)
  and editing existing notes via a scripted `$VISUAL`.
- [`retrieving-notes`](skills/retrieving-notes/SKILL.md) — finding and reading
  notes through the structured `--json` output, including the
  list → filter → show search idiom.

Install them by copying the relevant `skills/<name>/` directory into your agent's
skills location (e.g. `.claude/skills/`). Each `evals/` folder holds the test
prompts used to validate the skill.

## About the name

Nōki is a portmanteau of **no**tes and wi**ki** — but it's pronounced like the
Limp Bizkit song.

## License

[MIT](LICENSE)
