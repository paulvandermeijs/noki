use anyhow::{Context, Result};
use log::info;
use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// Open a temporary Markdown file in the user's editor and return its contents.
/// `input`, when present, pre-fills the file.
pub fn get_content_from_editor(input: Option<String>) -> Result<String> {
    let editor_command =
        editor_command().context("No editor configured. Set $VISUAL or $EDITOR.")?;

    let mut tmpfile = tempfile::NamedTempFile::with_suffix(".md")?;
    if let Some(input) = input {
        tmpfile.write_all(input.as_bytes())?;
    }
    let path = tmpfile.path();
    info!("Using temp file {path:?}");

    run_editor(&editor_command, path)?;

    Ok(std::fs::read_to_string(path)?)
}

/// Run `editor_command` (a program plus space-separated arguments) on `path`
/// and wait for it to finish. A non-zero exit (e.g. vim's `:cq`) is an error,
/// so an aborted edit never gets captured as a note.
fn run_editor(editor_command: &str, path: &Path) -> Result<()> {
    let mut args = editor_command.split(' ').collect::<VecDeque<&str>>();
    let program = args.pop_front().context("No editor configured.")?;
    let mut command = Command::new(program);
    for arg in args {
        command.arg(arg);
    }
    let status = command
        .arg(path)
        .status()
        .with_context(|| format!("Failed to run editor command {program}"))?;
    if !status.success() {
        anyhow::bail!("Editor exited with {status}; the note was not saved");
    }
    Ok(())
}

fn editor_command() -> Option<String> {
    for key in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_editor_errors_when_editor_exits_non_zero() {
        // An aborted edit (e.g. vim's `:cq`) exits non-zero; the note must not
        // be captured as if the edit succeeded.
        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let err = run_editor("false", tmpfile.path()).unwrap_err();
        assert!(
            err.to_string().contains("exited"),
            "expected an exit-status error, got: {err}"
        );
    }

    #[test]
    fn run_editor_succeeds_when_editor_exits_zero() {
        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        assert!(run_editor("true", tmpfile.path()).is_ok());
    }

    #[test]
    fn run_editor_errors_when_program_is_missing() {
        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let err = run_editor("definitely-not-a-real-editor", tmpfile.path()).unwrap_err();
        assert!(
            err.to_string().contains("Failed to run editor"),
            "expected a spawn error, got: {err}"
        );
    }
}
