# Implementation Plan: Feature 2 — Web tools

**Design:** [design.md](design.md) | **Project plan:** [../plan.md](../plan.md) | **Feature 1:** [../feature-1-harness/design.md](../feature-1-harness/design.md)

**User story:** A model issuing a `web_search` query or a `web_fetch` URL through the Feature 1 `ToolExecutor` loop receives a real `tool_result`, with each tool's logic exercised by its own network-free unit test.

**No single feature test (by design).** Feature 2 has no one end-to-end feature test of its own; each tool's correctness is its own mocked unit test (feature-2 design §"Metrics", project design Features table row 2). Every step therefore writes its own unit test red-first, then implements to green. "Green" per step = the GREEN DEFINITION in the task brief: `mise run check` (cargo check --all-features --all-targets) exits 0; `mise run lint` (clippy -D warnings) exits 0; `cargo nextest run --all-features --all-targets --no-fail-fast` passes except the 3 pre-existing baseline e2e tests (`e2e_delegate_52`, `e2e_patterns_eval`, `eval_insurance_claim`); the Feature 1 feature test `tests/tool_loop.rs` stays green; `mise run format` clean. No live network call in any test or in CI.

**Steps:**
- [ ] Step 1: `SearchProvider` seam + `BraveSearchProvider` adapter; promote `reqwest`; register `mod web`
- [ ] Step 2: `WebSearch` `ToolExecutor` impl (shape hits, missing-query and provider-error → `is_error`)
- [ ] Step 3: `e2e/research/context/tools/web_search.json` fixture + round-trip test
- [ ] Step 4: `Fetcher` seam + `ReqwestFetcher` adapter
- [ ] Step 5: `WebFetch` `ToolExecutor` impl (shape body+status, non-2xx and missing-url → `is_error`)
- [x] Step 6: `e2e/research/context/tools/web_fetch.json` fixture + round-trip test

**Files (whole feature):**
- Create: `src/knowledge/tools/web.rs` — both ports + adapters + executors + `#[cfg(test)]` fakes and tests (steps 1, 2, 4, 5).
- Modify: `src/knowledge/tools/mod.rs` — add `mod web;` registration (step 1). `web` is `pub(crate)`/private as needed; the executors are the public surface a future registry wires (project design §6 follow-on), so register `pub mod web;`.
- Modify: `Cargo.toml` — the ONLY manifest change: promote `reqwest` from transitive (via rig) to a direct dep `reqwest = { version = "0.13", default-features = false, features = ["rustls-tls", "json"] }` (step 1). Lockfile-neutral: `reqwest 0.13.3` is already locked (Cargo.lock:2092) and already pulls `rustls`/`hyper-rustls`/`serde_json`, so the resolved set is unchanged.
- Create: `e2e/research/context/tools/web_search.json` (step 3) and `e2e/research/context/tools/web_fetch.json` (step 6) — owned by Feature 2; Feature 3 only references them and never re-creates them.

**Ordering rationale.** The two seams (search, fetch) are fully independent — they share only the `ContentBlock` destructure/build idiom inherited from `NoopToolExecutor` (`mod.rs:93`). The plan does search end-to-end (steps 1–3) then fetch end-to-end (steps 4–6) so each half lands as a coherent vertical slice and a refactor pass (extracting shared `ok_result`/`error_result` helpers) has both call sites present by step 5. Every step compiles and is green standalone: the production adapters (`BraveSearchProvider`, `ReqwestFetcher`) are compiled-but-uncovered from the moment they land — the documented `TokioScriptRunner` posture (`script_runner.rs:111-117`: "a subprocess adapter has no seam below it to mock") — and are never executed under `nextest`. The tests cover only the executors, driven by hand-rolled fakes injected through the port trait.

---

## Step 1: `SearchProvider` seam + `BraveSearchProvider` adapter; promote `reqwest`; register `mod web`

**Enables:** the `web_search` mock seam — the port the step-2 unit test injects a `FakeSearchProvider` through. No executor yet, so this step's RED is the fake itself proving the trait is object-safe and drivable.

