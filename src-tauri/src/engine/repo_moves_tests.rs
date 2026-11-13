#![cfg(unix)]
use super::*;
use std::path::Path;

/// A gh that knows one repository, `a/b`: its open pull request from
/// `pull.json` (or none), and its name from `repo.json` (or `a/b`). Any
/// other answer is empty. Each call is logged to `log`.
fn fake_gh(dir: &Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let d = dir.display();
    let script = format!(
        r#"#!/bin/sh
echo "$*" >> {d}/log
case "$*" in
  *"GET user") echo '{{"login":"me"}}' ;;
  *"GET repos/a/b/pulls/1") cat {d}/pull.json ;;
  *"GET repos/a/b/pulls "*)
    if [ -e {d}/pull.json ]; then printf '['; cat {d}/pull.json; printf ']'; else echo '[]'; fi ;;
  *"GET repos/a/b") cat {d}/repo.json 2>/dev/null || echo '{{"full_name":"a/b"}}' ;;
  *) echo '[]' ;;
esac
"#
    );
    let gh = dir.join("gh");
    std::fs::write(&gh, script).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    gh
}

fn repository(name: &str) -> Repository {
    Repository {
        name: name.into(),
        path: format!("/home/me/{name}"),
        automatic: false,
        setup_command: String::new(),
        network_access: false,
        round_limit: 4,
        approval_minutes: 0,
        keep_updated: false,
        provider: Default::default(),
        dev_container: Default::default(),
        sandbox: false,
        former: vec![],
    }
}

async fn watching(dir: &Path, names: &[&str]) -> Arc<Engine> {
    let e = Engine::new(dir.join("state"), |_| {}).unwrap();
    e.update(|s| {
        s.settings.buddy.brief = false;
        for name in names {
            s.settings.repositories.push(repository(name));
        }
    })
    .await;
    e
}

async fn poll(dir: &Path, e: &Engine) {
    crate::github::GH
        .scope(fake_gh(dir), e.poll())
        .await
        .unwrap();
}

fn calls(dir: &Path, wanted: &str) -> usize {
    std::fs::read_to_string(dir.join("log"))
        .unwrap_or_default()
        .lines()
        .filter(|l| l.ends_with(wanted) || l.contains(&format!("{wanted} ")))
        .count()
}

fn names(s: &Snapshot) -> Vec<String> {
    s.settings
        .repositories
        .iter()
        .map(|r| r.name.clone())
        .collect()
}

#[tokio::test]
async fn follows_a_repository_its_pull_requests_say_moved() {
    let d = tempfile::tempdir().unwrap();
    let e = watching(d.path(), &["a/b"]).await;
    let pull = serde_json::json!({
        "number": 1, "state": "open", "title": "Fix", "user": { "login": "dev" },
        "head": { "sha": "a".repeat(40), "ref": "fix", "repo": { "full_name": "c/D" } },
        "base": { "sha": "b".repeat(40), "ref": "main", "repo": { "full_name": "c/D" } },
        "author_association": "OWNER", "updated_at": "2026-11-13T00:00:00Z",
    });
    std::fs::write(d.path().join("pull.json"), pull.to_string()).unwrap();
    // Reviewed under the old name, with its history stored.
    let review = Review::new(&crate::types::tests::pr(), &Settings::default(), false);
    e.update(|s| s.reviews.push(review)).await;
    let stored = d.path().join("state/ledger/a/b");
    std::fs::create_dir_all(&stored).unwrap();
    std::fs::write(
        stored.join("1.json"),
        serde_json::json!({ "repo": "a/b" }).to_string(),
    )
    .unwrap();

    poll(d.path(), &e).await;
    let s = e.snapshot().await;
    assert_eq!(names(&s), ["c/D"]);
    assert_eq!(s.settings.repositories[0].path, "/home/me/a/b");
    assert_eq!(s.prs.len(), 1);
    assert_eq!(
        (s.prs[0].id.as_str(), s.prs[0].repo.as_str()),
        ("c/d#1", "c/D")
    );
    assert_eq!(s.reviews[0].pr_id, "c/d#1");
    assert!(s.ledger.version > 0);
    let moved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(d.path().join("state/ledger/c/d/1.json")).unwrap())
            .unwrap();
    assert_eq!(moved["repo"], "c/d");
    assert!(!stored.exists());
    let note = s.buddy_feed.notes.last().unwrap();
    assert_eq!(note.repo.as_deref(), Some("c/D"));
    assert!(note.text.contains("a/b moved to c/D"), "{}", note.text);
    // Saved that way.
    let saved = std::fs::read_to_string(d.path().join("state/state.json")).unwrap();
    assert!(saved.contains("\"c/D\""));

    // The next sync reads it under its new name, and doesn't ask its name.
    poll(d.path(), &e).await;
    assert_eq!(calls(d.path(), "GET repos/c/D/pulls"), 1);
    assert_eq!(calls(d.path(), "GET repos/c/D"), 0);
    assert_eq!(names(&e.snapshot().await), ["c/D"]);
}

