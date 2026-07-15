# Slug Length Cap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cap every slugified template field at 80 characters, truncated floored to whole words, so paragraph-length dictated titles no longer fail with `File name too long (os error 63)`.

**Architecture:** Per the spec (`docs/superpowers/specs/2026-07-14-slug-length-cap-design.md`), the cap lives at the single slug-rendering seam: the `Sanitize::Slug` arm of `resolve_token` in `src/template.rs`, which currently emits `slug::slugify(&value)` unbounded. A private `truncate_slug` helper caps the result at `MAX_SLUG_LENGTH = 80`. `slug::slugify` output is guaranteed lowercase ASCII (`a-z0-9-`) with no leading/trailing/consecutive dashes, so characters == bytes, `-` marks every word boundary, and byte slicing is always char-safe. `Sanitize::Raw`, the `unknown-<field>` placeholder path, and the stored frontmatter title are untouched.

**Tech Stack:** Rust 2024, existing `slug` crate. No new dependencies.

## Global Constraints

- Lint gate before every commit: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
- No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code; tests may `unwrap()` freely
- Public API at the top of each file, private helpers at the bottom
- Cap value is exactly **80**, a fixed built-in constant — no config surface (spec decision)
- Cap applies to **every** `Sanitize::Slug` text field, not just `{title}` (spec decision)
- `cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki` — run `cargo build` before manually exercising the binary
- **NEVER stage `docs/superpowers/specs/2026-07-13-global-config-flag-design.md`** — it belongs to a separate in-flight feature. Always `git add` explicit paths; never `git add -A`, `git add .`, or `git add docs/`.

---

### Task 1: `truncate_slug` in the slug-rendering path

**Files:**
- Modify: `src/template.rs` (Slug arm of `resolve_token` at line 87; new constant + helper at the bottom, before `mod tests`)

**Interfaces:**
- Consumes: nothing new — `slug::slugify` already in use.
- Produces: `const MAX_SLUG_LENGTH: usize = 80;` and `fn truncate_slug(slug: String, max: usize) -> String`, both private to `template.rs`. Task 2 relies only on the observable behavior (no slug segment longer than 80) through the existing `pub(crate) fn render`.

- [ ] **Step 1: Write the failing tests**

In `src/template.rs`, inside the existing `mod tests`, after `empty_value_defaults_to_unknown_placeholder`:

```rust
    #[test]
    fn overlong_slug_cuts_exactly_on_a_word_boundary() {
        // "sentence" slugs to 8 chars; n words join to 9n-1 chars, so 9 words
        // is exactly 80 — the boundary sits precisely at the cap.
        let value = "sentence ".repeat(20);
        let out = render("{title}", |_| Some(Field::Text(value.clone())), Sanitize::Slug).unwrap();
        let nine_words = ["sentence"; 9].join("-");
        assert_eq!(out, nine_words);
        assert_eq!(out.len(), 80);
    }

    #[test]
    fn overlong_slug_floors_to_the_last_whole_word() {
        // "abcdefghij" slugs to 10 chars; 7 words = 76 chars, 8 words = 87.
        // The cap must floor to 7 whole words, never cut mid-word.
        let value = "abcdefghij ".repeat(20);
        let out = render("{title}", |_| Some(Field::Text(value.clone())), Sanitize::Slug).unwrap();
        let seven_words = ["abcdefghij"; 7].join("-");
        assert_eq!(out, seven_words);
        assert!(!out.ends_with('-'));
    }

    #[test]
    fn single_giant_word_is_hard_cut_never_empty() {
        let value = "a".repeat(200);
        let out = render("{title}", |_| Some(Field::Text(value)), Sanitize::Slug).unwrap();
        assert_eq!(out, "a".repeat(80));
    }

    #[test]
    fn slug_of_exactly_max_length_is_unchanged() {
        // 9 "sentence" words slug to exactly 80 chars — at the cap, not over it.
        let value = "sentence ".repeat(9);
        let out = render("{title}", |_| Some(Field::Text(value)), Sanitize::Slug).unwrap();
        assert_eq!(out.len(), 80);
        assert_eq!(out, ["sentence"; 9].join("-"));
    }
```

