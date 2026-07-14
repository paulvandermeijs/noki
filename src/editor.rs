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
    command.arg(path);
    reset_sigint(&mut command);
    let status = command
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

/// `read_stdin` may have marked SIGINT ignored (see `io::ignore_sigint`);
/// ignored dispositions survive exec, so without a reset the editor would
/// start with Ctrl+C disabled. Restore the default for the child only.
#[cfg(unix)]
fn reset_sigint(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: the pre_exec closure runs in the forked child before exec and
    // only calls `signal`, which is async-signal-safe; no allocation, no
    // locks, no access to parent state.
    unsafe {
        command.pre_exec(|| {
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn reset_sigint(_command: &mut Command) {}

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

    #[cfg(unix)]
    #[test]
    fn run_editor_resets_sigint_to_default_in_the_child() {
        use std::io::Write as _;
        use std::os::unix::fs::PermissionsExt;

        // Simulate `read_stdin` having left SIGINT ignored (see io::ignore_sigint).
        // If that disposition leaked into the editor child across exec, the
        // script below would survive its own `kill -INT $$` and exit 0.
        unsafe { libc::signal(libc::SIGINT, libc::SIG_IGN) };

        let mut script = tempfile::NamedTempFile::new().unwrap();
        script
            .write_all(b"#!/bin/sh\nkill -INT $$\nexit 0\n")
            .unwrap();
        let mut perms = script.as_file().metadata().unwrap().permissions();
        perms.set_mode(0o755);
        script.as_file().set_permissions(perms).unwrap();

        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let script_path = script.path().to_str().unwrap();
        let err = run_editor(script_path, tmpfile.path()).unwrap_err();
        assert!(
            err.to_string().contains("exited"),
            "expected the script to die from its own SIGINT, got: {err}"
        );
    }
}
