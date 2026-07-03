# Colored Labels in the Note List Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show each note's labels as color-coded chips in the `ls` table (truncated to a configurable maximum with a `+N more` overflow marker) and color the labels in the single-note view too.

**Architecture:** A new `src/label.rs` module owns all label presentation: it deterministically derives a background+foreground color per label from its first-seen index (`create_label_color`), renders one ANSI-colored padded chip (`render_label`), remembers per-render color assignments so a repeated label keeps its color within one list (`LabelPalette`), and joins/truncates a note's labels into a table cell (`render_labels`). `src/output.rs` calls into it from both the list and single-note renderers. `src/config.rs` gains a `[list]` section with `max_visible_labels`.

**Tech Stack:** Rust 2024, `tabled` (needs its `ansi` feature so ANSI escapes don't corrupt column widths), `anyhow`, `serde`/`toml`. Colors are emitted as 24-bit truecolor ANSI escapes; no new crate is required.

## Global Constraints

- **No `unwrap()`/`expect()`/`panic!`/`unreachable!` in non-test code.** Tests may `unwrap()` freely. (`create_label_color`'s HSL math must not index out of bounds or divide by an untested zero.)
- **Errors use `anyhow::Result` with `.context(...)`** — no `thiserror`.
- **Public API at the top of each file, private helpers and private `const`s at the bottom** (author-wide rule).
- **TDD:** write the failing test, run it red, implement minimally, run it green, commit.
- **Lint gate before every commit:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings` must pass.
- **Reminder:** `cargo test`/`cargo clippy` do NOT rebuild `target/debug/noki`; run `cargo build` before exercising the binary by hand.
- **Default `max_visible_labels` is `3`** when not configured.
- **Chip format:** exactly one space of padding before and after the label text, inside the background color: `<fg><bg> label <reset>`. Chips are separated by a single space; the overflow marker `+N more` is rendered as plain (uncolored) text. (The spec's `feature, priority::high, backend, +3 more` illustration predates the chip/background requirement; comma separators read badly between colored backgrounds, so single-space separation is used and the overflow marker is plain text.)
- **Color mapping is per-render only** — never persisted between executions. `LabelPalette` lives on the stack for the duration of one render and is dropped afterward.

---

## File Structure

- **`src/label.rs`** (new): all label color + chip rendering. Public: `Rgb`, `LabelColor`, `create_label_color`, `render_label`, `LabelPalette`, `render_labels`. Private (bottom): `hsl_to_rgb`, `GOLDEN_ANGLE`, `BACKGROUND_LIGHTNESS`.
- **`src/lib.rs`** (modify): register `pub mod label;`.
- **`src/config.rs`** (modify): add `ListConfig`, `Config.list`, `Config::max_visible_labels()`, merge logic, `DEFAULT_MAX_VISIBLE_LABELS`.
- **`src/output.rs`** (modify): `render_list_human` takes `max_visible_labels` and renders a `labels` column via a shared `LabelPalette`; `render_note_human` colors its labels row.
- **`src/commands/list.rs`** (modify): `run` takes and forwards `max_visible_labels`.
- **`src/main.rs`** (modify): pass `config.max_visible_labels()` into `commands::list::run`.
- **`Cargo.toml`** (modify): enable `tabled`'s `ansi` feature.

---

### Task 1: `[list]` config section

**Files:**
- Modify: `src/config.rs`

**Interfaces:**
- Produces: `Config::max_visible_labels(&self) -> usize` (returns `3` when unset); `pub struct ListConfig { pub max_visible_labels: Option<usize> }`; `Config.list: ListConfig`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/config.rs`:

```rust
    #[test]
    fn parses_list_section() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".noki.toml"),
            "repository = \"r\"\n\n[list]\nmax_visible_labels = 5\n",
        )
        .unwrap();
        let config = load_from(None, dir.path(), None).unwrap();
        assert_eq!(config.max_visible_labels(), 5);
    }

    #[test]
    fn max_visible_labels_defaults_to_three() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_from(None, dir.path(), None).unwrap();
        assert_eq!(config.max_visible_labels(), 3);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib config`
Expected: FAIL — `no method named max_visible_labels` / `no field list`.

- [ ] **Step 3: Add the `ListConfig` type and wire it into `Config`**

In `src/config.rs`, add the `list` field to `Config` (just after `pub note: NoteConfig,`):

```rust
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub repository: Option<String>,
    pub note: NoteConfig,
    pub list: ListConfig,
}
```

Add the new struct immediately below `NoteConfig`:

```rust
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListConfig {
    pub max_visible_labels: Option<usize>,
}
```

- [ ] **Step 4: Add the public accessor and the default constant**

Add the accessor to the existing `impl Config` block that holds `repository()` (public API stays at the top):

```rust
impl Config {
    /// The resolved repository, or an error if none was configured.
    pub fn repository(&self) -> Result<&str> {
        self.repository
            .as_deref()
            .context("No repository configured. Set one with --repository or in .noki.toml.")
    }

    /// The maximum number of labels to show per note in the list.
    pub fn max_visible_labels(&self) -> usize {
        self.list
            .max_visible_labels
            .unwrap_or(DEFAULT_MAX_VISIBLE_LABELS)
    }
}
```

Add the constant next to `LOCAL_CONFIG_NAME` near the top of the file:

```rust
const DEFAULT_MAX_VISIBLE_LABELS: usize = 3;
```

- [ ] **Step 5: Extend `merge` so nearer configs override**

In the private `impl Config { fn merge(&mut self, other: Config) {...} }` block (bottom of file), add after the `note.meta` loop:

```rust
        if other.list.max_visible_labels.is_some() {
            self.list.max_visible_labels = other.list.max_visible_labels;
        }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib config`
Expected: PASS (all config tests, including the two new ones).

- [ ] **Step 7: Lint and commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/config.rs
git commit -m "feat: add [list] config section with max_visible_labels"
```

---

### Task 2: Deterministic label color from index

**Files:**
- Create: `src/label.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, PartialEq, Debug)] pub struct Rgb { pub r: u8, pub g: u8, pub b: u8 }`
  - `#[derive(Clone, Copy, PartialEq, Debug)] pub struct LabelColor { pub background: Rgb, pub foreground: Rgb }`
  - `pub fn create_label_color(index: usize) -> LabelColor` — deterministic; distinct indices generally get distinct hues; foreground is the same hue as the background but lighter when the background is dark and darker when the background is light.