Mirror `src/knowledge/script_runner.rs`'s `ScriptRunner` / `TokioScriptRunner` trio (and `assertions.rs:2114`'s `FakeScriptRunner`). Promote `reqwest` in `Cargo.toml` (the single manifest change; lockfile-neutral). Add `pub mod web;` to `src/knowledge/tools/mod.rs`. In `web.rs`, introduce:

```rust
pub struct SearchQuery { pub query: String, pub count: u8 }
pub struct SearchHit { pub title: String, pub url: String, pub snippet: String }
pub struct SearchResults { pub hits: Vec<SearchHit> }

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("search request failed: {0}")] Request(String),
    #[error("search response was not the expected shape: {0}")] Decode(String),
}

#[async_trait::async_trait]
pub trait SearchProvider: Send + Sync {
    /// # Errors
    /// [`SearchError`] on request/decode failure. Zero hits is `Ok`, not an error.
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, SearchError>;
}

pub struct BraveSearchProvider { client: reqwest::Client, api_key: String }
// from_env() reads BRAVE_SEARCH_API_KEY lazily (only when invoked, never at construct).
// search(): single authenticated GET https://api.search.brave.com/res/v1/web/search
//   ?q=&count=, X-Subscription-Token header; decode web.results[] -> SearchHit{title,url,description->snippet}.
//   reqwest::Error lowered to String at the boundary so the port never leaks reqwest.
```

`SearchError` carries `String`, not a wrapped `reqwest::Error`, so the port signature and the fake stay reqwest-free (mirrors `ScriptError` carrying `std::io::Error`, not a tokio handle). Suppress any unavoidable clippy lint (e.g. `clippy::missing_errors_doc` is satisfied by the `# Errors` doc; `module_name_repetitions` on `SearchProvider` if it fires) per-call-site with `#[expect(clippy::NAME, reason = "...")]`.

**RED → GREEN:** the step's test is `fake_search_provider_drives_through_the_port` — construct `FakeSearchProvider` (the `Mutex<VecDeque<Result<SearchResults, SearchError>>>` + `Mutex<Vec<SearchQuery>>` shape copied from `FakeScriptRunner`), call `.search(...)` through `&dyn SearchProvider`, assert it returns the canned `SearchResults` and recorded the `SearchQuery`. (This is the minimal behavioral test that forces the trait + data types to exist and be object-safe; it is subsumed/extended by step 2's executor tests, so keep it tiny — one assertion on the round-trip — to avoid redundancy with step 2 per the parsimony rule.) `BraveSearchProvider` is compiled-but-uncovered.

**Commit:** `feat(feat2): web_search provider seam + brave adapter`

---

## Step 2: `WebSearch` `ToolExecutor` impl

**Enables:** the three `web_search` unit assertions — `web_search_shapes_hits_into_tool_result`, `web_search_missing_query_is_error_result`, `web_search_provider_error_is_error_result` (feature-2 design §"Unit-test fake strategy" tests 1–3).

```rust
pub struct WebSearch { provider: Box<dyn SearchProvider> }

#[async_trait::async_trait]
impl crate::knowledge::tools::ToolExecutor for WebSearch {
    async fn execute(&self, call: &ContentBlock) -> Result<ContentBlock, ToolError> {
        let ContentBlock::ToolUse { id, name: _, input } = call else {
            return Err(ToolError::NotAToolUse);          // identical to NoopToolExecutor (mod.rs:94)
        };
        let Some(query) = input.get("query").and_then(serde_yaml_ng::Value::as_str) else {
            return Ok(error_result(id, "web_search requires a string `query`"));
        };
        let count = input.get("count").and_then(serde_yaml_ng::Value::as_u64)
            .and_then(|n| u8::try_from(n).ok()).unwrap_or(DEFAULT_RESULT_COUNT);
        match self.provider.search(SearchQuery { query: query.to_owned(), count }).await {
            Ok(results) => Ok(ok_result(id, &render_hits(&results))),
            Err(err)    => Ok(error_result(id, &err.to_string())),
        }
    }
}
```

