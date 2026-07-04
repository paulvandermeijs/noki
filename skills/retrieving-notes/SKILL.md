---
name: retrieving-notes
description: Find, read, and reason over the user's noki notes by driving noki's structured output. Use this whenever the user wants to look up, search, list, summarize, or read notes they saved in noki: phrasings like "what did I write about X", "find my notes on Y", "show me the note at <path>", "list my recent notes", "read today's daily note", or "what notes do I have tagged Z". noki has no search command, so reach for this skill to run the list → filter → show idiom rather than guessing.
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

## What you can't do from an agent

- `noki edit <path>` opens `$EDITOR` — it's interactive and not agent-usable.
- noki has **no non-interactive write-back** for an existing note. You can read a note's exact text with `noki show <path> --raw`, but there is no supported flag to save an edited version programmatically. If the user wants a note changed, say so plainly rather than inventing a command; a new note can still be captured (see the capturing-notes skill).
