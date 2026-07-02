use crate::vcs::VersionControl;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct GitBackend {
    workdir: PathBuf,
}

impl GitBackend {
    /// Open the working clone at `dest`, cloning `url` into it if absent.
    pub fn open_or_clone(url: &str, dest: &Path) -> Result<Self> {
        if dest.join(".git").exists() {
            return Ok(Self {
                workdir: dest.to_path_buf(),
            });
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        clone(url, dest)?;
        Ok(Self {
            workdir: dest.to_path_buf(),
        })
    }
}

impl VersionControl for GitBackend {
    fn list_files(&self) -> Result<Vec<String>> {
        let mut files = Vec::new();
        collect_notes(&self.workdir, &self.workdir, &mut files)?;
        files.sort();
        Ok(files)
    }

    fn read_file(&self, path: &str) -> Result<String> {
        std::fs::read_to_string(self.workdir.join(path))
            .with_context(|| format!("No note at {path}"))
    }

    fn write_file(&self, path: &str, contents: &str, message: &str) -> Result<()> {
        let full = self.workdir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, contents)?;
        commit_and_push(&self.workdir, path, message)
    }
}

fn clone(url: &str, dest: &Path) -> Result<()> {
    let mut prepare = gix::prepare_clone(url, dest).context("Failed to start clone")?;
    let (mut checkout, _) = prepare
        .fetch_then_checkout(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
        .context("Failed to fetch repository")?;
    checkout
        .main_worktree(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
        .context("Failed to check out worktree")?;
    Ok(())
}

fn collect_notes(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == ".git" {
            continue;
        }
        if path.is_dir() {
            collect_notes(root, &path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            // `path` is always produced by walking `root`, so stripping it cannot fail.
            let rel = path
                .strip_prefix(root)
                .expect("walked path is within the repository root");
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn commit_and_push(workdir: &Path, path: &str, message: &str) -> Result<()> {
    let repo = git2::Repository::open(workdir)?;

    let mut index = repo.index()?;
    index.add_path(Path::new(path))?;
    index.write()?;
    let tree = repo.find_tree(index.write_tree()?)?;

    let signature = repo
        .signature()
        .context("No Git identity configured. Set user.name and user.email in your Git config.")?;
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )?;

    // The commit is safe on disk; a failed push must not lose the note.
    if let Err(error) = push(&repo) {
        eprintln!("Warning: committed locally but failed to push to origin: {error:#}");
    }
    Ok(())
}

fn push(repo: &git2::Repository) -> Result<()> {
    let mut remote = match repo.find_remote("origin") {
        Ok(remote) => remote,
        Err(_) => return Ok(()), // no remote: local-only repository
    };
    let branch = repo
        .head()?
        .shorthand()
        .context("Cannot push from a detached HEAD")?
        .to_string();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    let config = repo.config()?;
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(move |url, username, allowed| {
        if allowed.contains(git2::CredentialType::SSH_KEY) {
            git2::Cred::ssh_key_from_agent(username.unwrap_or("git"))
        } else if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            git2::Cred::credential_helper(&config, url, username)
        } else {
            git2::Cred::default()
        }
    });
    let mut options = git2::PushOptions::new();
    options.remote_callbacks(callbacks);
    remote
        .push(&[refspec.as_str()], Some(&mut options))
        .context("Failed to push to origin")
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};

    // Create a repository at `path` with one initial commit so HEAD exists.
    fn init_repo_with_commit(path: &Path) -> Repository {
        let repo = Repository::init(path).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }
        std::fs::write(path.join("seed.txt"), "seed\n").unwrap();
        {
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("seed.txt")).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let sig = Signature::now("Test", "test@example.com").unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                .unwrap();
        }
        repo
    }

    #[test]
    fn open_existing_repo() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path());
        let backend = GitBackend::open_or_clone(dir.path().to_str().unwrap(), dir.path()).unwrap();
        assert!(backend.list_files().unwrap().is_empty()); // only seed.txt, no *.md notes yet
    }

    #[test]
    fn write_then_read_and_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path());
        let backend = GitBackend::open_or_clone(dir.path().to_str().unwrap(), dir.path()).unwrap();

        backend
            .write_file("2026/06/02/note.md", "hello\n", "Add note")
            .unwrap();

        assert_eq!(backend.read_file("2026/06/02/note.md").unwrap(), "hello\n");
        assert!(
            backend
                .list_files()
                .unwrap()
                .contains(&"2026/06/02/note.md".to_string())
        );
        let head_msg = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message()
            .unwrap()
            .to_string();
        assert_eq!(head_msg, "Add note");
    }
}
