//! Feature test for the built-in `web.search` and `web.fetch` tool slice.
//!
//! User story (narrative):
//!
//! An operator wiring up the conversation tool palette constructs a
//! `HashMapRegistry` containing both `web.search` (backed by a recorded
//! `MapSearchBackend`) and `web.fetch` (backed by a `reqwest::Client` aimed
//! at a `mockito::Server`). Each tool is wrapped in a `PermissionGated`
//! decorator pairing the tool's per-tool `Classifier`
//! (`WebSearchClassifier`, `WebFetchClassifier`) with an `AllowAllBackend`
//! so the gate forwards every call to the inner tool. A workflow whose two
//! turns issue `USE web.search WITH {...}` and `USE web.fetch WITH {...}`
//! is driven through a `Generator` over the resulting two-turn
//! `Conversation`. The operator expects the search round-trip's
//! `ToolResult` to carry a JSON-serialized `Vec<SearchResult>` whose
//! recorded entries appear verbatim, the fetch round-trip's `ToolResult`
//! to carry the body the mock server returned for the requested URL, both
//! pairs to round-trip onto the per-turn TOML files as `tool_call` and
//! `tool_result` entries naming `web.search` and `web.fetch`, and the
//! gated path to be byte-identical to the un-gated path on `Allow`
//! (recorded by calling each underlying tool directly before the registry
//! round-trip and asserting the same payload appears in the persisted
//! tool_result entry).

use std::io::Write;
use std::sync::Arc;

use futures::StreamExt;

use ailly::content::Conversation;
use ailly::engine::{
    Generator, HashMapRegistry, Noop, Settings, StopReason, ToolRegistry, TurnEvent,
};
use ailly::knowledge::skills::NullSkillRepository;
use ailly::permissions::{AllowAllBackend, PermissionBackend, PermissionGated};
use ailly::tools::web::{
    MapSearchBackend, SearchBackend, SearchResult, WebFetch, WebFetchClassifier, WebSearch,
    WebSearchClassifier,
};

use rig::message::ToolResultContent;
use rig::tool::ToolDyn;
use vfs::{MemoryFS, VfsPath};

