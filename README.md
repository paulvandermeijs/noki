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
filename = "%Y/%m/%d/%H:%M:%S-%title"
daily_filename = "%Y/%m/%d"
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
default `%Y/%m/%d`). If today's note already exists it opens pre-filled for you
to update; otherwise it is created with the title `Daily note for <date>`. Piped
input is appended to an existing daily note. Every daily note is tagged with the
`daily` label:

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

Edit an existing note (opens your editor with the note's body; on save the
`updated` timestamp is refreshed while `created` is preserved, and the title,
labels, and other frontmatter are kept as-is):

```sh
noki edit 2026/06/02/10:00:00-my-new-note.md
```

## License

[MIT](LICENSE)
