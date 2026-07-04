---
name: capturing-notes
description: Capture or edit a Markdown note in the user's noki note repository — a git-backed store that commits and pushes every change. Use this whenever the user explicitly asks to save, jot, record, log, or note something down with noki ("save this as a note", "add this to my noki", "jot this down", "log this to my journal", "make/append today's daily note"), OR to change, update, fix, or append to an existing note ("update the note at …", "fix the second bullet in my sprint note", "edit that note"). Because notes are committed to a git remote, only capture or edit on an explicit request — never automatically.
---

# Capturing notes with noki

`noki` captures a Markdown note into a git-backed repository: it writes the file, commits it, and pushes it to the configured remote. Treat it as durable storage the user will keep, not a scratch buffer.

## The one rule that matters: never open the editor

Bare `noki` (and `noki edit`) launches `$EDITOR` and **blocks waiting for an interactive editor** — an agent has no terminal editor, so it would hang. Always supply the note body on **stdin** and pass `--no-edit` so noki stores the piped input directly.

```sh
printf '%s\n' "Remember to rotate the API keys before Friday." | noki --no-edit
```

Use `printf` (or a heredoc) rather than `echo` when the body has multiple lines or Markdown, so newlines and characters survive intact:

```sh
noki --no-edit <<'EOF'
# Sprint retro

- Shipped the release
- Follow up on the flaky test
EOF
```

## Set the title and labels explicitly

Without `--title`, noki derives the title from the note's first heading or paragraph. When you know the title, set it — it makes the note easier to find later. Add labels with `--label`, repeating the flag for several (there is no short form):

```sh
printf '%s\n' "Discussed Q3 roadmap and staffing." \
  | noki --no-edit --title "Sprint planning" --label work --label meeting
```

## Daily notes

`--daily` targets today's daily note (a single note per day). Combined with `--no-edit` and stdin, it **appends** to today's note if it already exists, or creates it if it doesn't — ideal for logging progress through the day:

```sh
printf '%s\n' "Shipped the 1.2 release." | noki --daily --no-edit
```

## Before you capture: a repository must be configured

noki writes into the repository set in a `.noki.toml` (in the working directory or any parent), a global config, or the `--repository <url>` flag. If none is configured, noki exits with an error instead of writing. If you hit that error, tell the user they need to set `repository` in `.noki.toml` (or pass `--repository`) — don't guess a URL.

## Editing an existing note

`noki edit <path>` updates a note that already exists. By default it opens the note's body in an interactive editor, which an agent can't drive. But under the hood noki just writes the note's **body** to a temporary `.md` file, runs your editor on that file, then reads it back — so you can edit non-interactively by pointing the editor at a small script that rewrites the temp file.

The robust, cross-platform way is to compute the full new body yourself and have the script drop it in. First read the current body:

```sh
noki show "2026/06/02/10:00:00-sprint-planning.md" --raw
```

Work out the new body (the part after the frontmatter), write it to a file, then run the edit with a one-line script that copies it into place:

```sh
printf '%s\n' "$NEW_BODY" > /tmp/new-body.md
cat > /tmp/noki-edit.sh <<'SH'
#!/bin/sh
cp /tmp/new-body.md "$1"   # $1 is noki's temp file holding the note body
SH
chmod +x /tmp/noki-edit.sh
VISUAL=/tmp/noki-edit.sh noki edit "2026/06/02/10:00:00-sprint-planning.md"
```

noki then re-saves the note: it refreshes `updated`, preserves `created`, and keeps the title, labels, and other frontmatter — then commits and pushes like any capture. (For a quick mechanical change your script can run `sed` on `"$1"` instead — but note in-place `sed` differs across platforms: `sed -i ''` on macOS vs `sed -i` on GNU/Linux — which is why replacing the whole body is more reliable.)

Two things to get right:

- **noki checks `$VISUAL` before `$EDITOR`.** Many shells already set `$VISUAL` (to `vim`, `hx`, …), and that would win and open an interactive editor that hangs. Set **`VISUAL`** to your script; setting only `EDITOR` won't help when `VISUAL` is already set.
- **The script sees the body only** — the temp file has no frontmatter. Don't try to rewrite `title:` or `labels:` this way; those are managed by noki, and `created`/`updated` are handled for you.

## A push warning is not a failure

After committing locally, noki fetches, rebases onto the remote, and pushes. If any of that fails — offline, no `origin`, a rejected ref, a rare same-path conflict — noki prints a **warning** and still exits successfully, because the note is already committed locally and will sync on the next successful push. So:

- A printed warning means "saved locally, not yet pushed" — report it calmly, don't treat it as a lost note.
- Don't re-run the capture on a warning; that would create a duplicate note.

## Not for agents

- Bare `noki` with no arguments opens `$EDITOR` interactively — never use it to capture from an agent. Pipe stdin with `--no-edit` instead. (`noki edit` is also editor-based by default, but can be driven non-interactively via `$VISUAL` — see "Editing an existing note" above.)
- Don't auto-capture or auto-edit. Notes are committed and pushed to the user's remote, so only save or change a note when the user explicitly asks.