(Short slugs passing through untouched is already pinned by the existing `text_field_is_slugified` test.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib template`
Expected: the two `overlong_*` tests and `single_giant_word_is_hard_cut_never_empty` FAIL (assertion mismatch — unbounded slugs of 179, 219, and 200 chars respectively); `slug_of_exactly_max_length_is_unchanged` passes already (it is a guard against over-eager truncation). If all four pass, something is wrong — stop and re-check.

- [ ] **Step 3: Write the implementation**

Two edits to `src/template.rs`.

First, the Slug arm inside `resolve_token` (line 87) becomes:

```rust
                Sanitize::Slug => truncate_slug(slug::slugify(&value), MAX_SLUG_LENGTH),
```

Second, the constant and private helper at the bottom of the file, after `format_date` and before `mod tests`:

```rust
/// Path-segment slugs are capped so a paragraph-length title (e.g. a dictated
/// recording) cannot exceed the OS filename-component limit (255 bytes on
/// macOS/Linux). 80 keeps names readable with ample headroom for the
/// timestamp prefix and `.md` suffix.
const MAX_SLUG_LENGTH: usize = 80;

/// Cap `slug` at `max` characters, flooring to a whole word. `slug::slugify`
/// output is lowercase ASCII with single `-` separators and no leading or
/// trailing dash, so byte indexing is char-safe and every `-` is a word
/// boundary. A single word longer than `max` is hard-cut so the result is
/// never empty.
fn truncate_slug(slug: String, max: usize) -> String {
    if slug.len() <= max {
        return slug;
    }
    match slug[..=max].rfind('-') {
        Some(cut) => slug[..cut].to_string(),
        None => slug[..max].to_string(),
    }
}
```

Note on `slug[..=max]`: the early return guarantees `slug.len() >= max + 1`, so the inclusive slice is in bounds, and including index `max` itself means a dash sitting exactly at position 80 yields an 80-char result (pinned by `overlong_slug_cuts_exactly_on_a_word_boundary`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib template`
Expected: PASS — all template tests green, including the four new ones.

- [ ] **Step 5: Run the full suite and the lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass, no formatting diff, no clippy warnings.

- [ ] **Step 6: Commit (explicit paths only)**

```bash
git add src/template.rs
git commit -m "fix(template): cap slugified fields at 80 chars, word-floored"
```

Do NOT add anything under `docs/superpowers/specs/`.

---

### Task 2: Path-level guarantee test + manual repro of the original failure

**Files:**
- Modify: `src/note.rs` (one test in the existing `mod tests`)
- No production-code changes — this task pins the end-to-end guarantee and reproduces the user's original failure against the real binary.

**Interfaces:**
- Consumes: `note::note_path(template, title, labels, meta, now) -> Result<String>` and `note::DEFAULT_FILENAME` (both existing, `src/note.rs:60,103`); the capped slug behavior from Task 1.
- Produces: nothing for later tasks — this is the final task.

- [ ] **Step 1: Write the test**

In `src/note.rs`, inside the existing `mod tests`, after `note_path_interpolates_meta_and_labels` (reuse the file's existing timestamp helper — the neighboring `note_path_*` tests construct a `when` value; follow the same pattern):

```rust
    #[test]
    fn note_path_component_stays_under_os_limit_for_paragraph_titles() {
        // A dictated note's title is its entire first paragraph; the final
        // path component must stay under the 255-byte OS limit regardless.
        let when = DateTime::parse_from_rfc3339("2026-07-14T17:25:03+02:00").unwrap();
        let title = "this is one very long dictated sentence that just keeps going ".repeat(10);
        let path = note_path(DEFAULT_FILENAME, &title, &[], &BTreeMap::new(), when).unwrap();
        let component = path.rsplit('/').next().unwrap();
        assert!(
            component.len() < 255,
            "final component is {} bytes: {component}",
            component.len()
        );
        assert!(component.ends_with(".md"));
    }
```

If the file's tests use a helper like `fn at(s: &str) -> DateTime<FixedOffset>` (as `src/template.rs`'s tests do), call that instead of inline `parse_from_rfc3339` — match the file's existing idiom. Adjust the `note_path` argument types to match the existing neighboring tests exactly (they pass `&[]` for labels and `&BTreeMap::new()` for meta).

- [ ] **Step 2: Run the test to verify it passes (and prove it can fail)**

Run: `cargo test --lib note::tests::note_path_component_stays_under_os_limit_for_paragraph_titles`
Expected: PASS.

Then prove the test discriminates: temporarily change `MAX_SLUG_LENGTH` in `src/template.rs` from `80` to `8000`, run the same test — Expected: FAIL (component ≥ 255 bytes) — then restore `80` and confirm `git diff src/template.rs` is empty before proceeding.

- [ ] **Step 3: Manual repro of the original failure, now fixed**

```bash
cargo build
rm -rf /tmp/noki-slugcap-e2e && mkdir -p /tmp/noki-slugcap-e2e
git init --bare --initial-branch=master /tmp/noki-slugcap-e2e/notes.git
printf 'word %.0s' {1..200} | ./target/debug/noki --no-edit --repository /tmp/noki-slugcap-e2e/notes.git
./target/debug/noki ls --repository /tmp/noki-slugcap-e2e/notes.git
```

Expected: the capture succeeds (no `File name too long (os error 63)`), and `ls` shows one note whose path's final component is ≤ 92 bytes (timestamp prefix + 80-char slug + `.md`). The `printf` builds a 1000-char single-paragraph body (zsh brace expansion; the format consumes each number without printing it).

Cleanup:

```bash
rm -rf /tmp/noki-slugcap-e2e
find ~/Library/Application\ Support -maxdepth 3 -type d -name '*noki-slugcap-e2e*' 2>/dev/null
```

Delete only what the `find` returns (the per-URL clone noki created for the throwaway repo).

- [ ] **Step 4: Run the full suite and the lint gate**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: all green.

- [ ] **Step 5: Commit (explicit paths only)**

```bash
git add src/note.rs
git commit -m "test(note): pin note_path component under the OS filename limit"
```

Do NOT add anything under `docs/superpowers/specs/`.

---

## Self-Review

- **Spec coverage:** cap value + word-flooring + hard-cut + never-empty (Task 1 helper and its four tests); scope = every Slug field (cap sits inside `resolve_token`, upstream of all fields); Raw/placeholder/frontmatter untouched (no edits to those paths); headroom guarantee (Task 2 test asserts < 255 on the real `DEFAULT_FILENAME`); manual verification with throwaway repo + cleanup (Task 2 Step 3). No gaps.
- **Placeholder scan:** none — every step carries exact code, commands, and expected outcomes.
- **Type consistency:** `truncate_slug(String, usize) -> String` is defined and used only in `src/template.rs`; Task 2 refers to the constant by the same name, `MAX_SLUG_LENGTH`, in its discrimination check. Consistent.