`render_hits` formats `SearchResults` into a compact model-readable block (one `title — url\nsnippet` per hit). `ok_result`/`error_result` are two small `ContentBlock::ToolResult { tool_use_id: id.clone(), content: text.into(), is_error }` constructors (`ok_result` → `is_error: None`; `error_result` → `is_error: Some(true)`). `DEFAULT_RESULT_COUNT` is a small const (matches the `count` default in `web_search.json`, step 3).

**Error mapping (feature-2 design §"Error mapping"):** non-`ToolUse` block → `Err(ToolError::NotAToolUse)` (harness invariant, never model-recoverable); missing/non-string `query` → `Ok(is_error: Some(true))` (model sent a bad argument — recoverable, fed back as a `tool_result`); provider error → `Ok(is_error: Some(true))` (transient; the message rides through). `ToolError` stays reserved for the structural non-tool_use case only.

**RED → GREEN, three tests** driving `WebSearch { provider: Box::new(FakeSearchProvider::new(vec![...])) }`:
1. `web_search_shapes_hits_into_tool_result` — fake returns two hits; assert the `ToolResult` echoes the call `id`, `is_error: None`, and the text contains the **exact** title/url projection of a known hit (one-character-bug check: assert the exact `title — url` line, not just non-empty).
2. `web_search_missing_query_is_error_result` — `input` lacks `query`; assert `is_error: Some(true)` AND the provider was not called (recorded `calls` empty).
3. `web_search_provider_error_is_error_result` — fake returns `Err(SearchError::Request("brave 503"))`; assert `is_error: Some(true)` and the message text carries `503`.

The non-tool_use rejection is already proven generically by `NoopToolExecutor`'s `execute_errors_on_non_tool_use_block` (`mod.rs:182`) over identical destructure code; not re-tested per tool (parsimony). `BraveSearchProvider` remains uncovered.

**Commit:** `feat(feat2): web_search tool executor`

---

## Step 3: `web_search.json` fixture + round-trip test

**Enables:** the contract Feature 3's evals assert on (`must_call_tool: web_search`, `tool_call_order: [web_search, web_fetch]` — project plan §"Feature 3"). The `name` string `web_search` must match the tool the executor registers under; the `input_schema` must match the args step 2's `execute` reads (`query`, `count`).

Create `e2e/research/context/tools/web_search.json`, byte-shaped like `e2e/insurance-claim/context/tools/lookup_policy.json`:

```json
{
  "name": "web_search",
  "description": "Search the web and return a ranked list of result titles, URLs, and snippets.",
  "input_schema": {
    "type": "object",
    "properties": {
      "query": { "type": "string", "description": "The search query." },
      "count": { "type": "integer", "description": "Max results to return.", "default": 5 }
    },
    "required": ["query"]
  }
}
```

**RED → GREEN:** `web_search_json_round_trips_into_tool_definition` in `web.rs`'s test module, mirroring `conversation.rs:820`'s `tool_definition_round_trips_lookup_policy_fixture`. `include_str!("../../../e2e/research/context/tools/web_search.json")`, deserialize into `crate::content::conversation::ToolDefinition` via `serde_yaml_ng::from_str` (JSON is a YAML subset — the exact path the assembly resolver uses), assert `name == "web_search"` and `input_schema["required"]` contains `"query"`. A future schema edit that breaks the contract fails loud here.

(Confirm the relative path from `src/knowledge/tools/web.rs` to the fixture before writing the `include_str!` — `src/` and `e2e/` are siblings, so it is `../../../e2e/research/context/tools/web_search.json`; verify against the live tree at build time rather than trusting the count.)

**Commit:** `feat(feat2): web_search tool-definition fixture`

---

## Step 4: `Fetcher` seam + `ReqwestFetcher` adapter

