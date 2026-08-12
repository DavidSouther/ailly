//! Feature test for Task 3: rebuildable SQLite index.
//!
//! User story: Given discovered Claude/Codex/Pi fixtures, when the index
//! reconciles and the investigator lists, pages, summarizes, and searches,
//! then all three harnesses are queryable without fabricating absent
//! metadata, and a second reconcile without file changes stays stable.
//!
//! Expected red: `ailly_analyzer_lib::index` does not exist until Task 3 lands.

use ailly_analyzer_lib::index::{
    open_index, IndexRefresh, ListSessionsQuery, PageQuery, SearchQuery, SessionIndex,
};
use ailly_analyzer_lib::loader::DiscoveryRoots;
use ailly_analyzer_lib::model::{Harness, SourceValue};
use std::collections::HashSet;
use std::path::PathBuf;
use std::{env, fs};

/// Builds a throwaway `$HOME` tree holding one fixture per harness under the
/// default roots, keyed by process id so parallel test runs never collide.
fn build_fake_home() -> PathBuf {
    let home = env::temp_dir().join(format!("ailly-index-feature-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let claude_dir = home.join(".claude/sessions");
    let codex_dir = home.join(".codex/sessions");
    let pi_dir = home.join(".pi/agent/sessions");
    fs::create_dir_all(&claude_dir).expect("create fake claude sessions root");
    fs::create_dir_all(&codex_dir).expect("create fake codex sessions root");
    fs::create_dir_all(&pi_dir).expect("create fake pi sessions root");
    fs::copy(
        "tests/fixtures/claude.jsonl",
        claude_dir.join("session.jsonl"),
    )
    .expect("copy claude fixture");
    fs::copy(
        "tests/fixtures/codex.jsonl",
        codex_dir.join("session.jsonl"),
    )
    .expect("copy codex fixture");
    fs::copy("tests/fixtures/pi.jsonl", pi_dir.join("session.jsonl")).expect("copy pi fixture");
    home
}

#[test]
fn indexes_discovered_sessions_for_query_search_and_stable_refresh() {
    let home = build_fake_home();
    let index_path = env::temp_dir().join(format!(
        "ailly-index-feature-db-{}.sqlite",
        std::process::id()
    ));
    let _ = fs::remove_file(&index_path);

    let roots = DiscoveryRoots {
        home: Some(home.clone()),
        pi_session_roots: Vec::new(),
    };

    let index = open_index(&index_path).expect("open injectable index path");
    index
        .refresh(IndexRefresh {
            roots: roots.clone(),
        })
        .expect("first reconcile indexes discovered fixtures");

    let listed = index
        .list_sessions(ListSessionsQuery {
            limit: 50,
            offset: 0,
            harness: None,
            project: None,
        })
        .expect("list sessions after refresh");
    assert_eq!(
        listed.items.len(),
        3,
        "expected one indexed session per harness, got {:?}",
        listed
            .items
            .iter()
            .map(|item| item.harness)
            .collect::<Vec<_>>()
    );
    for harness in [Harness::ClaudeCode, Harness::Codex, Harness::Pi] {
        assert!(
            listed.items.iter().any(|item| item.harness == harness),
            "missing harness {harness:?} from session list"
        );
    }

    // Codex fixtures record no usage: summary must stay Absent, not Recorded(0).
    let codex_id = listed
        .items
        .iter()
        .find(|item| item.harness == Harness::Codex)
        .expect("Codex session listed")
        .id
        .clone();
    let codex_summary = index.get_session_summary(&codex_id).expect("Codex summary");
    assert!(
        matches!(codex_summary.token_total, SourceValue::Absent),
        "Codex summary token_total must be Absent, not a fabricated Recorded value; got {:?}",
        codex_summary.token_total
    );
    assert_eq!(
        codex_summary.token_recorded_count, 0,
        "Codex summary must report zero recorded token events"
    );

    let page = index
        .get_event_page(
            &codex_id,
            PageQuery {
                limit: 100,
                offset: 0,
            },
        )
        .expect("Codex event page");
    assert!(
        !page.events.is_empty(),
        "indexed Codex session should expose events in source order"
    );
    assert!(
        page.events.iter().all(|event| {
            matches!(event.token_usage, SourceValue::Absent)
                && !event.source.path.is_empty()
                && event.source.line > 0
        }),
        "every Codex event must keep Absent token_usage and round-trip provenance path/line"
    );

    // "hello" appears as recorded turn text in the fixtures; "session" does not.
    let hits = index
        .search_index(SearchQuery {
            query: "hello".into(),
            limit: 20,
            offset: 0,
        })
        .expect("FTS search over small indexed text set");
    assert!(
        !hits.items.is_empty(),
        "expected at least one FTS hit for fixture turn text 'hello'"
    );
    assert!(
        hits.items.iter().any(|hit| listed
            .items
            .iter()
            .any(|session| session.id == hit.session_id)),
        "search hits must resolve to indexed session ids"
    );

    let first_ids: HashSet<String> = listed.items.iter().map(|item| item.id.clone()).collect();
    index
        .refresh(IndexRefresh { roots })
        .expect("second reconcile with unchanged files");
    let listed_again = index
        .list_sessions(ListSessionsQuery {
            limit: 50,
            offset: 0,
            harness: None,
            project: None,
        })
        .expect("list after stable refresh");
    let second_ids: HashSet<String> = listed_again
        .items
        .iter()
        .map(|item| item.id.clone())
        .collect();
    assert_eq!(
        first_ids, second_ids,
        "repeat refresh without source changes must keep the same session id set"
    );
    assert_eq!(
        listed_again.items.len(),
        3,
        "repeat refresh must not duplicate or drop sessions"
    );

    let _ = fs::remove_file(&index_path);
    let _ = fs::remove_dir_all(&home);
}
