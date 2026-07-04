---
name: retrieving-notes
description: Find, read, and reason over the user's noki notes by driving noki's structured output. Use this whenever the user wants to look up, search, list, summarize, or read notes they saved in noki — phrasings like "what did I write about X", "find my notes on Y", "show me the note at <path>", "list my recent notes", "read today's daily note", or "what notes do I have tagged Z". noki has no search command, so reach for this skill to run the list → filter → show idiom rather than guessing.
---

# Retrieving notes from noki

noki stores notes as Markdown files in a git-backed repo and can print them as JSON. Always drive it **structured** — the human table output is colored, width-dependent, and truncates labels, so parse `--json` instead of scraping the table.

## List notes

```sh
noki ls --json
```

Returns a JSON **array**, one entry per note, each wrapped in a `meta` object and **without the note body**:

```json
[
  { "meta": { "title": "Sprint planning", "path": "2026/06/02/10:00:00-sprint-planning.md", "labels": ["work", "meeting"], "created": "2026-06-02T10:00:00+01:00", "updated": "2026-06-02T10:00:00+01:00" } }
]
```

Use `ls --json` to enumerate notes and filter on metadata (title, labels, dates). `meta` may also carry extra frontmatter fields (e.g. `author`).

## Show one note

Once you have a `path`, read the whole note — this one includes `content`:

```sh
noki show "2026/06/02/10:00:00-sprint-planning.md" --json
```

```json
{ "meta": { "title": "Sprint planning", "path": "…", "labels": ["work", "meeting"], "created": "…", "updated": "…" }, "content": "# Sprint planning\n\n…" }
```

For the byte-exact file (frontmatter delimiters and all), use `--raw`:

```sh
noki show "2026/06/02/10:00:00-sprint-planning.md" --raw
```

## Searching: there is no search command

noki has no `search`. To answer "what did I write about X", list everything and filter client-side, then show the matches. The paths are date-prefixed (`YYYY/MM/DD/…`), which helps with time-based filtering.

Filter by label:

```sh
noki ls --json | jq '[.[] | select(.meta.labels | index("work"))]'
```

Filter by a case-insensitive title substring:

```sh
noki ls --json | jq '[.[] | select(.meta.title | test("sprint"; "i"))]'
```

Then read the winners:

```sh
noki show "<path from the filtered result>" --json
```

For matching on note *bodies* (not just metadata), you must `show --json` the candidates and inspect `content` — `ls --json` deliberately omits bodies to stay small, so body search is a two-step: narrow by metadata first, then fetch and scan content.

## Changing a note is a separate skill

This skill is read-only. If the user wants to **edit** an existing note — change a line, fix a bullet, append text — that's a write, and it's covered by the **capturing-notes** skill.

Two things worth knowing so you route correctly:

- `noki edit <path>` opens the note in an interactive editor by default, so it looks agent-unusable at first glance. It isn't: noki runs `$VISUAL`/`$EDITOR` on a temp file holding the note body, so you can drive it non-interactively by pointing `$VISUAL` at a script. The capturing-notes skill documents the exact technique and its gotchas.
- Reading is still your job here: to prepare an edit, fetch the current text with `noki show <path> --raw`, then hand off to the capturing-notes approach to write it back.
