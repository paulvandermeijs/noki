use anyhow::{Context, Result};
use log::info;
use std::collections::VecDeque;
use std::io::Write;
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

    let mut args = editor_command.split(' ').collect::<VecDeque<&str>>();
    let program = args.pop_front().context("No editor configured.")?;
    let mut command = Command::new(program);
    for arg in args {
        command.arg(arg);
    }
    command
        .arg(path)
        .spawn()
        .with_context(|| format!("Failed to run editor command {program}"))?
        .wait()
        .context("Editor command returned a non-zero status")?;

    Ok(std::fs::read_to_string(path)?)
}

fn editor_command() -> Option<String> {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .ok()
}
