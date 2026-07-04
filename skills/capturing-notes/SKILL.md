---
name: capturing-notes
description: Capture a Markdown note into the user's noki note repository — a git-backed store that commits and pushes every note. Use this whenever the user explicitly asks to save, jot, record, log, or note something down with noki: phrasings like "save this as a note", "add this to my noki", "jot this down in noki", "log this to my journal", "note that…", or "make/append today's daily note". Because notes are committed to a git remote, capture only on an explicit request — never automatically.
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

## A push warning is not a failure

After committing locally, noki fetches, rebases onto the remote, and pushes. If any of that fails — offline, no `origin`, a rejected ref, a rare same-path conflict — noki prints a **warning** and still exits successfully, because the note is already committed locally and will sync on the next successful push. So:

- A printed warning means "saved locally, not yet pushed" — report it calmly, don't treat it as a lost note.
- Don't re-run the capture on a warning; that would create a duplicate note.

## Not for agents

- Bare `noki` and `noki edit <path>` are interactive (they open `$EDITOR`). Never use them to capture from an agent.
- Don't auto-capture. Notes are committed and pushed to the user's remote, so only save when the user explicitly asks.