**Enables:** the `web_fetch` mock seam — the port the step-5 unit test injects a `FakeFetcher` through. Same `script_runner.rs` precedent as step 1.

In `web.rs`, below the search seam:

```rust
pub struct FetchRequest { pub url: String }
pub struct FetchResponse {
    pub status: u16,
    pub body: String,                 // raw body text for v1; HTML->text extraction deferred
    pub content_type: Option<String>, // lower-cased Content-Type if present
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("fetch request failed: {0}")] Request(String),
    #[error("invalid url {url:?}: {reason}")] InvalidUrl { url: String, reason: String },
}

#[async_trait::async_trait]
pub trait Fetcher: Send + Sync {
    /// # Errors
    /// [`FetchError`] on a malformed URL or transport failure. A non-2xx HTTP
    /// status is NOT an error — it is an `Ok(FetchResponse)` whose `status` the
    /// tool reports (mirrors `ScriptRunner` treating a non-zero exit as data).
    async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, FetchError>;
}

pub struct ReqwestFetcher { client: reqwest::Client }
// fetch(): validate the URL, GET it, lower the response into
//   FetchResponse { status, body (raw text), content_type (lower-cased) }.
//   reqwest::Error lowered to a String at the boundary so the port stays reqwest-free.
```

`FetchError` carries `String`, not `reqwest::Error` (same boundary rule as `SearchError`). Non-2xx is `Ok`, not `Err` — the tool decides `is_error` from the status (step 5), exactly as `check_script` reads `ExitDisposition` from a runner that returned `Ok`.

**RED → GREEN:** `fake_fetcher_drives_through_the_port` — tiny round-trip test driving `FakeFetcher` (same `Mutex<VecDeque<Result<FetchResponse, FetchError>>>` + `Mutex<Vec<FetchRequest>>` shape) through `&dyn Fetcher`; assert the canned `FetchResponse` returns and the `FetchRequest` was recorded. Keep it minimal — step 5's executor tests subsume it. `ReqwestFetcher` compiled-but-uncovered.

**Commit:** `feat(feat2): web_fetch fetcher seam + reqwest adapter`

---

## Step 5: `WebFetch` `ToolExecutor` impl

**Enables:** the three `web_fetch` unit assertions — `web_fetch_shapes_body_into_tool_result`, `web_fetch_non_2xx_is_error_result`, `web_fetch_missing_url_is_error_result` (feature-2 design §"Unit-test fake strategy" tests 4–6).

Same skeleton as `WebSearch::execute`, sharing the `ok_result`/`error_result` constructors (refactor them to free fns above both executors now that both call sites exist):

```rust
pub struct WebFetch { fetcher: Box<dyn Fetcher> }

#[async_trait::async_trait]
impl crate::knowledge::tools::ToolExecutor for WebFetch {
    async fn execute(&self, call: &ContentBlock) -> Result<ContentBlock, ToolError> {
        let ContentBlock::ToolUse { id, name: _, input } = call else {
            return Err(ToolError::NotAToolUse);
        };
        let Some(url) = input.get("url").and_then(serde_yaml_ng::Value::as_str) else {
            return Ok(error_result(id, "web_fetch requires a string `url`"));
        };
        match self.fetcher.fetch(FetchRequest { url: url.to_owned() }).await {
            Ok(resp) if (200..300).contains(&resp.status) =>
                Ok(ok_result(id, &render_response(&resp))),
            Ok(resp) => Ok(error_result(id, &render_response(&resp))), // non-2xx is data → is_error
            Err(err) => Ok(error_result(id, &err.to_string())),
        }
    }
}
```

`render_response` projects `status` (+ `content_type` annotation) and the body into the result text so a downstream model can tell HTML from JSON from plain text.

**Error mapping:** non-`ToolUse` → `Err(ToolError::NotAToolUse)`; missing/non-string `url` → `is_error: Some(true)`; non-2xx status → `is_error: Some(true)` (status reported in the text); `FetchError` → `is_error: Some(true)`.

