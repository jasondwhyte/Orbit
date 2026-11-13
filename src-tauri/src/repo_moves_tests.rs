use super::*;
use crate::{
    ops::{recover, Item, Posted, Thread},
    types::{pr_id, tests::pr, Repository, Review, Settings, Snapshot},
};
use serde_json::json;

fn repository(name: &str, path: &str) -> Repository {
    Repository {
        name: name.into(),
        path: path.into(),
        automatic: true,
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

#[test]
fn renames_the_repository_where_it_stands_on_its_own() {
    let rename = |text: &str| rename_text(text, "Old/Repo", "New/Repo");
    assert_eq!(rename("Old/Repo").as_deref(), Some("New/Repo"));
    // Any case counts; lower case, as ids and keys keep it, stays lower.
    assert_eq!(rename("OLD/REPO").as_deref(), Some("New/Repo"));
    assert_eq!(rename("old/repo#12").as_deref(), Some("new/repo#12"));
    assert_eq!(rename("pr:old/repo#12").as_deref(), Some("pr:new/repo#12"));
    assert_eq!(
        rename("tag:old/repo:v1.0").as_deref(),
        Some("tag:new/repo:v1.0")
    );
    assert_eq!(
        rename("Fix Old/Repo, then old/repo.").as_deref(),
        Some("Fix New/Repo, then new/repo.")
    );
    assert_eq!(
        rename("`Old/Repo` builds").as_deref(),
        Some("`New/Repo` builds")
    );
    // Not inside a longer name, nor in a path or a link.
    for stays in [
        "Old/Repo-two",
        "Old/Repo.js",
        "Old/Repository",
        "my-Old/Repo",
        "Bold/Repo",
        "/home/me/Old/Repo",
        "C:\\code\\Old/Repo",
        "~/Old/Repo",
        "Old/Repo/src/main.rs",
        "https://github.com/Old/Repo/pull/1",
        "",
    ] {
        assert_eq!(rename(stays), None, "{stays}");
    }
    assert_eq!(rename_text("old/repo", "", "new/repo"), None);
}

#[test]
fn renames_keys_and_keeps_what_was_written_under_the_new_name() {
    let mut v = json!({
        "old/repo": "kept",
        "new/repo": "newer",
        "old/repo#3": ["old/repo#3", 4, null],
        "other/repo": { "pr": "old/repo#5", "path": "/x/old/repo" },
    });
    assert!(rename_value(&mut v, "old/repo", "new/repo"));
    assert_eq!(
        v,
        json!({
            "new/repo": "newer",
            "new/repo#3": ["new/repo#3", 4, null],
            "other/repo": { "pr": "new/repo#5", "path": "/x/old/repo" },
        })
    );
    assert!(!rename_value(&mut v, "old/repo", "new/repo"));
}

/// A state where the repository `a/b` stands in many places: what Orbit
/// shows and posts by it, and a path that mustn't change.
fn watched() -> Snapshot {
    let mut s = Snapshot::default();
    s.settings
        .repositories
        .push(repository("A/b", "/home/me/A/b"));
    s.settings
        .repositories
        .push(repository("a/b-two", "/home/me/a/b-two"));
    let mut p = pr();
    p.repo = "A/b".into();
    p.id = pr_id("A/b", 1);
    p.base_repo = "c/D".into();
    let mut review = Review::new(&p, &Settings::default(), false);
    review.report = "Looked at A/b at its latest commit.".into();
    review.extraction_problem = Some("cut short".into());
    s.reviews.push(review);
    s.prs.push(p);
    s.ops.push(Thread {
        key: "pr:a/b#1".into(),
        repo: "A/b".into(),
        pr_id: Some("a/b#1".into()),
        comment_id: Some(7),
        items: vec![Item {
            number: 1,
            failing: true,
            since: "abc".into(),
            ..Default::default()
        }],
        posted: concat!(
            "Failing in A/b.\n<!-- orbit-ops thread=pr:a/b#1 ",
            r#"{"repo":"A/b","items":[{"id":"I1","failing":true,"since":"abc"}]} -->"#
        )
        .into(),
        ..Default::default()
    });
    s.ops.push(Thread {
        key: "pr:a/b#2".into(),
        repo: "A/b".into(),
        pr_id: Some("a/b#2".into()),
        ..Default::default()
    });
    s.ops.push(Thread {
        key: "pr:a/b#3".into(),
        repo: "A/b".into(),
        unsure: true,
        ..Default::default()
    });
    s.password_store = Some("No password store".into());
    s
}

#[test]
fn renames_the_repository_everywhere_in_the_state() {
    let s = watched();
    let out = renamed(&s, "A/b", "c/D").unwrap();
    let names: Vec<&str> = out
        .settings
        .repositories
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(names, ["c/D", "a/b-two"]);
    // The folder on this computer didn't move.
    assert_eq!(out.settings.repositories[0].path, "/home/me/A/b");
    // Its clone may still name the old address.
    assert_eq!(out.settings.repositories[0].former, ["A/b"]);
    assert!(out.settings.repositories[1].former.is_empty());
    assert_eq!(out.prs[0].repo, "c/D");
    assert_eq!(out.prs[0].id, "c/d#1");
    assert_eq!(out.prs[0].id, pr_id("c/D", 1));
    assert_eq!(out.reviews[0].pr_id, "c/d#1");
    assert_eq!(out.reviews[0].report, "Looked at c/D at its latest commit.");
    // Kept, though not saved.
    assert_eq!(
        out.reviews[0].extraction_problem.as_deref(),
        Some("cut short")
    );
    assert_eq!(out.password_store, s.password_store);
    // Threads on GitHub, or perhaps on it, keep their hidden line.
    let keys: Vec<(&str, &str)> = out
        .ops
        .iter()
        .map(|t| (t.key.as_str(), t.marker_key()))
        .collect();
    assert_eq!(
        keys,
        [
            ("pr:c/d#1", "pr:a/b#1"),
            ("pr:c/d#2", "pr:c/d#2"),
            ("pr:c/d#3", "pr:a/b#3"),
        ]
    );
    assert_eq!(out.ops[0].tag(), crate::ops::tag("pr:a/b#1"));
    assert_eq!(out.ops[0].repo, "c/D");
    // Its copy of what GitHub has stays as GitHub has it.
    assert_eq!(out.ops[0].posted, s.ops[0].posted);
    // Nothing else holds the old name on its own.
    let mut saved = serde_json::to_value(&out).unwrap();
    for t in saved["ops"].as_array_mut().unwrap() {
        let t = t.as_object_mut().unwrap();
        t.remove("marked");
        t.remove("posted");
    }
    for r in saved["settings"]["repositories"].as_array_mut().unwrap() {
        r.as_object_mut().unwrap().remove("former");
    }
    assert!(!rename_value(&mut saved, "A/b", "c/D"));
    // Moving again keeps the first hidden line, and both earlier names.
    let again = renamed(&out, "c/D", "e/f").unwrap();
    assert_eq!(again.settings.repositories[0].name, "e/f");
    assert_eq!(again.settings.repositories[0].former, ["A/b", "c/D"]);
    assert_eq!(again.ops[0].key, "pr:e/f#1");
    assert_eq!(again.ops[0].marker_key(), "pr:a/b#1");
    assert_eq!(again.ops[1].marker_key(), "pr:e/f#2");
    // And moving back drops the name it has again.
    let back = renamed(&again, "e/f", "a/B").unwrap();
    assert_eq!(back.settings.repositories[0].former, ["c/D", "e/f"]);
    // Moving away from the first name once more still keeps its first
    // hidden line, though it names the repository again.
    let away = renamed(&back, "a/B", "g/h").unwrap();
    let keys: Vec<(&str, &str)> = away
        .ops
        .iter()
        .map(|t| (t.key.as_str(), t.marker_key()))
        .collect();
    assert_eq!(
        keys,
        [
            ("pr:g/h#1", "pr:a/b#1"),
            ("pr:g/h#2", "pr:g/h#2"),
            ("pr:g/h#3", "pr:a/b#3"),
        ]
    );
    assert_eq!(away.ops[0].posted, s.ops[0].posted);
    // So Orbit still reads its comment on GitHub as its own.
    let copy = Posted {
        id: 7,
        number: None,
        url: String::new(),
        updated_at: "2026-11-13T08:00:00Z".into(),
        body: s.ops[0].posted.clone(),
    };
    let mut t = away.ops[0].clone();
    recover(&mut t, &copy);
    assert_eq!(t.items[0].heard_from, "2026-11-13T08:00:00Z");
    let mut unmarked = away.ops[0].clone();
    unmarked.marked = None;
    recover(&mut unmarked, &copy);
    assert_eq!(unmarked.items[0].heard_from, "");
}

#[test]
fn refuses_a_name_github_wouldnt_give() {
    assert!(renamed(&watched(), "A/b", "../x").is_err());
}

#[test]
fn moves_the_stored_history_to_the_new_name() {
    let d = tempfile::tempdir().unwrap();
    let old = d.path().join("ledger/a/b");
    std::fs::create_dir_all(old.join("writeups")).unwrap();
    let stored = |n: u32| json!({ "repo": "A/b", "pr": { "number": n, "html_url": "https://github.com/A/b/pull/1" } });
    std::fs::write(old.join("1.json"), stored(1).to_string()).unwrap();
    std::fs::write(old.join("2.json"), stored(2).to_string()).unwrap();
    std::fs::write(old.join("2.json.tmp"), "half").unwrap();
    std::fs::write(
        old.join("writeups/1.json"),
        json!({ "repo": "a/b", "number": 1 }).to_string(),
    )
    .unwrap();
    // Read under the new name since it moved.
    let new = d.path().join("ledger/c/d");
    std::fs::create_dir_all(&new).unwrap();
    std::fs::write(
        new.join("2.json"),
        json!({ "repo": "c/D", "fresh": true }).to_string(),
    )
    .unwrap();
    // Another repository of the same owner stays.
    std::fs::create_dir_all(d.path().join("ledger/a/other")).unwrap();
    move_history(d.path(), "A/b", "c/D").unwrap();
    let read = |p: &str| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(new.join(p)).unwrap()).unwrap()
    };
    assert_eq!(read("1.json")["repo"], "c/D");
    // Links stay: GitHub sends them on.
    assert_eq!(
        read("1.json")["pr"]["html_url"],
        "https://github.com/A/b/pull/1"
    );
    assert_eq!(read("2.json"), json!({ "repo": "c/D", "fresh": true }));
    assert_eq!(read("writeups/1.json")["repo"], "c/d");
    assert!(!new.join("2.json.tmp").exists());
    assert!(!old.exists());
    assert!(d.path().join("ledger/a/other").exists());
    // Nothing stored under the old name: nothing to move.
    move_history(d.path(), "A/b", "c/D").unwrap();
    move_history(d.path(), "x/y", "c/D").unwrap();
    assert_eq!(read("1.json")["repo"], "c/D");
}

#[test]
fn a_lone_owner_folder_goes_with_its_last_repository() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("ledger/a/b")).unwrap();
    move_history(d.path(), "a/b", "c/d").unwrap();
    assert!(!d.path().join("ledger/a").exists());
    assert!(d.path().join("ledger/c/d").is_dir());
}
