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
        if name.starts_with('.') {
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

    // The commit is safe on disk; a failed fetch/rebase/push must not lose the note.
    if let Err(error) = sync_and_push(&repo) {
        eprintln!("Warning: committed locally but failed to push to origin: {error:#}");
    }
    Ok(())
}

fn remote_callbacks(config: git2::Config) -> git2::RemoteCallbacks<'static> {
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
    callbacks
}

fn fetch_origin(repo: &git2::Repository, remote: &mut git2::Remote) -> Result<()> {
    let mut options = git2::FetchOptions::new();
    options.remote_callbacks(remote_callbacks(repo.config()?));
    // Empty refspec list uses origin's configured fetch refspecs, updating refs/remotes/origin/*.
    let refspecs: [&str; 0] = [];
    remote
        .fetch(&refspecs, Some(&mut options), None)
        .context("Failed to fetch from origin")
}

fn rebase_onto_origin(repo: &git2::Repository, branch: &str) -> Result<()> {
    let upstream_ref = format!("refs/remotes/origin/{branch}");
    let upstream = match repo.find_reference(&upstream_ref) {
        Ok(reference) => reference.peel_to_commit()?,
        Err(_) => return Ok(()), // origin has no matching branch yet: nothing to rebase onto
    };
    let local = repo.head()?.peel_to_commit()?;
    // If local already contains upstream, a plain push fast-forwards; no rebase needed.
    if local.id() == upstream.id() || repo.graph_descendant_of(local.id(), upstream.id())? {
        return Ok(());
    }

    let onto = repo.find_annotated_commit(upstream.id())?;
    let signature = repo.signature()?;
    let mut rebase = repo.rebase(None, Some(&onto), None, None)?;

    let result = (|| -> Result<()> {
        while let Some(operation) = rebase.next() {
            operation.context("Rebase step failed")?;
            if repo.index()?.has_conflicts() {
                anyhow::bail!("Rebase conflict while integrating origin changes");
            }
            rebase.commit(None, &signature, None)?;
        }
        rebase.finish(Some(&signature))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = rebase.abort(); // restore HEAD; the local commit is preserved
    }
    result
}

fn sync_and_push(repo: &git2::Repository) -> Result<()> {
    let mut remote = match repo.find_remote("origin") {
        Ok(remote) => remote,
        Err(_) => return Ok(()), // no remote: local-only repository
    };
    let branch = repo
        .head()?
        .shorthand()
        .context("Cannot push from a detached HEAD")?
        .to_string();

    fetch_origin(repo, &mut remote)?;
    rebase_onto_origin(repo, &branch)?;
    push(repo, &mut remote, &branch)
}

fn push(repo: &git2::Repository, remote: &mut git2::Remote, branch: &str) -> Result<()> {
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let mut options = git2::PushOptions::new();
    options.remote_callbacks(remote_callbacks(repo.config()?));
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

    // Commit `contents` to `name` in `repo`'s working tree, on top of the current HEAD.
    fn commit_file(repo: &Repository, name: &str, contents: &str, message: &str) -> git2::Oid {
        let workdir = repo.workdir().unwrap();
        let full = workdir.join(name);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full, contents).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("Test", "test@example.com").unwrap();
        let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = head.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap()
    }

    fn set_identity(repo: &Repository) {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }

    // Push `master` to the repo's `origin` (a local bare path needs no credentials).
    fn push_master(repo: &Repository) {
        let mut remote = repo.find_remote("origin").unwrap();
        remote
            .push(&["refs/heads/master:refs/heads/master"], None)
            .unwrap();
    }

    // A bare `origin` seeded with one commit, plus a working "seed" clone wired to it.
    // Returns (origin tempdir, seed tempdir, seed repo). Both tempdirs must stay alive.
    fn origin_with_seed() -> (tempfile::TempDir, tempfile::TempDir, Repository) {
        let origin_dir = tempfile::tempdir().unwrap();
        Repository::init_bare(origin_dir.path()).unwrap();
        let origin_url = origin_dir.path().to_str().unwrap();

        let seed_dir = tempfile::tempdir().unwrap();
        let seed = Repository::init(seed_dir.path()).unwrap();
        set_identity(&seed);
        commit_file(&seed, "seed.txt", "seed\n", "initial");
        seed.remote("origin", origin_url).unwrap();
        push_master(&seed);

        (origin_dir, seed_dir, seed)
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

    #[test]
    fn push_rebases_onto_diverged_origin() {
        let (origin_dir, _seed_dir, seed) = origin_with_seed();
        let origin_url = origin_dir.path().to_str().unwrap();

        // noki's private clone of origin.
        let workdir = tempfile::tempdir().unwrap();
        let noki = Repository::clone(origin_url, workdir.path()).unwrap();
        set_identity(&noki);

        // Someone advances origin behind noki's back (e.g. a wiki edit via the web UI).
        commit_file(&seed, "other.txt", "web edit\n", "unrelated remote change");
        push_master(&seed);

        // noki captures a note; it must fetch, rebase onto origin, and push.
        let backend = GitBackend {
            workdir: workdir.path().to_path_buf(),
        };
        backend
            .write_file("note.md", "hello\n", "Add note")
            .unwrap();

        // Origin now contains BOTH the note and the unrelated remote change.
        let origin_repo = Repository::open_bare(origin_dir.path()).unwrap();
        let tree = origin_repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_path(Path::new("note.md")).is_ok(), "note pushed");
        assert!(
            tree.get_path(Path::new("other.txt")).is_ok(),
            "remote change preserved"
        );

        // History is linear: the note commit has exactly one parent (the remote change).
        let local = Repository::open(workdir.path()).unwrap();
        assert_eq!(
            local
                .head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .parent_count(),
            1
        );
    }
}
