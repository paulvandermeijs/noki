# Noki

A version control powered, AI friendly note taking app.

> [!WARNING]
> Early work in progress.

## About

Noki stores your notes as plain files under version control, so every change
is tracked, diffable, and recoverable. An agent-friendly CLI, with piped
input and structured output, makes your notes easy for AI tools to drive,
search, and reason over.

## Building

```sh
cargo build
```

## Running

```sh
cargo run
```

## Usage

Set your notes repository once in `.noki.toml` (or pass `--repository`):

```toml
repository = "git@github.com:you/notes.git"

[note]
# Filename templates use {field} tokens drawn from the note's frontmatter:
# {title}, {labels}, {created:%Y/%m/%d}, {updated:...}, and any custom meta key
# (e.g. {author}). Date tokens take a chrono strftime format (default %Y-%m-%d).
# A missing or empty value renders as "unknown-<field>" (e.g. unknown-author).
filename = "{created:%Y/%m/%d/%H-%M-%S}-{title}"
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

## Agent skills

Noki ships with two [agent skills](skills/) that teach an AI coding agent (Claude
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

## License

[MIT](LICENSE)
