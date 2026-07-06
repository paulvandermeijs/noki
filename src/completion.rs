use crate::config;
use crate::vcs::{self, VersionControl};
use clap_complete::engine::CompletionCandidate;

/// Completion candidates for a note `path` argument: every note file in the
/// configured repository.
///
/// Config is resolved from the global config file and the `.noki.toml` chain
/// only — a `--repository` typed on the command line is not visible during
/// completion. Candidates are produced only when the repository has already
/// been cloned locally; completion never clones over the network. Any error is
/// swallowed so completion never fails or blocks the shell.
pub fn note_paths() -> Vec<CompletionCandidate> {
    load_backend()
        .map(|backend| candidates(backend.as_ref()))
        .unwrap_or_default()
}

fn candidates(backend: &dyn VersionControl) -> Vec<CompletionCandidate> {
    backend
        .list_files()
        .unwrap_or_default()
        .into_iter()
        .map(CompletionCandidate::new)
        .collect()
}

fn load_backend() -> Option<Box<dyn VersionControl>> {
    let config = config::load(None).ok()?;
    vcs::open_existing(&config).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::MemoryBackend;

    fn values(candidates: &[CompletionCandidate]) -> Vec<String> {
        candidates
            .iter()
            .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn candidates_are_the_note_paths() {
        let backend = MemoryBackend::with_files(&[("b/c.md", "y"), ("a.md", "x")]);
        assert_eq!(values(&candidates(&backend)), vec!["a.md", "b/c.md"]);
    }

    #[test]
    fn no_notes_yields_no_candidates() {
        let backend = MemoryBackend::new();
        assert!(candidates(&backend).is_empty());
    }
}
