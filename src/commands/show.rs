use std::io::{IsTerminal, stdout};

use crate::note::parse_note;
use crate::output;
use crate::vcs::VersionControl;
use anyhow::Result;

/// Show a single note identified by its repository-relative path. `max_width`,
/// when `Some`, caps both the rendered body and the metadata table to that many
/// columns (from `note.max_width`); `None` uses the full terminal width.
pub fn run(
    vcs: &dyn VersionControl,
    path: &str,
    json: bool,
    raw: bool,
    max_width: Option<usize>,
) -> Result<()> {
    let contents = vcs.read_file(path)?;

    if raw {
        print!("{contents}");
        return Ok(());
    }

    let note = parse_note(&contents)?;
    let rendered = if json {
        output::render_note_json(&note)?
    } else {
        let width = cap_width(terminal_width(), max_width);
        let table_width = max_width.map(|_| width);
        output::render_note_human(&note, width, table_width, stdout().is_terminal())
    };
    println!("{rendered}");
    Ok(())
}

/// The effective wrap width: the smaller of the available width and a configured
/// cap, or the full available width when no cap is set.
fn cap_width(available: usize, max_width: Option<usize>) -> usize {
    match max_width {
        Some(cap) => available.min(cap.max(1)),
        None => available,
    }
}

/// The current terminal width in columns, or 80 when it can't be determined
/// (e.g. output is piped).
fn terminal_width() -> usize {
    terminal_size::terminal_size().map_or(80, |(terminal_size::Width(cols), _)| cols as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::MemoryBackend;

    const NOTE: &str = "---\ntitle: A note\npath: 2026/06/02/a.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:00+01:00\n---\n\nBody\n";

    #[test]
    fn show_missing_note_errors() {
        let backend = MemoryBackend::new();
        let err = run(&backend, "nope.md", false, false, None).unwrap_err();
        assert_eq!(err.to_string(), "No note at nope.md");
    }

    #[test]
    fn show_renders_human_output() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/a.md", NOTE)]);
        assert!(run(&backend, "2026/06/02/a.md", false, false, None).is_ok());
    }

    #[test]
    fn show_renders_human_output_with_max_width() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/a.md", NOTE)]);
        assert!(run(&backend, "2026/06/02/a.md", false, false, Some(60)).is_ok());
    }

    #[test]
    fn show_renders_json_output() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/a.md", NOTE)]);
        assert!(run(&backend, "2026/06/02/a.md", true, false, None).is_ok());
    }

    #[test]
    fn show_renders_raw_output() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/a.md", NOTE)]);
        assert!(run(&backend, "2026/06/02/a.md", false, true, None).is_ok());
    }

    #[test]
    fn cap_width_uses_available_when_no_cap() {
        assert_eq!(cap_width(120, None), 120);
    }

    #[test]
    fn cap_width_caps_to_smaller_of_available_and_max() {
        assert_eq!(cap_width(120, Some(80)), 80);
        assert_eq!(cap_width(50, Some(80)), 50);
    }

    #[test]
    fn cap_width_never_returns_zero() {
        assert_eq!(cap_width(120, Some(0)), 1);
    }
}
