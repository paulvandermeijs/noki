use std::io::{IsTerminal, stdout};

use crate::note::parse_note;
use crate::output;
use crate::vcs::VersionControl;
use anyhow::Result;

/// Show a single note identified by its repository-relative path.
pub fn run(vcs: &dyn VersionControl, path: &str, json: bool, raw: bool) -> Result<()> {
    let contents = vcs.read_file(path)?;

    if raw {
        print!("{contents}");
        return Ok(());
    }

    let note = parse_note(&contents)?;
    let rendered = if json {
        output::render_note_json(&note)?
    } else {
        output::render_note_human(&note, stdout().is_terminal())
    };
    println!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::MemoryBackend;

    const NOTE: &str = "---\ntitle: A note\npath: 2026/06/02/a.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:00+01:00\n---\n\nBody\n";

    #[test]
    fn show_missing_note_errors() {
        let backend = MemoryBackend::new();
        let err = run(&backend, "nope.md", false, false).unwrap_err();
        assert_eq!(err.to_string(), "No note at nope.md");
    }

    #[test]
    fn show_renders_human_output() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/a.md", NOTE)]);
        assert!(run(&backend, "2026/06/02/a.md", false, false).is_ok());
    }

    #[test]
    fn show_renders_json_output() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/a.md", NOTE)]);
        assert!(run(&backend, "2026/06/02/a.md", true, false).is_ok());
    }

    #[test]
    fn show_renders_raw_output() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/a.md", NOTE)]);
        assert!(run(&backend, "2026/06/02/a.md", false, true).is_ok());
    }
}
