//! A repository that moved on GitHub, to another owner or another name.
//!
//! GitHub sends the old address on to the new one, so a moved repository
//! keeps syncing under the name Orbit has for it. Orbit sees the move when
//! GitHub names the repository differently: as an open pull request's base,
//! or when Orbit reads the repository itself (`engine/repo_moves.rs`). It
//! then follows the repository to its new name, so its history, settings
//! and threads carry on there: in the state, the name is rewritten wherever
//! it stands on its own in a piece of text (`rename_text`), and the stored
//! pull-request history moves to the new name's folder.
//!
//! Some things keep their old text. A path is never rewritten, as the
//! folders on this computer didn't move. A repository keeps its earlier
//! names (`Repository::former`), as its clone's `origin` may still name one;
//! Orbit then reads the clone's repository from the same address with its
//! current name (`engine::settings::remote_for`), as GitHub sends the old
//! one on only until another repository takes that name. And a CI thread
//! already on GitHub keeps the key its hidden line was posted with, through
//! every later move (`ops::Thread::marked`), and Orbit's copy of its text, so
//! Orbit still finds and edits its own comment or issue there.

use crate::types::{same_repo, validate_repo, Snapshot};
use serde_json::Value;
use std::path::Path;

/// Whether `c` can be part of an owner or repository name, or joins one to
/// a path.
fn joins(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '\\' | '~')
}

/// `text` with each `old` that stands on its own renamed `new`, or None
/// when it has none. GitHub's names ignore case, so any case of `old`
/// counts; one written in lower case, like a pull request's id, becomes
/// `new` in lower case. A name inside a longer name or a path (`x/old/name`,
/// `old/name-two`, `old/name/file`) isn't this repository, and stays.
pub fn rename_text(text: &str, old: &str, new: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let wanted = old.to_ascii_lowercase();
    if wanted.is_empty() {
        return None;
    }
    let mut out = String::new();
    let (mut copied, mut from) = (0, 0);
    while let Some(at) = lower[from..].find(&wanted).map(|i| i + from) {
        let end = at + wanted.len();
        from = at + 1;
        let before = text[..at].chars().next_back();
        let mut rest = text[end..].chars();
        let after = rest.next();
        let alone = before.is_none_or(|c| !joins(c))
            && match after {
                None => true,
                // The end of a sentence.
                Some('.') => rest.next().is_none_or(char::is_whitespace),
                Some(c) => !joins(c),
            };
        if !alone {
            continue;
        }
        let found = &text[at..end];
        out.push_str(&text[copied..at]);
        if found.bytes().any(|b| b.is_ascii_uppercase()) {
            out.push_str(new);
        } else {
            out.push_str(&new.to_ascii_lowercase());
        }
        copied = end;
        from = end;
    }
    (copied > 0).then(|| {
        out.push_str(&text[copied..]);
        out
    })
}

/// Renames `old` to `new` in every string and every key of `v`. When a
/// renamed key is already there, the entry under it stays, as it was
/// written under the new name. Returns whether anything changed.
pub fn rename_value(v: &mut Value, old: &str, new: &str) -> bool {
    match v {
        Value::String(text) => match rename_text(text, old, new) {
            Some(renamed) => {
                *text = renamed;
                true
            }
            None => false,
        },
        Value::Array(items) => items.iter_mut().fold(false, |changed, item| {
            rename_value(item, old, new) | changed
        }),
        Value::Object(map) => {
            let mut changed = false;
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                let Some(renamed) = rename_text(&key, old, new) else {
                    continue;
                };
                changed = true;
                if let Some(item) = map.remove(&key) {
                    map.entry(renamed).or_insert(item);
                }
            }
            for item in map.values_mut() {
                changed |= rename_value(item, old, new);
            }
            changed
        }
        _ => false,
    }
}

/// The state `s` with the repository `old` renamed `new` everywhere, as if
/// it had always been called that, apart from paths and what Orbit already
/// posted. What isn't saved with the state is kept as
/// it was, or, for what Orbit only keeps between reads (such as a commit's
/// checks by name), starts again as after a restart.
pub fn renamed(s: &Snapshot, old: &str, new: &str) -> Result<Snapshot, String> {
    validate_repo(new)?;
    let mut v = serde_json::to_value(s).map_err(|e| e.to_string())?;
    rename_value(&mut v, old, new);
    let mut out: Snapshot = serde_json::from_value(v).map_err(|e| e.to_string())?;
    out.usage = s.usage.clone();
    out.password_store = s.password_store.clone();
    for r in &mut out.reviews {
        if let Some(was) = s.reviews.iter().find(|w| w.id == r.id) {
            r.extraction_problem = was.extraction_problem.clone();
        }
    }
    // A thread on GitHub, or perhaps on it, keeps its hidden line: the one
    // it kept through an earlier move, or else the key it had before this
    // one. Its copy of what GitHub has stays as GitHub has it.
    for (t, was) in out.ops.iter_mut().zip(&s.ops) {
        t.posted = was.posted.clone();
        t.marked = was.marked.clone().or_else(|| {
            (t.key != was.key && (was.is_posted() || was.unsure)).then(|| was.key.clone())
        });
    }
    // The name as GitHub has it, wherever Orbit shows it, and the names a
    // repository had before, for its clone's `origin`.
    for (r, was) in out
        .settings
        .repositories
        .iter_mut()
        .zip(&s.settings.repositories)
    {
        r.former = was.former.clone();
        if same_repo(&was.name, old) {
            r.name = new.into();
            r.former
                .retain(|name| !same_repo(name, old) && !same_repo(name, new));
            r.former.push(was.name.clone());
        }
    }
    for p in out.prs.iter_mut().chain(out.history_prs.iter_mut()) {
        if same_repo(&p.repo, new) {
            p.repo = new.into();
        }
    }
    out.settings.validate()?;
    Ok(out)
}

/// Moves the stored pull-request history of `old` to `new`, renaming it in
/// each stored file.
pub fn move_history(root: &Path, old: &str, new: &str) -> Result<(), String> {
    crate::ledger::rename(root, old, new, &|v| {
        rename_value(v, old, new);
    })
}

#[cfg(test)]
#[path = "repo_moves_tests.rs"]
mod tests;