#[tokio::test]
async fn asks_github_where_a_repository_is_once_a_day() {
    let d = tempfile::tempdir().unwrap();
    let e = watching(d.path(), &["a/b", "a/b-two"]).await;
    poll(d.path(), &e).await;
    poll(d.path(), &e).await;
    // Asked once, and nothing moved.
    assert_eq!(calls(d.path(), "GET repos/a/b"), 1);
    assert_eq!(calls(d.path(), "GET repos/a/b-two"), 1);
    assert_eq!(names(&e.snapshot().await), ["a/b", "a/b-two"]);
    // A day later, it has.
    std::fs::write(d.path().join("repo.json"), r#"{"full_name":"c/d"}"#).unwrap();
    e.names_read.lock().unwrap().insert(
        "a/b".into(),
        std::time::Instant::now() - super::repo_moves::NAME_EVERY,
    );
    poll(d.path(), &e).await;
    assert_eq!(calls(d.path(), "GET repos/a/b"), 2);
    assert_eq!(names(&e.snapshot().await), ["c/d", "a/b-two"]);
}

#[tokio::test]
async fn stops_watching_the_old_name_when_it_watches_the_new_one() {
    let d = tempfile::tempdir().unwrap();
    let e = watching(d.path(), &["a/b", "c/d"]).await;
    std::fs::write(d.path().join("repo.json"), r#"{"full_name":"C/D"}"#).unwrap();
    poll(d.path(), &e).await;
    let s = e.snapshot().await;
    assert_eq!(names(&s), ["c/d"]);
    let note = s.buddy_feed.notes.last().unwrap();
    assert!(note.text.contains("you already watch C/D"), "{}", note.text);
}

#[tokio::test]
async fn waits_while_orbit_reads_github_in_the_background() {
    let d = tempfile::tempdir().unwrap();
    let e = watching(d.path(), &["a/b"]).await;
    std::fs::write(d.path().join("repo.json"), r#"{"full_name":"c/d"}"#).unwrap();
    e.watching_ci.store(true, Ordering::SeqCst);
    poll(d.path(), &e).await;
    assert_eq!(names(&e.snapshot().await), ["a/b"]);
    // Once the read is done, the next sync follows it.
    e.watching_ci.store(false, Ordering::SeqCst);
    poll(d.path(), &e).await;
    assert_eq!(calls(d.path(), "GET repos/a/b"), 2);
    assert_eq!(names(&e.snapshot().await), ["c/d"]);
    assert!(!e.watching_ci.load(Ordering::SeqCst));
}

#[tokio::test]
async fn a_name_github_doesnt_give_moves_nothing() {
    let d = tempfile::tempdir().unwrap();
    let e = watching(d.path(), &["a/b"]).await;
    for answer in [r#"{"full_name":"../x"}"#, r#"[]"#, r#"{"full_name":"a/B"}"#] {
        std::fs::write(d.path().join("repo.json"), answer).unwrap();
        e.names_read.lock().unwrap().clear();
        poll(d.path(), &e).await;
        assert_eq!(names(&e.snapshot().await), ["a/b"], "{answer}");
    }
}
