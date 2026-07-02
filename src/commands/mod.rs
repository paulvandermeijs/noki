pub mod create;
pub mod list;
pub mod show;

use crate::note::{Note, parse_note};
use crate::vcs::VersionControl;
use anyhow::Result;

/// Read and parse every note file in the repository.
pub(crate) fn load_notes(vcs: &dyn VersionControl) -> Result<Vec<Note>> {
    let mut notes = Vec::new();
    for path in vcs.list_files()? {
        let raw = vcs.read_file(&path)?;
        notes.push(parse_note(&raw)?);
    }
    Ok(notes)
}