#[tokio::test]
async fn web_search_and_web_fetch_round_trip_through_gated_registry_and_files() {
    let query = "ddd aggregate boundaries";
    let recorded = vec![
        SearchResult {
            title: "Aggregates carve consistency boundaries".to_string(),
            url: "https://example.test/aggregates".to_string(),
            snippet: "An aggregate root protects its invariants.".to_string(),
        },
        SearchResult {
            title: "DDD Reference".to_string(),
            url: "https://example.test/ddd-ref".to_string(),
            snippet: "Aggregates cluster entities and value objects.".to_string(),
        },
    ];
    let search_backend: Arc<dyn SearchBackend> =
        Arc::new(MapSearchBackend::new([(query, recorded.clone())]));

    let mut mock_server = mockito::Server::new_async().await;
    let body = "<!doctype html>\n<title>fixture</title>\n<p>hello, web.fetch</p>\n";
    let mock = mock_server
        .mock("GET", "/page")
        .with_status(200)
        .with_header("content-type", "text/html; charset=utf-8")
        .with_body(body)
        .expect(2)
        .create_async()
        .await;
    let mock_url = format!("{}/page", mock_server.url());

    let bare_search: Arc<dyn ToolDyn> = Arc::new(WebSearch::new(Arc::clone(&search_backend)));
    let search_args = serde_json::json!({ "query": query, "max_results": 2 }).to_string();
    let bare_search_result = bare_search
        .call(search_args.clone())
        .await
        .expect("bare WebSearch::call succeeds against MapSearchBackend hit");
    let parsed_bare: Vec<SearchResult> = serde_json::from_str(&bare_search_result)
        .expect("bare web.search result deserializes as Vec<SearchResult>");
    assert_eq!(
        parsed_bare, recorded,
        "bare WebSearch returns the recorded fixture verbatim"
    );

    let bare_fetch: Arc<dyn ToolDyn> = Arc::new(WebFetch::new().expect("WebFetch::new builds"));
    let fetch_args = serde_json::json!({ "url": mock_url.clone() }).to_string();
    let bare_fetch_result = bare_fetch
        .call(fetch_args.clone())
        .await
        .expect("bare WebFetch::call succeeds against mockito server");
    assert_eq!(
        bare_fetch_result, body,
        "bare WebFetch returns the mock body decoded verbatim"
    );

    let permission_backend: Arc<dyn PermissionBackend> = Arc::new(AllowAllBackend);

    let gated_search: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
        WebSearch::NAME,
        Arc::new(WebSearch::new(Arc::clone(&search_backend))) as Arc<dyn ToolDyn>,
        Arc::new(WebSearchClassifier),
        Arc::clone(&permission_backend),
    ));

    let gated_fetch: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
        WebFetch::NAME,
        Arc::new(WebFetch::new().expect("WebFetch::new builds")) as Arc<dyn ToolDyn>,
        Arc::new(WebFetchClassifier),
        Arc::clone(&permission_backend),
    ));

    let mut registry = HashMapRegistry::default();
    registry.insert(WebSearch::NAME, gated_search);
    registry.insert(WebFetch::NAME, gated_fetch);
    let registry: Arc<dyn ToolRegistry> = Arc::new(registry);

    let search_prompt = format!(
        "prompt = 'USE web.search WITH {{\"query\":\"{query}\",\"max_results\":2}}'\ntools = [\"web.search\"]\n"
    );
    let fetch_prompt = format!(
        "prompt = 'USE web.fetch WITH {{\"url\":\"{mock_url}\"}}'\ntools = [\"web.fetch\"]\n"
    );

    let fs: VfsPath = VfsPath::new(MemoryFS::new());
    let root = fs.join("root").expect("join root");
    root.create_dir().expect("create root dir");
    write!(
        root.join("01.toml")
            .expect("join 01.toml")
            .create_file()
            .expect("create 01.toml"),
        "{search_prompt}"
    )
    .expect("write 01.toml");
    write!(
        root.join("02.toml")
            .expect("join 02.toml")
            .create_file()
            .expect("create 02.toml"),
        "{fetch_prompt}"
    )
    .expect("write 02.toml");

    let conversation = Conversation::load(root.clone(), &NullSkillRepository)
        .await
        .expect("load two-turn conversation");

    let engine = Arc::new(Noop::default());
    let generator =
        Generator::new(conversation, engine, Settings::default()).with_registry(registry);

    let events: Vec<TurnEvent> = generator.run().collect().await;

    let tool_calls: Vec<&TurnEvent> = events
        .iter()
        .filter(|e| matches!(e, TurnEvent::ToolCall { .. }))
        .collect();
    let tool_results: Vec<&TurnEvent> = events
        .iter()
        .filter(|e| matches!(e, TurnEvent::ToolResult { .. }))
        .collect();
    assert_eq!(
        tool_calls.len(),
        2,
        "two tool calls (one per turn): {events:#?}"
    );
    assert_eq!(
        tool_results.len(),
        2,
        "two tool results (one per turn): {events:#?}"
    );

    let mut search_result_text: Option<String> = None;
    let mut fetch_result_text: Option<String> = None;

    for (call_ev, result_ev) in tool_calls.iter().zip(tool_results.iter()) {
        let TurnEvent::ToolCall { call, .. } = call_ev else {
            unreachable!()
        };
        let TurnEvent::ToolResult { result, .. } = result_ev else {
            unreachable!()
        };
        assert_eq!(
            result.id, call.id,
            "tool result id pairs with originating tool call id"
        );
        let result_body = match result.content.first() {
            ToolResultContent::Text(t) => t.text.clone(),
            other => panic!("expected ToolResultContent::Text, got {other:?}"),
        };
        match call.function.name.as_str() {
            n if n == WebSearch::NAME => search_result_text = Some(result_body),
            n if n == WebFetch::NAME => fetch_result_text = Some(result_body),
            other => panic!("unexpected tool name in round-trip: {other}"),
        }
    }

    let search_result_text =
        search_result_text.expect("web.search round-trip observed in the event stream");
    let fetch_result_text =
        fetch_result_text.expect("web.fetch round-trip observed in the event stream");

    for entry in &recorded {
        assert!(
            search_result_text.contains(&entry.url),
            "web.search ToolResult should carry recorded url {url}; got {search_result_text}",
            url = entry.url
        );
        assert!(
            search_result_text.contains(&entry.title),
            "web.search ToolResult should carry recorded title {title}; got {search_result_text}",
            title = entry.title
        );
        assert!(
            search_result_text.contains(&entry.snippet),
            "web.search ToolResult should carry recorded snippet {snippet}; got {search_result_text}",
            snippet = entry.snippet
        );
    }

    assert!(
        fetch_result_text.contains("hello, web.fetch"),
        "web.fetch ToolResult should carry the mock body verbatim; got {fetch_result_text}"
    );

    let finished = events
        .iter()
        .rev()
        .find(|e| matches!(e, TurnEvent::Finished { .. }))
        .expect("at least one Finished event in the stream");
    let TurnEvent::Finished { stop_reason, .. } = finished else {
        unreachable!()
    };
    assert!(
        matches!(stop_reason, StopReason::EndTurn),
        "two single-tool round-trips finish normally inside the default tool-turn budget"
    );

    for (path, name) in [
        ("root/01.toml", WebSearch::NAME),
        ("root/02.toml", WebFetch::NAME),
    ] {
        let written = fs
            .join(path)
            .expect("join turn file")
            .read_to_string()
            .unwrap_or_else(|e| panic!("read {path}: {e}"));
        let call_pos = written
            .find(r#"role = "tool_call""#)
            .unwrap_or_else(|| panic!("tool_call entry written to {path}: {written}"));
        let result_pos = written
            .find(r#"role = "tool_result""#)
            .unwrap_or_else(|| panic!("tool_result entry written to {path}: {written}"));
        assert!(
            call_pos < result_pos,
            "tool call precedes tool result in {path}: {written}"
        );
        assert!(
            written.contains(&format!(r#"name = "{name}""#)),
            "tool_call entry in {path} should record the {name} tool name; got: {written}"
        );
    }

    assert!(
        search_result_text.contains(&bare_search_result),
        "gated web.search ToolResult should embed the bare WebSearch JSON verbatim on Allow; bare={bare_search_result}; gated={search_result_text}"
    );
    assert!(
        fetch_result_text.contains(body),
        "gated web.fetch ToolResult should embed the bare WebFetch body verbatim on Allow; bare={body}; gated={fetch_result_text}"
    );

    mock.assert_async().await;
}