**RED → GREEN, three tests** driving `WebFetch { fetcher: Box::new(FakeFetcher::new(vec![...])) }`:
4. `web_fetch_shapes_body_into_tool_result` — fake returns `FetchResponse { status: 200, body: "<page>", content_type: Some("text/html".into()) }`; assert the result text carries the body AND the status, `is_error: None`.
5. `web_fetch_non_2xx_is_error_result` — fake returns `status: 404`; assert `is_error: Some(true)` and the text reports `404`.
6. `web_fetch_missing_url_is_error_result` — `input` lacks `url`; assert `is_error: Some(true)` AND the fetcher was not called (recorded `calls` empty).

`ReqwestFetcher` remains uncovered. **Refactor (green-only):** hoist `ok_result`/`error_result` to shared free fns now both executors use them — `developer:refactor`, tests stay green, no new behavior.

**Commit:** `feat(feat2): web_fetch tool executor`

---

## Step 6: `web_fetch.json` fixture + round-trip test

**Enables:** the second half of the Feature 3 contract (`must_call_tool: web_fetch`, `tool_call_order: [web_search, web_fetch]`). `name` must be `web_fetch`; `input_schema` must match the `url` arg step 5 reads.

Create `e2e/research/context/tools/web_fetch.json`:

```json
{
  "name": "web_fetch",
  "description": "Fetch the contents of a URL and return the response body as text.",
  "input_schema": {
    "type": "object",
    "properties": {
      "url": { "type": "string", "description": "The absolute URL to fetch." }
    },
    "required": ["url"]
  }
}
```

**RED → GREEN:** `web_fetch_json_round_trips_into_tool_definition`, mirroring step 3 — `include_str!` the fixture, deserialize into `ToolDefinition`, assert `name == "web_fetch"` and `input_schema["required"]` contains `"url"`.

**Commit:** `feat(feat2): web_fetch tool-definition fixture`

---

## Self-Review

**Spec coverage** — every item the feature-2 design and project plan §"Feature 2" enumerate has a step: the `SearchProvider` seam + `BraveSearchProvider` (step 1), `WebSearch::execute` with its three error/shape tests (step 2), `web_search.json` + round-trip (step 3), the `Fetcher` seam + `ReqwestFetcher` (step 4), `WebFetch::execute` with its three tests (step 5), `web_fetch.json` + round-trip (step 6). The single manifest change (promote `reqwest`) and the `mod web;` registration ride in step 1. All six unit tests from the design's fake-strategy list (tests 1–6) are placed.

**Step count** — 6 steps, within the 3–7 ceiling with one slot of slack. The project plan sketched 3–5 for Feature 2; this adds one step by separating each seam's port (steps 1, 4) from its executor (steps 2, 5) so the production adapter and its fake land green before the executor logic that consumes them — each remains one clean `developer:red-green-refactor` cycle. No step exceeds a single cycle; no return-to-design is triggered.

**Parsimony** — one `#[cfg(test)]` module in `web.rs` (the ceiling for the file), behavior-focused, no per-method test classes. The non-tool_use rejection is tested once generically by `NoopToolExecutor` (`mod.rs:182`) and not duplicated per web tool. The two seam round-trip tests (steps 1, 4) are kept to a single assertion each because the executor tests subsume them.

**No live network** — every test injects a hand-rolled fake through the port trait; `BraveSearchProvider`/`ReqwestFetcher` are compiled-but-uncovered (the documented `TokioScriptRunner` posture). No HTTP-mock crate and no HTML parser are added. CI makes zero live calls.

**Out of scope (not touched)** — the 3 pre-existing baseline e2e tests; the Feature 1 run loop / `tests/tool_loop.rs` (Feature 2 only adds files); the `e2e/research/` project skeleton, assembly, prompts, evals, and `ci.sh` (Feature 3 owns these — Feature 2 authors only the two `context/tools/*.json` fixtures). HTML→text extraction, a live integration test for the adapters, tool-call retry/recovery, and a real executor registry are deferred (feature-2 design §"Deferred").
