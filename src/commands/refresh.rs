use crate::vcs::VersionControl;
use anyhow::Result;

/// Fetch the latest notes from the remote and rebase the local clone onto them.
pub fn run(vcs: &dyn VersionControl) -> Result<()> {
    vcs.refresh()?;
    println!("Refreshed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::vcs::MemoryBackend;

    #[test]
    fn run_returns_ok() {
        let backend = MemoryBackend::new();
        assert!(super::run(&backend).is_ok());
    }
}