- [ ] **Step 1: Register the module**

In `src/lib.rs`, add `pub mod label;` in alphabetical position (after `pub mod io;`, before `pub mod note;`):

```rust
pub mod cli;
pub mod commands;
pub mod config;
pub mod editor;
pub mod io;
pub mod label;
pub mod note;
pub mod output;
pub mod vcs;
```

- [ ] **Step 2: Write the failing tests**

Create `src/label.rs` with only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn brightness(color: Rgb) -> u32 {
        color.r as u32 + color.g as u32 + color.b as u32
    }

    #[test]
    fn hsl_primaries_convert_to_rgb() {
        assert_eq!(hsl_to_rgb(0.0, 1.0, 0.5), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(hsl_to_rgb(120.0, 1.0, 0.5), Rgb { r: 0, g: 255, b: 0 });
        assert_eq!(hsl_to_rgb(240.0, 1.0, 0.5), Rgb { r: 0, g: 0, b: 255 });
    }

    #[test]
    fn distinct_indices_get_distinct_backgrounds() {
        assert_ne!(
            create_label_color(0).background,
            create_label_color(1).background
        );
    }

    #[test]
    fn foreground_is_lighter_on_a_dark_background() {
        let color = create_label_color(0);
        assert!(
            brightness(color.foreground) > brightness(color.background),
            "expected a lighter foreground on a dark chip"
        );
    }

    #[test]
    fn foreground_is_darker_on_a_light_background() {
        let color = create_label_color(1);
        assert!(
            brightness(color.foreground) < brightness(color.background),
            "expected a darker foreground on a light chip"
        );
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib label`
Expected: FAIL — `cannot find type Rgb` / `cannot find function create_label_color`.

- [ ] **Step 4: Implement the public API and private HSL helper**

Prepend to `src/label.rs` (above the test module) — public API first, private helpers and consts at the bottom:

```rust
/// A 24-bit RGB color.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A label chip's background and foreground colors.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LabelColor {
    pub background: Rgb,
    pub foreground: Rgb,
}

/// Derive a stable color for the label at first-seen `index`. The hue is spread
/// by the golden angle so nearby indices look distinct; the foreground shares
/// the background's hue but is lighter on a dark chip and darker on a light one.
pub fn create_label_color(index: usize) -> LabelColor {
    let hue = (index as f64 * GOLDEN_ANGLE) % 360.0;
    let saturation = 0.55;
    let background_lightness = BACKGROUND_LIGHTNESS[index % BACKGROUND_LIGHTNESS.len()];
    let background = hsl_to_rgb(hue, saturation, background_lightness);

    let foreground_lightness = if background_lightness < 0.5 {
        (background_lightness + 0.45).min(0.95)
    } else {
        (background_lightness - 0.45).max(0.05)
    };
    let foreground = hsl_to_rgb(hue, saturation, foreground_lightness);

    LabelColor {
        background,
        foreground,
    }
}

/// Convert an HSL color (hue in degrees `[0, 360)`, saturation and lightness in
/// `[0, 1]`) to 24-bit RGB.
fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> Rgb {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h_prime = hue / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c / 2.0;
    Rgb {
        r: ((r1 + m) * 255.0).round() as u8,
        g: ((g1 + m) * 255.0).round() as u8,
        b: ((b1 + m) * 255.0).round() as u8,
    }
}

/// The golden angle, in degrees — spreads hues so adjacent indices differ.
const GOLDEN_ANGLE: f64 = 137.507_764_05;

/// Background lightness cycled by index so chips vary between dark and light,
/// which in turn drives the foreground contrast direction.
const BACKGROUND_LIGHTNESS: [f64; 3] = [0.30, 0.70, 0.45];
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib label`
Expected: PASS (4 tests).

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/lib.rs src/label.rs
git commit -m "feat: derive a stable per-index color for labels"
```

---

### Task 3: Render one colored, padded chip

**Files:**
- Modify: `src/label.rs`

**Interfaces:**
- Consumes: `LabelColor`, `Rgb` (Task 2).
- Produces: `pub fn render_label(label: &str, color: LabelColor) -> String` — returns `\x1b[38;2;r;g;bm\x1b[48;2;r;g;bm label \x1b[0m` (one space of padding each side, reset at the end).

- [ ] **Step 1: Write the failing test**

Add inside `mod tests` in `src/label.rs`:

```rust
    #[test]
    fn render_label_wraps_padded_text_in_color_codes() {
        let color = LabelColor {
            background: Rgb { r: 10, g: 20, b: 30 },
            foreground: Rgb { r: 200, g: 210, b: 220 },
        };
        let chip = render_label("feature", color);
        assert!(chip.contains(" feature "), "expected padded text in: {chip:?}");
        assert!(chip.contains("\x1b[48;2;10;20;30m"), "missing background: {chip:?}");
        assert!(chip.contains("\x1b[38;2;200;210;220m"), "missing foreground: {chip:?}");
        assert!(chip.ends_with("\x1b[0m"), "missing reset: {chip:?}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib label::tests::render_label_wraps_padded_text_in_color_codes`
Expected: FAIL — `cannot find function render_label`.

- [ ] **Step 3: Implement `render_label`**

Add to the public API section of `src/label.rs`, immediately after `create_label_color`:

```rust
/// Render one label as an ANSI-colored chip with a space of padding on each
/// side of the text, ending with a reset.
pub fn render_label(label: &str, color: LabelColor) -> String {
    let fg = color.foreground;
    let bg = color.background;
    format!(
        "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m {label} \x1b[0m",
        fg.r, fg.g, fg.b, bg.r, bg.g, bg.b
    )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib label`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/label.rs
git commit -m "feat: render a label as a padded ANSI chip"
```

---

### Task 4: Per-render color memory (`LabelPalette`)

**Files:**
- Modify: `src/label.rs`

**Interfaces:**
- Consumes: `create_label_color`, `LabelColor` (Task 2).
- Produces: `#[derive(Default)] pub struct LabelPalette`; `LabelPalette::new() -> Self`; `LabelPalette::color_for(&mut self, label: &str) -> LabelColor` — first sighting of a label assigns the next sequential index's color; later sightings of the same label return the stored color.

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests` in `src/label.rs`:

```rust
    #[test]
    fn palette_reuses_color_for_repeated_label() {
        let mut palette = LabelPalette::new();
        let first = palette.color_for("backend");
        let _other = palette.color_for("frontend");
        let again = palette.color_for("backend");
        assert_eq!(first, again);
    }

    #[test]
    fn palette_assigns_colors_in_first_seen_order() {
        let mut palette = LabelPalette::new();
        assert_eq!(palette.color_for("a"), create_label_color(0));
        assert_eq!(palette.color_for("b"), create_label_color(1));
        assert_eq!(palette.color_for("a"), create_label_color(0));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib label`
Expected: FAIL — `cannot find type LabelPalette`.

- [ ] **Step 3: Implement `LabelPalette`**

Add the import at the very top of `src/label.rs`:

```rust
use std::collections::HashMap;
```

Add to the public API section (after `render_label`):

```rust
/// Remembers which color each distinct label was assigned during a single
/// render, so a repeated label keeps its color. Never persisted between runs.
#[derive(Default)]
pub struct LabelPalette {
    assigned: HashMap<String, LabelColor>,
    next_index: usize,
}

impl LabelPalette {
    /// A palette with no assignments yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The color for `label`, assigning the next sequential color on first sight.
    pub fn color_for(&mut self, label: &str) -> LabelColor {
        if let Some(color) = self.assigned.get(label) {
            return *color;
        }
        let color = create_label_color(self.next_index);
        self.next_index += 1;
        self.assigned.insert(label.to_string(), color);
        color
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib label`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/label.rs
git commit -m "feat: remember per-render label colors in a palette"
```

---

### Task 5: Truncate and join a note's labels

**Files:**
- Modify: `src/label.rs`

**Interfaces:**
- Consumes: `render_label`, `LabelPalette` (Tasks 3–4).
- Produces: `pub fn render_labels(labels: &[String], max_visible: usize, palette: &mut LabelPalette) -> String` — empty string for no labels; otherwise up to `max_visible` colored chips joined by a single space, with `+N more` (plain text) appended when `labels.len() > max_visible`. Pass `usize::MAX` as `max_visible` to show every label.

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests` in `src/label.rs`:

```rust
    #[test]
    fn render_labels_empty_is_blank() {
        let mut palette = LabelPalette::new();
        assert_eq!(render_labels(&[], 3, &mut palette), "");
    }

    #[test]
    fn render_labels_under_limit_shows_all_without_overflow() {
        let mut palette = LabelPalette::new();
        let labels = vec!["feature".to_string(), "backend".to_string()];
        let out = render_labels(&labels, 3, &mut palette);
        assert!(out.contains(" feature "), "missing feature in: {out:?}");
        assert!(out.contains(" backend "), "missing backend in: {out:?}");
        assert!(!out.contains("more"), "unexpected overflow in: {out:?}");
    }

    #[test]
    fn render_labels_over_limit_truncates_with_overflow_marker() {
        let mut palette = LabelPalette::new();
        let labels = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ];
        let out = render_labels(&labels, 3, &mut palette);
        assert!(out.contains(" a "), "missing first label in: {out:?}");
        assert!(out.contains("+2 more"), "missing overflow marker in: {out:?}");
        assert!(!out.contains(" d "), "hidden label leaked in: {out:?}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib label`
Expected: FAIL — `cannot find function render_labels`.

- [ ] **Step 3: Implement `render_labels`**

Add to the public API section of `src/label.rs`, after `render_label` (and before `LabelPalette` is fine — keep public items grouped above the private helpers):

```rust
/// Render a note's labels as colored chips, showing at most `max_visible` and
/// appending `+N more` when some are hidden. `palette` keeps a repeated label's
/// color stable across notes in the same render.
pub fn render_labels(labels: &[String], max_visible: usize, palette: &mut LabelPalette) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let visible = labels.len().min(max_visible);
    let mut chips: Vec<String> = labels[..visible]
        .iter()
        .map(|label| render_label(label, palette.color_for(label)))
        .collect();
    let hidden = labels.len() - visible;
    if hidden > 0 {
        chips.push(format!("+{hidden} more"));
    }
    chips.join(" ")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib label`
Expected: PASS (all label tests).

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/label.rs
git commit -m "feat: render a note's labels as truncated colored chips"
```

---

### Task 6: Show colored labels in the `ls` list

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/output.rs`
- Modify: `src/commands/list.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `render_labels`, `LabelPalette` (Task 5); `Config::max_visible_labels()` (Task 1).
- Produces: `render_list_human(notes: &[Note], max_visible_labels: usize) -> String`; `commands::list::run(vcs: &dyn VersionControl, json: bool, max_visible_labels: usize) -> Result<()>`.

- [ ] **Step 1: Enable `tabled`'s `ansi` feature**

In `Cargo.toml`, change the `tabled` line so column widths account for ANSI escapes (this keeps the default `derive` feature — `default-features` is not disabled):

```toml
tabled = { version = "0.21", features = ["ansi"] }
```

- [ ] **Step 2: Write the failing test**

Add to `#[cfg(test)] mod tests` in `src/output.rs`:

```rust
    const RAW_LABELS: &str = "---\ntitle: A note\npath: 2026/06/02/a.md\nlabels:\n- feature\n- backend\n- priority\n- ops\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:02+01:00\n---\n\nBody\n";

    #[test]
    fn list_human_shows_colored_labels_truncated() {
        let note = parse_note(RAW_LABELS).unwrap();
        let text = render_list_human(&[note], 3);
        assert!(text.contains("\x1b["), "expected ANSI color codes in:\n{text}");
        assert!(text.contains("feature"), "expected first label in:\n{text}");
        assert!(text.contains("+1 more"), "expected overflow marker in:\n{text}");
    }
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib output::tests::list_human_shows_colored_labels_truncated`
Expected: FAIL — `render_list_human` takes 1 argument but 2 were supplied (and no `labels` column yet).

- [ ] **Step 4: Update `render_list_human`, `ListRow`, and imports**

In `src/output.rs`, add the label import near the top (after the existing `use crate::note...` line):

```rust
use crate::label::{self, LabelPalette};
```

Replace `render_list_human` with a version that threads a shared palette and a `max_visible_labels`:

```rust
/// Render a list of notes as a table (path, title, labels, updated), without content.
pub fn render_list_human(notes: &[Note], max_visible_labels: usize) -> String {
    let mut palette = LabelPalette::new();
    let rows: Vec<ListRow> = notes
        .iter()
        .map(|note| ListRow::from_note(note, max_visible_labels, &mut palette))
        .collect();
    let mut table = Table::new(rows);
    table.with(Style::modern());
    table.to_string()
}
```

Replace the `ListRow` struct and its `From<&Note>` impl (bottom of file) with a struct that has a `labels` column and a constructor taking the palette:

```rust
#[derive(Tabled)]
struct ListRow {
    path: String,
    title: String,
    labels: String,
    updated: String,
}

impl ListRow {
    fn from_note(note: &Note, max_visible_labels: usize, palette: &mut LabelPalette) -> Self {
        ListRow {
            path: note.meta.path.clone(),
            title: note.meta.title.clone(),
            labels: label::render_labels(&note.meta.labels, max_visible_labels, palette),
            updated: note.meta.updated.to_rfc2822(),
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib output`
Expected: PASS (existing output tests still pass; new one passes).

- [ ] **Step 6: Thread `max_visible_labels` through the list command**

In `src/commands/list.rs`, update `run`:

```rust
/// List notes, newest first. Prints a table, or JSON when `json` is set.
pub fn run(vcs: &dyn VersionControl, json: bool, max_visible_labels: usize) -> Result<()> {
    let mut notes = crate::commands::load_notes(vcs)?;
    notes.sort_by_key(|note| Reverse(note.meta.created));

    let rendered = if json {
        output::render_list_json(&notes)?
    } else {
        output::render_list_human(&notes, max_visible_labels)
    };
    println!("{rendered}");
    Ok(())
}
```

In `src/main.rs`, update the `List` dispatch arm:

```rust
        Some(Commands::List { json }) => {
            commands::list::run(backend.as_ref(), json, config.max_visible_labels())
        }
```

- [ ] **Step 7: Run the full suite**

Run: `cargo test`
Expected: PASS (all tests across all modules).

- [ ] **Step 8: Lint and commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add Cargo.toml Cargo.lock src/output.rs src/commands/list.rs src/main.rs
git commit -m "feat: show color-coded labels in the note list"
```

---

### Task 7: Color labels in the single-note view

**Files:**
- Modify: `src/output.rs`

**Interfaces:**
- Consumes: `render_labels`, `LabelPalette` (Task 5).
- Produces: no signature change to `render_note_human`; its `labels` row is now colored chips (all labels shown — no truncation, no persisted palette).

- [ ] **Step 1: Write the failing test**

Add to `#[cfg(test)] mod tests` in `src/output.rs`:

```rust
    #[test]
    fn note_human_colors_labels() {
        let note = parse_note(RAW_LABELS).unwrap();
        let text = render_note_human(&note);
        assert!(text.contains("labels"), "expected labels row in:\n{text}");
        assert!(text.contains("\x1b["), "expected ANSI color codes in:\n{text}");
        assert!(text.contains("feature"), "expected label text in:\n{text}");
        assert!(text.contains("ops"), "single-note view shows all labels:\n{text}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib output::tests::note_human_colors_labels`
Expected: FAIL — the labels row is still plain `feature, backend, ...` with no `\x1b[` codes.

- [ ] **Step 3: Color the labels row in `render_note_human`**

In `src/output.rs`, replace the labels block inside `render_note_human`:

```rust
    if !note.meta.labels.is_empty() {
        let mut palette = LabelPalette::new();
        let labels = label::render_labels(&note.meta.labels, usize::MAX, &mut palette);
        builder.push_record(["labels".to_string(), labels]);
    }
```

(`usize::MAX` shows every label; the throwaway `palette` is dropped when the function returns, so no color mapping is persisted.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib output`
Expected: PASS (including existing `note_human_shows_extra_meta`, which uses `labels: []` and so still renders no labels row).

- [ ] **Step 5: Manually verify the rendered output**

```bash
cargo build
```

(Optional visual check if a repo is configured: `cargo run -- ls` and `cargo run -- show <path>` should show colored label chips; `cargo run -- ls --json` must remain uncolored plain JSON.)

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings
git add src/output.rs
git commit -m "feat: color labels in the single-note view"
```

---

## Self-Review

**Spec coverage:**
- Labels visible in the list → Task 6 (new `labels` column). ✓
- Max 3 shown by default, `+N more` for the rest → Task 5 (`render_labels`) + Task 1 (default 3). ✓
- Configurable via `[list] max_visible_labels` → Task 1. ✓
- Color-coded backgrounds; foreground same hue, lighter on dark / darker on light → Task 2 (`create_label_color`). ✓
- One space padding before and after text → Task 3 (`render_label`). ✓
- Same label → same color within one list, via first-seen memory → Task 4 (`LabelPalette`); wired into the list in Task 6. ✓
- Mapping not persisted between executions → `LabelPalette` is stack-local per render (Global Constraints; Tasks 6–7). ✓
- Algorithmic color from label count, e.g. `create_label_color(4)` → Task 2 (public `create_label_color(index)`). ✓
- Single-note view also colored, without needing stored mapping → Task 7 (`usize::MAX` + throwaway palette). ✓

**Placeholder scan:** No TBD/TODO/"handle edge cases"/"similar to Task N" — every code step shows complete code. ✓

**Type consistency:** `LabelColor { background, foreground }`, `Rgb { r, g, b }`, `create_label_color(usize)`, `LabelPalette::{new, color_for}`, `render_label(&str, LabelColor)`, `render_labels(&[String], usize, &mut LabelPalette)`, `render_list_human(&[Note], usize)`, `commands::list::run(_, bool, usize)`, `Config::max_visible_labels() -> usize` — names and signatures match across all consuming tasks. ✓

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-07-03-colored-labels-in-list.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

**Which approach?**
