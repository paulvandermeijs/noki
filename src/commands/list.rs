use std::cmp::Reverse;
use std::io::{IsTerminal, stdout};

use crate::output;
use crate::vcs::VersionControl;
use anyhow::Result;

/// List notes, newest first. Prints a table, or JSON when `json` is set.
pub fn run(vcs: &dyn VersionControl, json: bool, max_visible_labels: usize) -> Result<()> {
    let mut notes = crate::commands::load_notes(vcs)?;
    notes.sort_by_key(|note| Reverse(note.meta.created));

    let rendered = if json {
        output::render_list_json(&notes)?
    } else {
        output::render_list_human(&notes, max_visible_labels, stdout().is_terminal())
    };
    println!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::vcs::MemoryBackend;

    const NOTE: &str = "---\ntitle: A note\npath: 2026/06/02/a.md\nlabels: []\ncreated: 2026-06-02T10:00:00+01:00\nupdated: 2026-06-02T10:00:00+01:00\n---\n\nBody\n";

    #[test]
    fn load_notes_parses_all_files() {
        let backend = MemoryBackend::with_files(&[("2026/06/02/a.md", NOTE)]);
        let notes = crate::commands::load_notes(&backend).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].meta.title, "A note");
    }

    #[test]
    fn load_notes_skips_unparseable_files() {
        let backend = MemoryBackend::with_files(&[
            ("2026/06/02/a.md", NOTE),
            ("README.md", "no frontmatter here"),
        ]);
        assert_eq!(crate::commands::load_notes(&backend).unwrap().len(), 1);
    }
}
