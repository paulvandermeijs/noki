# Slug Length Cap — Design

## Problem

`noki --no-edit` derives the note filename from the first heading or paragraph of the content (`note::title_from_content`). A dictated recording (`yap dictate | noki -n`) is one long paragraph, so the entire transcript becomes the title, and `Sanitize::Slug` in `src/template.rs` slugifies it unbounded into the `{title}` path segment. On macOS a filename component is capped at 255 bytes, so long recordings fail with `File name too long (os error 63)` — after the note content was already read, losing the capture.

## Decision

Cap every slugified template field at **80 characters**, truncating **floored to whole words**. Fixed built-in constant, not configurable (YAGNI — add a config key only if someone asks).

## Mechanism

In `src/template.rs`, the `Sanitize::Slug` arm of `resolve_token` currently emits `slug::slugify(&value)` unbounded. A private helper (bottom of file, per the public-top/private-bottom convention) caps it:

- `const MAX_SLUG_LENGTH: usize = 80;`
- `fn truncate_slug(slug: String, max: usize) -> String`

`slug::slugify` output is guaranteed lowercase ASCII (`a-z0-9-`), so characters == bytes and word boundaries are exactly the `-` separators.

**Truncation rule:**
- Length ≤ max → unchanged.
- Otherwise cut at the last `-` at-or-before index `max`, dropping the `-` and everything after it (word-floored).
- No `-` within the first `max` characters (one giant word) → hard cut at `max`. Never empty.
- No trailing `-` can survive, by construction.

## Scope

- Applies to **every** `Sanitize::Slug` text field: `{title}`, `{labels}`, static meta such as `{author}`. The invariant is "a slug segment never exceeds the OS filename limit," not "titles are special."
- `Sanitize::Raw` (e.g. `note.daily_title` rendering) is untouched.
- The frontmatter `title:` is untouched — a dictated note's stored title remains the full first sentence; only the path is capped.
- The `unknown-<field>` placeholder path is unaffected (placeholders are short; the cap still applies harmlessly).

Headroom: 80 (slug) + 9 (`17:25:03-` prefix in the default template) + 3 (`.md`) = 92 bytes, far under 255.

## Error Handling

None new. Truncation is total and infallible. Template syntax errors keep their existing behavior (`Err`, never panic).

## Testing

Unit tests in `template.rs`:
- long multi-word slug floors to a whole word, length ≤ 80, no trailing `-`
- slug of exactly 80 → unchanged
- word boundary exactly at the cut → cut there cleanly
- single word longer than 80 → hard cut at 80
- short slugs pass through untouched

Integration-level test in `note.rs`: `note_path` with a paragraph-length title yields a final path component under 255 bytes.

Manual verification: pipe a very long single-paragraph note into `noki -n --repository <throwaway local bare repo>` and confirm the note stores (previously: os error 63). Clean up the throwaway repo and its per-URL clone afterwards.

## Out of Scope

- Capping the stored frontmatter `title:` (explicitly decided against — display-layer concern, and the body carries the same text anyway).
- Configurability of the cap.
- Any change to `Sanitize::Raw` rendering or `title_from_content`.
