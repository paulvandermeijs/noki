# Noki Agent Skills — Design

**Status:** Approved (brainstorming) — ready for implementation plan

## Goal

Ship two agent-facing skills *with* the noki repository so that any end user's
agent (Claude Code or compatible) can install them and reliably drive the noki
CLI. This makes noki's "AI friendly" promise concrete: an agent should know
when to reach for noki, how to invoke it non-interactively, how to parse its
output, and how to recover from its quirks.

The skills are authored with `skill-creator:skill-creator` and follow the
Claude Code skill format (a `SKILL.md` with YAML frontmatter plus body).

## Decisions (from brainstorming)

1. **Consumer & location:** Skills ship *with* the noki repo, under
   `skills/<name>/SKILL.md`, so they are distributed alongside noki and any
   end-user agent can install them into their own projects.
2. **Granularity:** Two focused skills — `capturing-notes` (write) and
   `retrieving-notes` (read) — split by intent for clean triggering.
3. **Trigger posture:** Explicit requests only. The capture skill fires when
   the user explicitly asks to save/jot/record a note. No proactive
   auto-capture, because notes are committed and pushed to a git remote and
   silent commits would be unwelcome.
4. **Binary reference:** Skills document the **installed `noki` binary on
   `PATH`**, not `cargo run` (which is a noki-development concern).

## Background — the noki CLI surface (ground truth)

Verified against `src/cli.rs`, `src/note.rs`, `src/output.rs`, `README.md`.

### Capture (default command, no subcommand)

- `noki` — opens `$EDITOR` to capture a note (interactive; **not** agent-usable).
- `-n, --no-edit` — skip the editor and store piped stdin directly.
- `-t, --title <TITLE>` — set the note title (overrides title derived from content).
- `--label <LABEL>` — add a label; repeat for several. (No short form; `-l` was
  deliberately dropped — see commit `742aab5`.)
- `-d, --daily` — open/create today's daily note. Path from
  `note.daily_filename` (default `%Y/%m/%d`). If it exists, opens pre-filled;
  piped input is **appended**. Tagged with `note.daily_label` (default `daily`).
- `--repository <URL>` — global flag; the notes repository to use.

### Browse

- `noki ls` (alias for `list`) — list notes (human table).
  - `--json` — output as JSON.
- `noki show <path>` — show a single note by its repo-relative path.
  - `--json` — output as JSON.
  - `--raw` — output the unmodified file contents.
- `noki edit <path>` — edit an existing note in `$EDITOR` (interactive; **not**
  agent-usable). On save, `updated` is refreshed, `created` preserved, and
  title/labels/other frontmatter kept as-is.

### Output shapes (from `src/note.rs`, `src/output.rs`)

- `show --json` → a `Note`:
  ```json
  {
    "meta": {
      "title": "...",
      "path": "...",
      "labels": ["..."],
      "created": "2026-06-02T10:00:00+01:00",
      "updated": "2026-06-02T10:00:00+01:00"
    },
    "content": "..."
  }
  ```
  `meta` also carries any flattened static/`extra` meta (e.g. `author`).
  Timestamps are RFC 3339 strings.
- `ls --json` → an array of note **summaries**. Same `meta` fields **but no
  `content`** (list output omits the body). Exact field set to be confirmed
  against `render_list_json` during authoring.
- `show --raw` → the byte-exact file contents (frontmatter + body).

### Behavioural contracts an agent must know

- **Push is non-fatal and never loses a note.** `write_file` always commits
  locally, then fetch → rebase-onto-`origin/<branch>` → push. Any failure
  (offline, rejected ref, no `origin`, conflict) prints a **warning** and still
  returns success. So a warning on capture is **not** a failure — the note is
  saved locally.
- **A repository must be configured** (via `.noki.toml`, a parent `.noki.toml`,
  a global config, or `--repository`). Without it, capture/browse cannot run.
- **Notes use unique timestamped paths**, so concurrent captures essentially
  never path-collide.

## Skill 1 — `capturing-notes`

**One-line purpose:** Capture a Markdown note into the user's noki
(git-backed) note repository from an agent, non-interactively.

**Trigger surface (description):** explicit save/record/jot intents —
"save this as a note", "add this to my noki", "jot this down in noki", "make a
daily note", "log this to my journal". The description states that noki commits
and pushes to a git remote, so the agent treats it as durable storage, not a
scratch buffer.

**The one thing agents get wrong (headline teaching):** an agent has no
interactive `$EDITOR`. It MUST pipe content on stdin and pass `--no-edit`.
Never run bare `noki` (it would block on an editor).

**Body covers:**
- Canonical capture: `printf '%s' "$body" | noki --no-edit` (or `echo`), with a
  note on preserving Markdown/newlines.
- `--title "..."` and repeated `--label a --label b` to set metadata explicitly
  rather than relying on the first-heading title heuristic.
- Daily notes: `... | noki --daily --no-edit` appends to today's note; plain
  `noki --daily --no-edit` with stdin creates it if absent.
- Prerequisite: a `repository` must be configured; how to recognise the
  "no repository configured" error and what to tell the user.
- Push contract: a printed warning ≠ failure; the note is committed locally and
  will sync later. Do not retry-spam on a warning.

**Explicitly out of scope / honest limitations:**
- No proactive auto-capture (trigger posture is explicit-only).
- Bare `noki` and `noki edit` are interactive and not for agents.

## Skill 2 — `retrieving-notes`

**One-line purpose:** Find, read, and reason over notes stored in noki by
driving its structured (`--json`) output.

**Trigger surface (description):** "find/search/list/show my notes", "what did
I write about X", "read my daily note", "look up the note about Y".

**Headline teaching:** drive noki **structured** — always prefer `--json` and
parse it; never scrape the human table (it is color/width/terminal-dependent
and truncates labels).

**Body covers:**
- `noki ls --json` → array of summaries (no `content`); use for enumeration and
  metadata filtering.
- `noki show <path> --json` → full note with `content`; use once you know the
  path.
- `noki show <path> --raw` → byte-exact file when you need the original text
  (e.g. before manual re-write).
- **Search idiom (highest-value teaching):** noki has **no search command**.
  The pattern is: `ls --json` → filter client-side by `title` / `labels` /
  `created` (date-prefixed paths help) → `show` the winners with `--json`.
  Provide a concrete `jq` example for filtering by label and by title
  substring.

**Explicitly out of scope / honest limitations:**
- `noki edit` is interactive → not agent-usable directly.
- noki offers **no non-interactive write-back** for an existing note. To change
  a note an agent can read `--raw`, but the skill states plainly there is no
  supported programmatic edit path rather than inventing a flag.

## Success criteria

- Both skills exist at `skills/capturing-notes/SKILL.md` and
  `skills/retrieving-notes/SKILL.md`, valid per the Claude Code skill format.
- `capturing-notes` unambiguously instructs `--no-edit` + stdin and never
  suggests bare `noki` for agents.
- `retrieving-notes` teaches the `ls --json` → filter → `show` search idiom with
  a working `jq` example, and is honest about the missing edit path.
- Every CLI flag, output field, and behavioural claim in each skill matches the
  ground-truth surface above (no invented flags or fields).
- Descriptions trigger on the intended intents (validated with skill-creator's
  eval/triggering check where available) without over-firing.

## Out of scope

- A `configuring-noki` skill (`.noki.toml` setup) — deferred; the README
  already documents config for humans, and the capture skill only needs to
  detect the "not configured" state.
- Any change to the noki binary or its behaviour. This is documentation
  (skills) only.
- Proactive/automatic note capture.
