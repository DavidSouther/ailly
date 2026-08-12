//! Shared conformance suite for [`SessionIndex`](crate::index::aggregate::SessionIndex) backends.
//!
//! Every backend must reconcile the same three-harness fixture tree and expose
//! identical list, summary, page, search, and stable-refresh behavior.

use crate::index::aggregate::SessionIndex;
use crate::index::reconcile::IndexRefresh;
use crate::index::{ListSessionsQuery, PageQuery, SearchQuery};
use crate::loader::DiscoveryRoots;
use crate::model::{Harness, SourceValue};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;

fn unique_temp_dir(label: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "ailly-index-conformance-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ))
}

pub(crate) fn build_three_harness_home(label: &str) -> PathBuf {
    let home = unique_temp_dir(label);
    let _ = fs::remove_dir_all(&home);
    let claude_dir = home.join(".claude/sessions");
    let codex_dir = home.join(".codex/sessions");
    let pi_dir = home.join(".pi/agent/sessions");
    fs::create_dir_all(&claude_dir).expect("create claude sessions root");
    fs::create_dir_all(&codex_dir).expect("create codex sessions root");
    fs::create_dir_all(&pi_dir).expect("create pi sessions root");
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

pub(crate) fn assert_indexes_all_harnesses(index: &dyn SessionIndex, label: &str) {
    let home = build_three_harness_home(label);
    let roots = DiscoveryRoots {
        home: Some(home.clone()),
        pi_session_roots: Vec::new(),
    };
    let mut reports = Vec::new();
    index
        .refresh_with_progress(
            IndexRefresh {
                roots: roots.clone(),
            },
            &mut |progress| reports.push(progress),
        )
        .expect("first reconcile");
    assert_eq!(
        reports.len(),
        3,
        "reconcile must report progress once per discovered source, got {reports:?}"
    );
    assert!(
        reports.iter().all(|report| report.total == 3),
        "every progress report must carry the discovered total, got {reports:?}"
    );
    assert_eq!(
        reports
            .iter()
            .map(|report| report.indexed)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "progress must advance monotonically so the UI can stream partial results"
    );

    let listed = index
        .list_sessions(ListSessionsQuery {
            limit: 50,
            offset: 0,
            harness: None,
            project: None,
        })
        .expect("list sessions")
        .items;
    assert_eq!(
        listed.len(),
        3,
        "expected one indexed session per harness, got {:?}",
        listed.iter().map(|item| item.harness).collect::<Vec<_>>()
    );
    for harness in [Harness::ClaudeCode, Harness::Codex, Harness::Pi] {
        assert!(
            listed.iter().any(|item| item.harness == harness),
            "missing harness {harness:?}"
        );
    }

    let codex_id = listed
        .iter()
        .find(|item| item.harness == Harness::Codex)
        .expect("Codex session listed")
        .id
        .clone();
    let codex_summary = index.get_session_summary(&codex_id).expect("Codex summary");
    assert!(
        matches!(codex_summary.token_total, SourceValue::Absent),
        "Codex token_total must stay Absent, got {:?}",
        codex_summary.token_total
    );
    assert_eq!(codex_summary.token_recorded_count, 0);

    let page = index
        .get_event_page(
            &codex_id,
            PageQuery {
                limit: 100,
                offset: 0,
            },
        )
        .expect("Codex event page");
    assert!(!page.events.is_empty());
    assert!(page.events.iter().all(|event| {
        matches!(event.token_usage, SourceValue::Absent)
            && !event.source.path.is_empty()
            && event.source.line > 0
    }));

    let hits = index
        .search_index(SearchQuery {
            query: "hello".into(),
            limit: 20,
            offset: 0,
        })
        .expect("search")
        .items;
    assert!(
        !hits.is_empty(),
        "expected FTS hit for fixture turn text 'hello'"
    );
    assert!(hits
        .iter()
        .any(|hit| { listed.iter().any(|session| session.id == hit.session_id) }));

    let first_ids: HashSet<String> = listed.iter().map(|item| item.id.clone()).collect();
    index
        .refresh(IndexRefresh { roots })
        .expect("second reconcile");
    let listed_again = index
        .list_sessions(ListSessionsQuery {
            limit: 50,
            offset: 0,
            harness: None,
            project: None,
        })
        .expect("list after stable refresh")
        .items;
    let second_ids: HashSet<String> = listed_again.iter().map(|item| item.id.clone()).collect();
    assert_eq!(first_ids, second_ids);
    assert_eq!(listed_again.len(), 3);

    let _ = fs::remove_dir_all(&home);
}

/// Instantiates the shared conformance tests for one [`SessionIndex`] backend.
macro_rules! conformance_tests {
    ($factory:expr, $label:literal) => {
        #[test]
        fn indexes_discovered_sessions_for_query_search_and_stable_refresh() {
            let index = $factory();
            crate::index::conformance::assert_indexes_all_harnesses(&index, $label);
        }
    };
}
pub(crate) use conformance_tests;
