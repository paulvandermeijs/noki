# Noki Agent Skills Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship two agent-facing skills with the noki repo — `capturing-notes` and `retrieving-notes` — so any end-user's agent can install them and reliably drive the noki CLI.

**Architecture:** Two Claude Code skills, each a `skills/<name>/SKILL.md` (YAML frontmatter + Markdown body), authored and iterated with `skill-creator:skill-creator`. The skills are pure documentation — they change no noki code. `capturing-notes` teaches non-interactive capture (`--no-edit` + stdin); `retrieving-notes` teaches structured reading (`--json`) and the `ls → filter → show` search idiom.

**Tech Stack:** Markdown + YAML frontmatter (Claude Code skill format); `skill-creator` for drafting, evals, and description optimization; `jq` for the documented search examples; the installed `noki` binary as the subject.

## Global Constraints

- Skills live at `skills/<name>/SKILL.md`, relative to the repo root, and ship with noki.
- Document the **installed `noki` binary on `PATH`**, never `cargo run` (that's a noki-development concern, not an end-user concern).
- Every CLI flag, output field, and behavioural claim MUST match the ground-truth surface in the "Reference" section below. No invented flags or fields.
- Trigger posture is **explicit-only**: the capture skill fires only on an explicit user request to save/record a note; never suggest proactive/automatic capture.
- Follow skill-creator's writing guidance: imperative voice, explain the *why*, avoid heavy-handed all-caps MUSTs, and make descriptions slightly "pushy" so they trigger reliably without over-firing.
- Eval **workspaces** (`skills/<name>-workspace/`) are throwaway and MUST NOT be committed; the committed deliverable is `SKILL.md` (plus `evals/evals.json` as documentation of the test cases).

---

## Reference: ground-truth noki surface

Verified against `src/cli.rs`, `src/note.rs`, `src/output.rs`, `README.md` on 2026-07-05. Every skill claim must trace back here.

**Capture (default command, no subcommand):**
- `noki` — opens `$EDITOR` (interactive; **not** agent-usable).
- `-n`, `--no-edit` — skip editor, store piped stdin directly.
- `-t`, `--title <TITLE>` — set title (overrides title derived from content).
- `--label <LABEL>` — add a label; repeat for several. **No short form** (`-l` was deliberately dropped, commit `742aab5`).
- `-d`, `--daily` — open/create today's daily note (path from `note.daily_filename`, default `%Y/%m/%d`). If it exists, piped input is **appended**. Tagged with `note.daily_label` (default `daily`).
- `--repository <URL>` — global flag; the notes repository to use.

**Browse:**
- `noki ls` (alias for `list`) — human table. `--json` for JSON.
- `noki show <path>` — single note by repo-relative path. `--json` for JSON, `--raw` for byte-exact file contents.
- `noki edit <path>` — edit in `$EDITOR` (interactive; **not** agent-usable). On save `updated` refreshes, `created` is preserved, title/labels/frontmatter kept.

**JSON shapes** (exact):
- `noki show <path> --json` →
  ```json
  { "meta": { "title": "…", "path": "…", "labels": ["…"], "created": "2026-06-02T10:00:00+01:00", "updated": "2026-06-02T10:00:00+01:00" }, "content": "…" }
  ```
  `meta` also carries any flattened static/extra frontmatter (e.g. `author`). Timestamps are RFC 3339 strings.
- `noki ls --json` → an **array of `{ "meta": {…} }` objects** (each entry is wrapped in `meta`), with **no `content`** field. Same `meta` fields as above.
- `noki show <path> --raw` → the byte-exact file (frontmatter + body).

**Behavioural contracts:**
- **Push is non-fatal.** `write_file` always commits locally, then fetch → rebase-onto-`origin/<branch>` → push. Any failure (offline, rejected ref, no `origin`, conflict) prints a **warning** and still succeeds. A warning ≠ a lost note.
- **A repository must be configured** via `.noki.toml` (current dir or any parent), a global config, or `--repository`. Without it, commands cannot run.
- **Notes use unique timestamped paths**, so concurrent captures essentially never collide.

---

## File Structure

- `skills/capturing-notes/SKILL.md` — the capture skill (frontmatter + body).
- `skills/capturing-notes/evals/evals.json` — test prompts for the capture skill.
- `skills/retrieving-notes/SKILL.md` — the retrieve skill (frontmatter + body).
- `skills/retrieving-notes/evals/evals.json` — test prompts for the retrieve skill.
- `.gitignore` — modified to exclude `skills/*-workspace/` (throwaway eval output).

Each skill is self-contained in its own directory with one clear responsibility (capture vs. retrieve), so it can be installed, triggered, and evaluated independently.

---

## Task 1: Repo scaffolding for skills

**Files:**
- Create: `skills/` directory (via the files below).
- Modify: `.gitignore` (append one line).

**Interfaces:**
- Produces: the `skills/` tree that Tasks 2–5 write into; a `.gitignore` rule that keeps eval workspaces out of git.

- [ ] **Step 1: Inspect the current `.gitignore`**

Run: `cat .gitignore`
Expected: shows the existing ignore rules (currently just `/target` per the repo). Note the exact contents so the next edit only appends.

- [ ] **Step 2: Append the workspace ignore rule**

Add this line to `.gitignore` (keep existing lines unchanged):

```gitignore
skills/*-workspace/
```

- [ ] **Step 3: Verify the rule is active**

Run: `mkdir -p skills/capturing-notes-workspace && git status --porcelain skills/ ; rmdir skills/capturing-notes-workspace`
Expected: the `git status` line shows **no** `capturing-notes-workspace/` entry (it is ignored). The `rmdir` cleans up the probe dir.

- [ ] **Step 4: Commit**

```bash
git add .gitignore
git commit -m "chore: ignore skill eval workspaces"
```

---

## Task 2: Draft the `capturing-notes` skill

**Files:**
- Create: `skills/capturing-notes/SKILL.md`

**Interfaces:**
- Consumes: the ground-truth surface (Reference section).
- Produces: `skills/capturing-notes/SKILL.md` with a `capturing-notes` name and a triggering description; consumed by Task 3's evals.

- [ ] **Step 1: Write `skills/capturing-notes/SKILL.md`**

Write this exact content:

````markdown
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
````

- [ ] **Step 2: Sanity-check the frontmatter and claims**

Run: `head -5 skills/capturing-notes/SKILL.md`
Expected: a valid YAML frontmatter block — a `name: capturing-notes` line and a `description:` line — delimited by `---`.

Then re-read the body against the Reference section: confirm every flag shown (`--no-edit`, `--title`, `--label`, `--daily`, `--repository`) exists in `src/cli.rs` and that no invented flag appears.

- [ ] **Step 3: Commit**

```bash
git add skills/capturing-notes/SKILL.md
git commit -m "feat: add capturing-notes agent skill"
```

---

## Task 3: Evaluate and iterate `capturing-notes` with skill-creator

**Files:**
- Create: `skills/capturing-notes/evals/evals.json`
- Modify: `skills/capturing-notes/SKILL.md` (only if evals reveal problems)

**Interfaces:**
- Consumes: `skills/capturing-notes/SKILL.md` from Task 2.
- Produces: a verified capture skill plus committed test prompts.

- [ ] **Step 1: Write the eval prompts**

Create `skills/capturing-notes/evals/evals.json` (realistic, substantive prompts — skill-creator notes that trivial one-liners don't reliably trigger skills):

```json
{
  "skill_name": "capturing-notes",
  "evals": [
    {
      "id": 1,
      "prompt": "I'm on a call and just decided we're moving the launch to Sept 3rd because QA needs another week. Save this as a note in my noki with the title 'Launch date slip' and label it work and decisions.",
      "expected_output": "Runs noki non-interactively with piped stdin and --no-edit, passing --title and two --label flags. Never runs bare noki or noki edit.",
      "files": []
    },
    {
      "id": 2,
      "prompt": "Log to my daily note that I finished the auth refactor and merged the PR.",
      "expected_output": "Pipes the text to `noki --daily --no-edit` so it appends to today's daily note. No editor invocation.",
      "files": []
    },
    {
      "id": 3,
      "prompt": "Here's a markdown snippet with a heading and a bulleted list — jot it down in noki for me.",
      "expected_output": "Captures multi-line Markdown via stdin (printf/heredoc) with --no-edit, preserving newlines. Does not open an editor.",
      "files": []
    }
  ]
}
```

- [ ] **Step 2: Run skill-creator's eval loop on this skill**

Invoke `skill-creator:skill-creator` and tell it: the skill already has a draft at `skills/capturing-notes/` and test prompts at `skills/capturing-notes/evals/evals.json`; go straight to the eval/iterate part of the loop (spawn with-skill and baseline runs, grade, launch the review viewer).

Watch specifically for the failure this skill exists to prevent: any run that invokes **bare `noki`** or **`noki edit`** (which would hang on `$EDITOR`) is a fail. Every capture must use `--no-edit` with stdin.

- [ ] **Step 3: Review results and iterate**

Review the viewer output with the user. If any run opened the editor, used a non-existent flag, treated a push warning as a failure, or captured without an explicit request, revise `SKILL.md` (explain the *why* more clearly rather than adding rigid MUSTs) and rerun into a new iteration. Repeat until captures are reliably non-interactive and correct.

- [ ] **Step 4: Commit**

```bash
git add skills/capturing-notes/evals/evals.json skills/capturing-notes/SKILL.md
git commit -m "test: add capturing-notes evals and iterate skill"
```

---

## Task 4: Draft the `retrieving-notes` skill

**Files:**
- Create: `skills/retrieving-notes/SKILL.md`

**Interfaces:**
- Consumes: the ground-truth surface (Reference section) — especially the exact JSON shapes.
- Produces: `skills/retrieving-notes/SKILL.md`; consumed by Task 5's evals.

- [ ] **Step 1: Write `skills/retrieving-notes/SKILL.md`**

Write this exact content:

````markdown
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
````

- [ ] **Step 2: Sanity-check the frontmatter and JSON claims**

Run: `head -5 skills/retrieving-notes/SKILL.md`
Expected: valid frontmatter with `name: retrieving-notes` and a `description:` line.

Confirm against the Reference section: `ls --json` entries are `{ "meta": {…} }` with **no** `content`; `show --json` includes `content`; the `jq` filters reference `.meta.labels` and `.meta.title` (matching the wrapped shape).

- [ ] **Step 3: (Optional but recommended) verify the `jq` examples against a real repo**

If a noki repo with a note or two is available (build first — `cargo build` — since tests/clippy don't rebuild the binary), run:

```sh
noki ls --json | jq '[.[] | select(.meta.title | test("."; "i"))]'
```

Expected: valid JSON array output (no `jq` error), confirming the `.meta.*` paths match the real shape. If no repo is available, skip — the shape is already confirmed against `src/output.rs`.

- [ ] **Step 4: Commit**

```bash
git add skills/retrieving-notes/SKILL.md
git commit -m "feat: add retrieving-notes agent skill"
```

---

## Task 5: Evaluate and iterate `retrieving-notes` with skill-creator

**Files:**
- Create: `skills/retrieving-notes/evals/evals.json`
- Modify: `skills/retrieving-notes/SKILL.md` (only if evals reveal problems)

**Interfaces:**
- Consumes: `skills/retrieving-notes/SKILL.md` from Task 4.
- Produces: a verified retrieve skill plus committed test prompts.

- [ ] **Step 1: Write the eval prompts**

Create `skills/retrieving-notes/evals/evals.json`:

```json
{
  "skill_name": "retrieving-notes",
  "evals": [
    {
      "id": 1,
      "prompt": "What did I write about the sprint planning meeting? Look through my noki notes and summarize it.",
      "expected_output": "Runs `noki ls --json`, filters by title/label (e.g. with jq on .meta.title or .meta.labels), then `noki show <path> --json` on the match and summarizes content. Parses JSON, not the table.",
      "files": []
    },
    {
      "id": 2,
      "prompt": "List all my notes tagged 'work' from my noki.",
      "expected_output": "Runs `noki ls --json` and filters on .meta.labels containing 'work' (e.g. `jq '[.[] | select(.meta.labels | index(\"work\"))]'`). Does not scrape the human table.",
      "files": []
    },
    {
      "id": 3,
      "prompt": "I want to update the note at 2026/06/02/10:00:00-sprint-planning.md — can you change the second bullet for me?",
      "expected_output": "Explains that noki has no non-interactive edit path (noki edit is interactive), reads the current text with `noki show <path> --raw`, and does not invent an edit flag.",
      "files": []
    }
  ]
}
```

- [ ] **Step 2: Run skill-creator's eval loop on this skill**

Invoke `skill-creator:skill-creator`: draft is at `skills/retrieving-notes/`, prompts at `skills/retrieving-notes/evals/evals.json`; run the eval/iterate loop (with-skill + baseline runs, grade, viewer).

Watch for: scraping the human table instead of `--json`; using `.title`/`.labels` at the top level instead of `.meta.title`/`.meta.labels`; and, on eval 3, inventing an edit flag instead of admitting the limitation.

- [ ] **Step 3: Review results and iterate**

Review with the user; revise `SKILL.md` where runs went wrong (prefer clearer explanation over rigid rules) and rerun into a new iteration until retrieval is reliably structured and honest about the missing edit path.

- [ ] **Step 4: Commit**

```bash
git add skills/retrieving-notes/evals/evals.json skills/retrieving-notes/SKILL.md
git commit -m "test: add retrieving-notes evals and iterate skill"
```

---

## Task 6: Optimize descriptions and final verification

**Files:**
- Modify: `skills/capturing-notes/SKILL.md`, `skills/retrieving-notes/SKILL.md` (frontmatter `description` only, if optimization improves it)

**Interfaces:**
- Consumes: both finished skills.
- Produces: descriptions tuned for triggering accuracy; a verified final state.

- [ ] **Step 1: Run description optimization for each skill**

Use skill-creator's Description Optimization flow. For each skill, build a ~20-query trigger eval set (8–10 should-trigger, 8–10 tricky should-not-trigger near-misses), review it with the user, then run the optimizer:

```bash
python -m scripts.run_loop \
  --eval-set <path-to-trigger-eval.json> \
  --skill-path skills/capturing-notes \
  --model <model-id-powering-this-session> \
  --max-iterations 5 \
  --verbose
```

Repeat with `--skill-path skills/retrieving-notes`. Key near-misses to include:
- capture vs. retrieve ("save my notes to a file" is neither; "read my noki notes" should hit retrieve, not capture).
- noki notes vs. generic note-taking apps or git commits (should-not-trigger).

- [ ] **Step 2: Apply the best descriptions**

If the optimizer's `best_description` beats the current one, update the skill's frontmatter `description`. Show the user before/after and the scores.

- [ ] **Step 3: Final structural check of both skills**

Run: `for f in skills/*/SKILL.md; do echo "== $f =="; head -3 "$f"; done`
Expected: each file starts with `---` then a `name:` matching its directory and a `description:` line.

Run: `git status --porcelain skills/`
Expected: no `*-workspace/` directories appear (they are gitignored); only `SKILL.md` and `evals/evals.json` files are tracked.

- [ ] **Step 4: Commit any description changes**

```bash
git add skills/capturing-notes/SKILL.md skills/retrieving-notes/SKILL.md
git commit -m "refactor: optimize noki skill descriptions for triggering"
```

- [ ] **Step 5: Document the skills in the README (optional, recommended)**

Add a short "Agent skills" section to `README.md` pointing at `skills/capturing-notes` and `skills/retrieving-notes`, describing that they teach an agent to drive noki and how to install them. Commit:

```bash
git add README.md
git commit -m "docs: mention bundled agent skills in README"
```

---

## Self-Review

**Spec coverage:**
- Two skills shipped in-repo (`skills/`) — Tasks 2, 4. ✓
- Capture teaches `--no-edit` + stdin, never bare `noki` — Task 2 body + Task 3 eval focus. ✓
- Retrieve teaches `--json` and the `ls → filter → show` idiom with `jq` — Task 4 body. ✓
- Honest about no non-interactive edit path — Task 4 body + Task 5 eval 3. ✓
- Explicit-only trigger posture — Global Constraints + capture description/body. ✓
- Claims match ground truth — Reference section + verification steps in Tasks 2/4. ✓
- Descriptions tuned for triggering — Task 6. ✓
- Eval workspaces kept out of git — Task 1. ✓

**Placeholder scan:** SKILL.md bodies and eval JSON are given in full; commands have expected output. No TBD/TODO left in deliverables. ✓

**Type/name consistency:** Skill dir names, frontmatter `name` fields, and `evals.json` `skill_name` all use `capturing-notes` / `retrieving-notes` consistently. JSON field paths (`.meta.title`, `.meta.labels`, `.meta.path`, `content`) match the Reference shapes everywhere they appear. ✓
