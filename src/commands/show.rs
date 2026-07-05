use std::io::{IsTerminal, stdout};

use crate::note::parse_note;
use crate::output;
use crate::vcs::VersionControl;
use anyhow::Result;

/// Show a single note identified by its repository-relative path. `max_width`
/// (from `note.max_width`), when `Some`, forces both the rendered body and the
/// metadata table to that many columns, clamped down to the terminal width;
/// `None` adapts to content up to the terminal width.
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
        output::render_note_human(&note, table_width(max_width), stdout().is_terminal())
    };
    println!("{rendered}");
    Ok(())
}

/// Resolve the column budget for `show` from the configured `max_width` and the
/// current terminal width.
fn table_width(max_width: Option<usize>) -> output::TableWidth {
    table_width_for(terminal_width(), max_width)
}

/// Decide the table sizing given the `available` terminal columns: a configured
/// cap (clamped to `available`, floored at 1) yields a fixed width; no cap
/// adapts to content up to `available`.
fn table_width_for(available: usize, max_width: Option<usize>) -> output::TableWidth {
    match max_width {
        Some(cap) => output::TableWidth::Fixed(available.min(cap.max(1))),
        None => output::TableWidth::Fit(available),
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
    fn table_width_fits_available_when_no_cap() {
        assert_eq!(table_width_for(120, None), output::TableWidth::Fit(120));
    }

    #[test]
    fn table_width_fixes_to_smaller_of_available_and_cap() {
        assert_eq!(
            table_width_for(120, Some(80)),
            output::TableWidth::Fixed(80)
        );
        assert_eq!(table_width_for(50, Some(80)), output::TableWidth::Fixed(50));
    }

    #[test]
    fn table_width_never_fixes_to_zero() {
        assert_eq!(table_width_for(120, Some(0)), output::TableWidth::Fixed(1));
    }
}
