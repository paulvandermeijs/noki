pub mod git;

use anyhow::Result;

/// Backend-agnostic storage for notes, backed by a version-control system.
pub trait VersionControl {
    /// Repository-relative paths of every note file (`*.md`), using `/` separators.
    fn list_files(&self) -> Result<Vec<String>>;
    /// The raw contents of a single note file.
    fn read_file(&self, path: &str) -> Result<String>;
    /// Write a note file and record it (commit, and push when a remote exists).
    fn write_file(&self, path: &str, contents: &str, message: &str) -> Result<()>;
}

#[cfg(test)]
pub(crate) struct MemoryBackend {
    files: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
}

#[cfg(test)]
impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            files: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    #[allow(dead_code)]
    pub fn with_files(entries: &[(&str, &str)]) -> Self {
        let backend = Self::new();
        for (path, contents) in entries {
            backend.write_file(path, contents, "seed").unwrap();
        }
        backend
    }
}

#[cfg(test)]
impl VersionControl for MemoryBackend {
    fn list_files(&self) -> Result<Vec<String>> {
        Ok(self.files.lock().unwrap().keys().cloned().collect())
    }

    fn read_file(&self, path: &str) -> Result<String> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No note at {path}"))
    }

    fn write_file(&self, path: &str, contents: &str, _message: &str) -> Result<()> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), contents.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_backend_round_trips() {
        let backend = MemoryBackend::new();
        backend.write_file("a/b.md", "hello", "msg").unwrap();
        assert_eq!(backend.read_file("a/b.md").unwrap(), "hello");
        assert_eq!(backend.list_files().unwrap(), vec!["a/b.md".to_string()]);
    }

    #[test]
    fn memory_backend_read_missing_is_error() {
        let backend = MemoryBackend::new();
        assert!(backend.read_file("nope.md").is_err());
    }
}
