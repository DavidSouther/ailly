//! Verifies shell-derived accesses survive loading, indexing, SQLite storage,
//! and event-page readback with provenance and ambiguity intact.

use ailly_analyzer_lib::index::{
    open_index, IndexRefresh, ListSessionsQuery, PageQuery, SessionIndex,
};
use ailly_analyzer_lib::loader::DiscoveryRoots;
use ailly_analyzer_lib::model::{FileReference, Harness, SourceValue};
use std::path::PathBuf;
use std::{env, fs};

const CLAUDE_SESSION: &str = concat!(
    r#"{"type":"user","uuid":"u1","sessionId":"shell-access-claude","cwd":"/work/app","timestamp":"2026-08-20T15:00:00Z","message":{"role":"user","content":"regenerate the env file"}}"#,
    "\n",
    r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"shell-access-claude","cwd":"/work/app","message":{"role":"assistant","content":[{"type":"tool_use","id":"call-1","name":"Bash","input":{"command":"cat config/base.yml >> build/out.env"}},{"type":"tool_use","id":"call-2","name":"Bash","input":{"command":"sed -n '1,220p' src/lib.rs"}},{"type":"tool_use","id":"call-3","name":"Bash","input":{"command":"cat logs/*.txt"}},{"type":"tool_use","id":"call-4","name":"Bash","input":{"command":"ls ."}}]}}"#,
    "\n",
);

const CODEX_SESSION: &str = concat!(
    r#"{"type":"session_meta","payload":{"id":"shell-access-codex","cwd":"/work/app"}}"#,
    "\n",
    r#"{"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"exec_command","arguments":"{\"command\":\"rm -f build/stale.env\"}"}}"#,
    "\n",
);

/// Use a process-specific home so parallel tests do not collide.
fn build_fake_home() -> PathBuf {
    let home = env::temp_dir().join(format!("ailly-shell-access-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    let claude = home.join(".claude/sessions");
    let codex = home.join(".codex/sessions");
    fs::create_dir_all(&claude).expect("create a claude sessions root");
    fs::create_dir_all(&codex).expect("create a codex sessions root");
    fs::write(claude.join("session.jsonl"), CLAUDE_SESSION).expect("write the claude session");
    fs::write(codex.join("session.jsonl"), CODEX_SESSION).expect("write the codex session");
    home
}

fn label(value: &SourceValue<String>) -> Option<&str> {
    match value {
        SourceValue::Recorded(recorded) => Some(recorded.as_str()),
        _ => None,
    }
}

type Rendered<'a> = (
    &'a str,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
);

fn rendered(files: &[FileReference]) -> Vec<Rendered<'_>> {
    files
        .iter()
        .map(|file| {
            (
                file.path.as_str(),
                label(&file.target),
                label(&file.operation),
                label(&file.provenance),
                label(&file.ambiguity),
            )
        })
        .collect()
}

#[test]
fn indexing_a_recorded_command_stores_the_files_it_attempted() {
    let home = build_fake_home();
    let index_path = env::temp_dir().join(format!(
        "ailly-shell-access-db-{}.sqlite",
        std::process::id()
    ));
    let _ = fs::remove_file(&index_path);

    let index = open_index(&index_path).expect("open an index at an injected path");
    index
        .refresh(IndexRefresh {
            roots: DiscoveryRoots {
                home: Some(home.clone()),
                pi_session_roots: Vec::new(),
            },
        })
        .expect("index the written sessions");

    let listed = index
        .list_sessions(ListSessionsQuery {
            limit: 50,
            offset: 0,
            harness: None,
            project: None,
        })
        .expect("list the indexed sessions");

    let files_for = |harness: Harness| -> Vec<FileReference> {
        let session = listed
            .items
            .iter()
            .find(|item| item.harness == harness)
            .unwrap_or_else(|| panic!("no indexed {harness:?} session"));
        index
            .get_event_page(
                &session.id,
                PageQuery {
                    limit: 100,
                    offset: 0,
                },
            )
            .expect("page the session's events")
            .events
            .into_iter()
            .filter_map(|event| match event.files {
                SourceValue::Recorded(files) => Some(files),
                _ => None,
            })
            .flatten()
            .collect()
    };

    // In the order the commands named them, each labelled as shell evidence
    // rather than as a path the harness recorded. The relative operands stay
    // relative: the session's `/work/app` is context, not a root to resolve
    // against.
    let claude = files_for(Harness::ClaudeCode);
    assert_eq!(
        rendered(&claude),
        [
            (
                "config/base.yml",
                Some("file"),
                Some("read"),
                Some("shell"),
                None
            ),
            (
                "build/out.env",
                Some("file"),
                Some("write"),
                Some("shell"),
                None
            ),
            (
                "src/lib.rs",
                Some("file"),
                Some("read"),
                Some("shell"),
                None
            ),
            (
                "logs/*.txt",
                Some("file"),
                Some("read"),
                Some("shell"),
                Some("glob not expanded")
            ),
            (".", Some("directory"), Some("read"), Some("shell"), None),
        ]
    );

    assert_eq!(
        rendered(&files_for(Harness::Codex)),
        [(
            "build/stale.env",
            Some("file"),
            Some("delete"),
            Some("shell"),
            None
        )]
    );

    let _ = fs::remove_file(&index_path);
    let _ = fs::remove_dir_all(&home);
}
