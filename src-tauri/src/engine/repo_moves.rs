//! Following a repository that moved on GitHub to its new name (see
//! `crate::repo_moves`).
use super::{ledger::blocking, *};
use crate::{github, repo_moves};

/// How often Orbit asks GitHub a repository's name. An open pull request
/// tells it on every sync.
pub(super) const NAME_EVERY: Duration = Duration::from_secs(24 * 3600);
/// How long following a move waits for background reads to finish; if
/// they don't, it waits for a later sync.
const QUIET: Duration = if cfg!(test) {
    Duration::from_millis(300)
} else {
    Duration::from_secs(10)
};

/// Where GitHub says `repo` is now, from the pull requests just read under
/// that name: their base, when GitHub names it differently.
pub(super) fn moved_to(repo: &str, found: &[PullRequest]) -> Option<String> {
    found
        .iter()
        .map(|p| p.base_repo.as_str())
        .find(|base| !base.is_empty() && !same_repo(base, repo) && validate_repo(base).is_ok())
        .map(str::to_owned)
}

impl Engine {
    /// Asks GitHub the name of each repository in `repos` it hasn't asked
    /// about for `NAME_EVERY`, and adds those that moved to `moves`.
    pub(super) async fn read_names(&self, repos: &[String], moves: &mut Vec<(String, String)>) {
        for repo in repos {
            if moves.iter().any(|(old, _)| same_repo(old, repo)) {
                continue;
            }
            let key = repo.to_ascii_lowercase();
            let due = self
                .names_read
                .lock()
                .unwrap()
                .get(&key)
                .is_none_or(|at| at.elapsed() >= NAME_EVERY);
            if !due {
                continue;
            }
            self.names_read
                .lock()
                .unwrap()
                .insert(key, std::time::Instant::now());
            // One GitHub doesn't answer for is asked again the next day: its
            // sync says what's wrong meanwhile.
            if let Ok(name) = github::full_name(repo).await {
                if !same_repo(&name, repo) {
                    moves.push((repo.clone(), name));
                }
            }
        }
    }
    /// Follows each repository in `moves` to its new name. One Orbit can't
    /// follow now, while it's busy with GitHub, is followed after a later
    /// sync. Called holding `poll_lock`.
    pub(super) async fn follow_moves(&self, moves: Vec<(String, String)>) {
        for (old, new) in moves {
            let followed = matches!(self.follow_move(&old, &new).await, Ok(true));
            let mut read = self.names_read.lock().unwrap();
            if followed {
                read.insert(new.to_ascii_lowercase(), std::time::Instant::now());
            } else {
                read.remove(&old.to_ascii_lowercase());
            }
        }
    }
    /// Renames the repository `old` to `new` in Orbit's state and stored
    /// history, or, when Orbit watches `new` already, stops watching `old`
    /// so its pull requests aren't reviewed twice. Nothing else that reads
    /// GitHub into the state runs meanwhile, so nothing writes under the
    /// old name after the rename. Returns whether it did, or Ok(false) when
    /// it has to wait.
    pub(super) async fn follow_move(&self, old: &str, new: &str) -> Result<bool, String> {
        if self.moving.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let _publish = self.publish_lock.lock().await;
        let _cloning = self.cloning.lock().await;
        let _rebasing = self.rebasing.lock().await;
        let _tagging = self.tagging.lock().await;
        let Ok(_quiet) = self.quiet(tokio::time::Instant::now() + QUIET).await else {
            return Ok(false);
        };
        let renamed = {
            let mut s = self.state.lock().await;
            if !s
                .settings
                .repositories
                .iter()
                .any(|r| same_repo(&r.name, old))
            {
                return Ok(false);
            }
            let already = s
                .settings
                .repositories
                .iter()
                .any(|r| same_repo(&r.name, new));
            if already {
                s.settings.repositories.retain(|r| !same_repo(&r.name, old));
                crate::buddy::moved(&mut s, old, new, true);
            } else {
                let mut moved = repo_moves::renamed(&s, old, new)?;
                crate::buddy::moved(&mut moved, old, new, false);
                *s = moved;
            }
            self.dirty.store(true, Ordering::SeqCst);
            !already
        };
        if renamed {
            let (root, from, to) = (self.root.clone(), old.to_string(), new.to_string());
            // The history is Orbit's copy of what GitHub has: what didn't
            // move is read again, and the old name's folder is let go with
            // the history of repositories no longer connected.
            let _ = blocking(move || repo_moves::move_history(&root, &from, &to)).await;
            self.update(|s| s.ledger.version += 1).await;
        }
        let _ = self.flush().await;
        Ok(true)
    }
}
