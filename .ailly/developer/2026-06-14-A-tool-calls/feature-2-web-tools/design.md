# Feature 2 Design: Web tools

**Project:** [../design.md](../design.md) | **Plan:** [../plan.md](../plan.md) | **Feature 1:** [../feature-1-harness/design.md](../feature-1-harness/design.md)
**Type:** Feature (the two web tools; Feature 2 of 3) | **Status:** Review
**Depends on:** Feature 1's Step-0 contract — `ToolExecutor` (`src/knowledge/tools/mod.rs:35`), `ToolError` (`:23`), `ContentBlock::ToolUse`/`ToolResult` (`src/content/conversation.rs:281`/`:286`), `ToolDefinition` (`conversation.rs:69`).

## Problem Statement

The harness (Feature 1) routes a model's `tool_use` block through a `ToolExecutor` and appends the returned `tool_result`, but the only executor in the tree is `NoopToolExecutor`, which replays scripted strings. No executor actually *does* anything against the outside world. This feature delivers the first two real tools — `web_search` and `web_fetch` — each implementing the `ToolExecutor` contract, so a model can issue a search query or fetch a URL and receive a real `tool_result`.

Two facts from the project plan bound the work to a deliberately thin slice:

1. **The e2e never calls these tools live.** Feature 3's `e2e/research/` and the insurance-claim gate drive the loop through `NoopToolExecutor` with scripted replies (project plan §"Feature 3"; research.md "noop run — no live web API"). The real `web_search`/`web_fetch` code is therefore exercised **only** by their own mocked unit tests (project plan §"Feature 2": "each with its own unit test exercising the tool logic directly"). CI makes **zero** live network calls.
2. **No in-repo HTTP/search seam to copy.** Verified — `src/` has no `web*.rs`/`http*.rs` module (project plan §"Feature 2"). The closest precedent is `src/knowledge/script_runner.rs`, a port-plus-adapter seam over `tokio::process`, itself modelled on `EngineProvider`/`NoopEngine`. This feature builds the web seam by mirroring that precedent, not by inventing a new shape.

The research "(tbd)" — *which* search provider backs `web_search` — is resolved here (Specification §"web_search provider, resolved").

The job, then, is: pick a mockable seam for each tool so the unit tests inject a fake and the production adapter stays a minimal-but-correct vertical slice; author the two `ToolDefinition` JSON fixtures Feature 3 declares; and keep the whole feature off the network at test time.

## Prior Art (in-repo patterns this feature mirrors)

- **`ScriptRunner` port + `TokioScriptRunner` adapter + hand-rolled `FakeScriptRunner`** (`src/knowledge/script_runner.rs:89`/`:117`, fake at `assertions.rs:2114`). The canonical in-repo "I/O behind a trait so the caller unit-tests against a fake" seam. `check_script` (`assertions.rs:497`) takes `runner: &dyn ScriptRunner` and is unit-tested with a `Mutex<VecDeque<canned outputs>>` fake — never spawning a real process. The production adapter's own correctness is left to an integration test that touches the real resource (`tests/eval_script.rs` spawns real `python3`); the comment at `script_runner.rs:111-117` states the rule outright: *"a subprocess adapter has no seam below it to mock."* The web tools adopt this verbatim: a `SearchProvider`/`Fetcher` port, a `reqwest` production adapter, a hand-rolled fake in the unit test.
- **`EngineProvider` / `NoopEngine`** (`src/engine/engine.rs:179`). The original of the pattern `ScriptRunner` copies: a behavior trait with a real network adapter (`RigEngine`) and a deterministic scriptable adapter (`NoopEngine`, a `Mutex<VecDeque<ScriptEntry>>`). `NoopToolExecutor` (Feature 1) already follows it on the executor axis.
- **`NoopToolExecutor`'s `ToolUse` destructure + `ToolResult` build** (`src/knowledge/tools/mod.rs:93-117`). Each web tool's `execute` reuses this exact shape: `let ContentBlock::ToolUse { id, name, input } = call else { return Err(ToolError::NotAToolUse) };` … `Ok(ContentBlock::ToolResult { tool_use_id: id.clone(), content: <body>.into(), is_error: <…> })`. The destructure pattern and the `ToolResult` field set are fixed by Feature 1; this feature only fills the middle (run the search / fetch the URL).
- **`yaml_value_to_json`** (`src/content/conversation.rs:88`). Already the bridge from a tool's `serde_yaml_ng::Value` to `serde_json::Value`. `execute` reads its arguments out of `input: serde_yaml_ng::Value`; it indexes the value directly (`input["query"]`, `input["url"]`) the way the existing tests index `tool.input_schema["type"]` (`conversation.rs:835`), so no new bridge is needed to read a string argument.
- **The insurance-claim tool JSONs** (`e2e/insurance-claim/context/tools/lookup_policy.json`): `{ name, description, input_schema: { type: object, properties, required } }`. `web_search.json` / `web_fetch.json` mirror this byte-shape exactly so the e2e assembly's `kind: tools` resolver (Feature 1) parses them with the same `serde_yaml_ng` path.

## Metrics — what "green" means

- **Two unit tests pass, both network-free.** `web_search`'s unit test asserts it reads the `query` argument, calls the injected `SearchProvider`, and shapes the provider's results into a `ToolResult` echoing the call `id`. `web_fetch`'s unit test asserts it reads the `url` argument, calls the injected `Fetcher`, and shapes the body into a `ToolResult`. Both inject a hand-rolled fake (the `FakeScriptRunner` shape); neither opens a socket. A test that takes more than a few milliseconds means an unmocked client leaked in — a defect, per the project's mocking convention.
- **Error paths covered.** Each tool's `execute` has at least one test for the failure mode it owns: a missing/non-string argument (the tool builds an `is_error: Some(true)` result rather than panicking) and a provider/fetcher error (surfaced as a `ToolError` the run loop maps to `RunError::Tool`, *or* as an `is_error` result — the choice is fixed in Specification §"Error mapping").
- **Fixtures parse.** `e2e/research/context/tools/web_search.json` and `web_fetch.json` deserialize into `ToolDefinition` via the same `serde_yaml_ng::from_str` path the assembly resolver uses (a fixture round-trip test, mirroring `conversation.rs:830`'s `lookup_policy.json` round-trip, guards this).
- **No live API in CI.** `mise run check` / `mise run test` / `mise run lint` green; `mise run format` clean. The `reqwest`-backed production adapters are compiled but never executed under `cargo nextest` — they have no covering test by design (same posture as `TokioScriptRunner`, whose realness is proven only by the gated integration test, not the unit suite). The Feature 1 feature test `tests/tool_loop.rs` stays green (this feature adds files; it does not touch the run loop).

## Specification

### Module layout

One new file, `src/knowledge/tools/web.rs`, registered with `mod web;` in `src/knowledge/tools/mod.rs`. It holds, top to bottom:

1. `SearchProvider` port + `SearchQuery`/`SearchResults` data + `BraveSearchProvider` production adapter.
2. `Fetcher` port + `FetchRequest`/`FetchResponse` data + `ReqwestFetcher` production adapter.
3. `WebSearch` and `WebFetch` executors, each owning a boxed port.
4. `#[cfg(test)]` module: the two hand-rolled fakes and the four-to-six unit tests.

The two ports and two executors are independent; they share only the `ContentBlock` destructure/build idiom. Keeping them in one file (rather than `web/search.rs` + `web/fetch.rs`) matches `script_runner.rs`, which keeps its port, adapter, and data in one file; the file is small (well under the 800-line guide) and splits only if a tool grows.

### The mockable seam (both tools)

Each tool depends on a **port trait**, never on `reqwest` directly. The tool struct owns a `Box<dyn Port + Send + Sync>`; its constructor takes the port. Production code constructs the tool with the real adapter; the unit test constructs it with a fake. This is the `script_runner.rs` seam applied twice.

```rust
// ── web_search seam ───────────────────────────────────────────────
/// One web search request. The complete call description; the provider
/// only executes it. Mirrors `ScriptSpec`'s "executor builds it, runner
/// runs it" split.
pub struct SearchQuery {
    pub query: String,
    /// Cap on results requested; the tool sets a small default.
    pub count: u8,
}

/// One result row, provider-normalised. A flat shape so the production
/// adapter maps any provider's JSON into it and the tool never sees
/// provider-specific fields.
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub struct SearchResults {
    pub hits: Vec<SearchHit>,
}

/// Structural failure of the search itself (network, auth, bad JSON),
/// distinct from a search that ran and returned zero hits.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("search request failed: {0}")]
    Request(String),
    #[error("search response was not the expected shape: {0}")]
    Decode(String),
}

/// The sole seam between `WebSearch::execute` and the network, so the
/// unit test drives the tool against a hand-rolled fake. Mirrors
/// `ScriptRunner`.
#[async_trait::async_trait]
pub trait SearchProvider: Send + Sync {
    /// # Errors
    /// Returns [`SearchError`] when the request fails or the response
    /// cannot be decoded. Zero hits is `Ok(SearchResults { hits: [] })`,
    /// not an error.
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, SearchError>;
}

// ── web_fetch seam ────────────────────────────────────────────────
pub struct FetchRequest {
    pub url: String,
}

pub struct FetchResponse {
    pub status: u16,
    /// Decoded body text. v1 returns the raw response body; HTML→text
    /// extraction is deferred (Summary).
    pub body: String,
    /// `Content-Type` header, lower-cased, if present. Lets the tool note
    /// "(html)" in the result without parsing it.
    pub content_type: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("fetch request failed: {0}")]
    Request(String),
    #[error("invalid url {url:?}: {reason}")]
    InvalidUrl { url: String, reason: String },
}

/// The sole seam between `WebFetch::execute` and the network. Mirrors
/// `ScriptRunner`.
#[async_trait::async_trait]
pub trait Fetcher: Send + Sync {
    /// # Errors
    /// Returns [`FetchError`] on a malformed URL or a transport failure.
    /// A non-2xx HTTP status is **not** an error — it is an
    /// `Ok(FetchResponse)` whose `status` the tool reports, mirroring how
    /// `ScriptRunner` treats a non-zero exit as data, not an error.
    async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, FetchError>;
}
```

The error enums carry `String` (not a wrapped `reqwest::Error`) so the port's signature does not leak `reqwest` into callers or the fake — exactly as `ScriptError` carries `std::io::Error` and not a tokio handle. The production adapter lowers `reqwest::Error` to a `String` at the boundary.

### web_search provider, resolved (research "(tbd)")

**Decision: a JSON HTTP search API behind the `SearchProvider` port, with the production adapter targeting the Brave Search API.** Rationale:

- **The provider choice is a production-adapter detail, fully isolated by the port.** Because the unit test injects a `FakeSearchProvider` and the e2e runs noop, the concrete provider is never exercised in CI. The choice therefore cannot break a test; it only fixes which real endpoint a live run would hit. This is the same containment `TokioScriptRunner` enjoys — its realness is invisible to the unit suite.
- **Brave Search API fits a minimal vertical slice**: a single authenticated `GET https://api.search.brave.com/res/v1/web/search?q=…&count=…` with an `X-Subscription-Token` header, returning JSON with `web.results[]` rows carrying `title`, `url`, `description`. One request, one JSON decode, a key from an env var (`BRAVE_SEARCH_API_KEY`). No SDK, no OAuth, no pagination for v1. It maps cleanly onto `SearchHit { title, url, snippet }`.
- **It needs only crates already (or near-already) in the tree**: `reqwest` (in Cargo.lock, see §"Crates"), `serde`/`serde_json` (direct deps) to decode the response, and `url`/query building via `reqwest`'s own `.query(&[…])`. No new search-SDK dependency.

The adapter reads the key once via `std::env::var("BRAVE_SEARCH_API_KEY")`; absent, `BraveSearchProvider::from_env()` returns a `SearchError::Request("BRAVE_SEARCH_API_KEY not set")` shape *only when invoked*, so merely constructing the tool in a no-search run never fails. (No live run happens in this project; this is the seam a future live run would use.)

A vendor-neutral note: the port is the contract, not Brave. Swapping to Serper, Tavily, or an internal endpoint is a new adapter implementing `SearchProvider` — zero change to `WebSearch::execute` or the tests. That is the whole point of resolving the "(tbd)" as *a seam over a provider* rather than as a hard provider dependency.

### web_fetch client, resolved

**Decision: `reqwest` behind the `Fetcher` port, returning the raw response body text.** `ReqwestFetcher` holds a `reqwest::Client`, parses/validates the URL, issues a `GET`, and lowers the response into `FetchResponse { status, body, content_type }`. v1 returns the body **verbatim** (no HTML-to-text extraction — deferred, Summary) because the e2e never fetches a live page and a real research run is usable with raw text; adding a parser now is unearned complexity (no HTML parser is in the tree — §"Crates"). The tool annotates the result with the status and content-type so a downstream model can tell HTML from JSON from plain text.

### Tool execute: `ToolUse(input) → ToolResult`

Each executor implements `ToolExecutor::execute` (Feature 1's trait). The body, in three steps, reusing the Feature 1 destructure idiom:

```rust
pub struct WebSearch {
    provider: Box<dyn SearchProvider>,
}

#[async_trait::async_trait]
impl ToolExecutor for WebSearch {
    async fn execute(&self, call: &ContentBlock) -> Result<ContentBlock, ToolError> {
        // 1. Destructure — identical to NoopToolExecutor (mod.rs:94).
        let ContentBlock::ToolUse { id, name: _, input } = call else {
            return Err(ToolError::NotAToolUse);
        };

        // 2. Read arguments out of `input: serde_yaml_ng::Value`. A missing
        //    or non-string `query` is a *tool-level* failure (the model
        //    sent a bad argument), surfaced as an is_error result, not a
        //    ToolError — see "Error mapping".
        let Some(query) = input.get("query").and_then(serde_yaml_ng::Value::as_str) else {
            return Ok(error_result(id, "web_search requires a string `query`"));
        };
        let count = input.get("count").and_then(serde_yaml_ng::Value::as_u64)
            .and_then(|n| u8::try_from(n).ok())
            .unwrap_or(DEFAULT_RESULT_COUNT);

        // 3. Call the port; shape the outcome into a ToolResult echoing `id`.
        match self.provider.search(SearchQuery { query: query.to_owned(), count }).await {
            Ok(results) => Ok(ok_result(id, render_hits(&results))),
            Err(err)    => Ok(error_result(id, &err.to_string())),
        }
    }
}
```

`render_hits` formats `SearchResults` into a compact, model-readable text block (one `title — url\nsnippet` per hit). `ok_result`/`error_result` are two small constructors over `ContentBlock::ToolResult { tool_use_id: id.clone(), content: text.into(), is_error }` — `ok_result` sets `is_error: None`, `error_result` sets `is_error: Some(true)`. `WebFetch::execute` is the same skeleton: read `url`, call `self.fetcher.fetch(...)`, render `status` + body into the result text, `is_error: Some(true)` on a `FetchError` or a non-2xx status.

### Error mapping (which failures are `ToolError` vs `is_error` result)

The seam is deliberate and matches the Anthropic loop shape (research.md [1]):

| Failure | Surfaced as | Why |
|---|---|---|
| Block is not a `ToolUse` | `Err(ToolError::NotAToolUse)` | A harness invariant violation, never a model-recoverable condition — identical to `NoopToolExecutor` (`mod.rs:95`). |
| Missing/wrong-typed argument (`query`/`url`) | `Ok(ToolResult { is_error: Some(true) })` | The *model* sent a bad argument; the standard agentic recovery is to feed the error back as a `tool_result` so the model can retry. A `ToolError` would abort the whole run — too harsh for a recoverable model mistake. |
| Network / provider / decode failure | `Ok(ToolResult { is_error: Some(true) })` | Same reasoning: the model may pick a different query/URL. Keeps a transient outage from killing the run. (The project's "tool-call error recovery / `is_error: true` retry loop" is deferred — research.md "What is deferred" — so v1 simply *emits* the `is_error` result; it does not itself retry.) |

`ToolError` is reserved for structural impossibilities (non-tool_use block). Everything data-driven becomes an `is_error` `ToolResult`. This mirrors `script_runner.rs`'s split exactly: `ScriptError` for structural runner failures, an `ExitDisposition` (data) for a checker that ran and failed.

### Unit-test fake strategy

Both fakes are hand-rolled in `web.rs`'s `#[cfg(test)]` module, copying `FakeScriptRunner` (`assertions.rs:2114`): a struct holding a `Mutex<VecDeque<canned outcome>>` plus a `Mutex<Vec<recorded request>>` for argument assertions.

```rust
#[cfg(test)]
struct FakeSearchProvider {
    canned: Mutex<VecDeque<Result<SearchResults, SearchError>>>,
    calls:  Mutex<Vec<SearchQuery>>,
}
#[async_trait::async_trait]
impl SearchProvider for FakeSearchProvider {
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, SearchError> {
        self.calls.lock().unwrap().push(query);
        self.canned.lock().unwrap().pop_front()
            .expect("FakeSearchProvider: a canned result is available")
    }
}
```

The tool under test is constructed as `WebSearch { provider: Box::new(FakeSearchProvider::new(vec![Ok(results)])) }`. Tests (the parsimonious ceiling — one unit-test module for the file, behaviour-focused, no per-method classes):

1. `web_search_shapes_hits_into_tool_result` — fake returns two hits; assert the `ToolResult` echoes the call `id`, `is_error: None`, and the text contains both hit titles/URLs. *(One-character-bug check: assert the exact text projection of a known hit, not just non-empty.)*
2. `web_search_missing_query_is_error_result` — `input` lacks `query`; assert `is_error: Some(true)` and the provider was **not** called (`calls` empty).
3. `web_search_provider_error_is_error_result` — fake returns `Err(SearchError::Request(...))`; assert `is_error: Some(true)` and the message rides through.
4. `web_fetch_shapes_body_into_tool_result` — `FakeFetcher` returns `FetchResponse { status: 200, body: "<page>", .. }`; assert the result text carries the body and the status, `is_error: None`.
5. `web_fetch_non_2xx_is_error_result` — fake returns `status: 404`; assert `is_error: Some(true)` and the status is reported.
6. `web_fetch_missing_url_is_error_result` — `input` lacks `url`; assert `is_error: Some(true)`, fetcher not called.

No test constructs `BraveSearchProvider` or `ReqwestFetcher`; the production adapters are compiled-but-uncovered, the documented `TokioScriptRunner` posture. The `non_tool_use` rejection is already proven generically by `NoopToolExecutor`'s test (`mod.rs:182`) and is identical code; re-testing it per tool would be redundant (parsimony), so it is asserted once via a shared helper if at all — not duplicated six times.

### Tool-definition fixtures (owned by this feature)

Two files under `e2e/research/context/tools/`, byte-shaped like `e2e/insurance-claim/context/tools/lookup_policy.json`:

```jsonc
// web_search.json
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

```jsonc
// web_fetch.json
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

The `name` strings (`web_search`, `web_fetch`) are the contract Feature 3's evals assert on (`must_call_tool: web_search`, `tool_call_order: [web_search, web_fetch]` — project plan §"Feature 3"); they must match the tool names the executors register under. The `input_schema` shapes match the arguments `execute` reads (`query`/`count`, `url`). A fixture round-trip test (`web.rs` or `conversation.rs`-adjacent, mirroring the `lookup_policy.json` round-trip at `conversation.rs:830`) deserialises each into `ToolDefinition` and asserts `name`/`input_schema["required"]`, so a future schema edit that breaks the contract fails loud.

### Crates

| Crate | In `Cargo.lock`? | Action |
|---|---|---|
| `reqwest` 0.13.3 | **Yes** — transitive via rig (`hyper-rustls`/`rustls` TLS already locked). **Not** a direct dependency. | **Add to `Cargo.toml`** as a direct dep, pinned to the locked line and matching its TLS backend: `reqwest = { version = "0.13", default-features = false, features = ["rustls-tls", "json"] }`. `default-features = false` + `rustls-tls` avoids pulling OpenSSL (rig already uses rustls); `json` enables `.json()` decode. This does **not** change the resolved version — 0.13.3 is already locked — only promotes it to a direct dependency. Noted explicitly per the "do not invent a crate/version; if a new dep is truly needed, add it to Cargo.toml explicitly and note it" instruction. |
| `serde` / `serde_json` | Yes — direct deps. | Reuse to derive/decode the Brave response struct. |
| `serde_yaml_ng` | Yes — direct dep. | Reuse to read `input["query"]`/`input["url"]` (already how the codebase indexes `serde_yaml_ng::Value`). |
| `async-trait` | Yes — direct dep. | Reuse for the two port traits and the `ToolExecutor` impls. |
| `thiserror` | Yes — direct dep. | Reuse for `SearchError`/`FetchError`. |
| `tokio` | Yes — direct dep (`rt`, `macros`, `time`). | `reqwest`'s async client runs on the existing tokio runtime; the unit tests use `#[tokio::test]` (already used at `mod.rs:137`). |
| HTML parser (`scraper`/`html2text`/…) | **No** — none in the tree. | **Not added.** v1 returns the raw body; HTML→text extraction is deferred (Summary). Avoids a heavy new dependency for a path the e2e never exercises. |
| HTTP-mock lib (`wiremock`/`mockito`/`httpmock`) | **No** — none in the tree. | **Not added.** The mock seam is the `SearchProvider`/`Fetcher` *trait* (hand-rolled fake), not a mock HTTP server — matching `FakeScriptRunner`. This is why the seam is a port, not a mocked `reqwest::Client`: no mock-server crate exists, and adding one to mock at the HTTP layer would be both a new dep and a worse seam (it would couple the test to `reqwest`'s wire behaviour instead of the tool's logic). |

The only Cargo manifest change is promoting `reqwest` to a direct dependency. No new crate is downloaded; the lockfile's resolved set is unchanged.

## Alternatives

**Seam placement — port trait (chosen) vs mock the HTTP client vs no seam.**

| Approach | Mock point | New deps | Verdict |
|---|---|---|---|
| **A: `SearchProvider`/`Fetcher` port + hand-rolled fake** | the tool's own trait dependency | none (reqwest promoted, already locked) | **chosen** — mirrors `ScriptRunner`/`FakeScriptRunner` (the established in-repo seam); tests assert *tool logic*, not wire behaviour; provider/client swap is a new adapter with zero test churn. |
| B: call `reqwest` directly in `execute`, mock with `wiremock`/`httpmock` | the HTTP wire | **adds** a mock-server crate | rejected — adds a dependency not in the tree, couples every unit test to `reqwest`'s request/response handling, and tests the network library rather than the tool. Breaks the "mock external deps behind a trait" convention `script_runner.rs` sets. |
| C: no seam — `execute` does real I/O, no unit test | n/a | none | rejected — the project plan mandates a per-tool unit test with mocked network (research.md "Do individual tools get individual tests? → Yes"; testing rule "mock ALL external deps"). A live test in CI violates the "NO live web/search calls" gate. |

**web_search provider — Brave (chosen) vs Serper/Tavily vs rig built-in vs DuckDuckGo HTML scrape.** rig has no search API (verified — no search crate in Cargo.lock). A DuckDuckGo HTML scrape needs an HTML parser (not in the tree) and is brittle. Serper/Tavily are equally valid JSON APIs; Brave is chosen for a clean single-GET + header-key shape, but **the decision is contained by the port** — any of them is a drop-in `SearchProvider` adapter, and since CI never calls it, the choice carries no test or correctness risk. The meaningful resolution of the research "(tbd)" is *"a `SearchProvider` seam over a JSON HTTP search API,"* with Brave as the concrete v1 adapter.

**web_fetch body — raw text (chosen) vs HTML→text extraction.** Extraction needs a new parser dependency for a path the e2e never runs and a live run tolerates as raw text. Deferred (Summary). Returning `status` + `content_type` alongside the body lets a later extraction pass slot in behind the same `Fetcher` port without changing `WebFetch::execute`.

**Build vs off-the-shelf tool executor.** No off-the-shelf web-tool crate produces Ailly's `ContentBlock::ToolResult` shape or fits the injected-`ToolExecutor` contract; the tools are ~40 lines each over the existing `reqwest`. Building is correct.

## Summary

- **Mockable seam (resolved):** two port traits, `SearchProvider` and `Fetcher`, each with a `reqwest`-backed production adapter (`BraveSearchProvider`, `ReqwestFetcher`) and a hand-rolled fake in the unit test — mirroring `script_runner.rs`'s `ScriptRunner`/`TokioScriptRunner`/`FakeScriptRunner`. The mock point is the trait, not the HTTP wire, so no HTTP-mock crate is added and tests assert tool logic. CI makes zero live calls.
- **web_search provider (resolves research "(tbd)"):** a JSON HTTP search API behind `SearchProvider`, concrete v1 adapter targeting the Brave Search API (single authenticated GET, key from `BRAVE_SEARCH_API_KEY`). Contained by the port — swappable to Serper/Tavily/internal with zero test change; never exercised in CI.
- **web_fetch client:** `reqwest` behind `Fetcher`, returning the raw response body text plus status and content-type.
- **Tool mapping:** each `execute` destructures the `ToolUse` (Feature 1 idiom), reads its string argument from `input: serde_yaml_ng::Value`, calls its port, and returns a `ToolResult` echoing the call `id` — `is_error: Some(true)` for a bad argument, a non-2xx status, or a network/provider failure; `Err(ToolError::NotAToolUse)` only for a non-tool_use block.
- **Fixtures (owned here):** `e2e/research/context/tools/web_search.json` and `web_fetch.json`, byte-shaped like the insurance-claim JSONs; their `name` strings are the contract Feature 3's evals assert on.
- **Crates:** only change is promoting `reqwest` (already locked at 0.13.3 via rig) to a direct dependency with `default-features = false, features = ["rustls-tls", "json"]`; the resolved lockfile set is unchanged. No HTML parser and no HTTP-mock library are added.

**Deferred (carried, not gold-plated in v1):**
- HTML→text extraction in `web_fetch` (raw body for v1; the `Fetcher` port absorbs a later extraction pass behind the same seam).
- A live integration test for `BraveSearchProvider`/`ReqwestFetcher` analogous to `tests/eval_script.rs`'s real-`python3` test (gated on a key; out of scope while CI stays network-free).
- Tool-call error *recovery*/retry on an `is_error` result — the run loop's job, already deferred at project altitude (research.md "What is deferred"). v1 only *emits* the `is_error` result.
- A real executor registry wiring `web_search`/`web_fetch` into `cli/run.rs` (Feature 1 ships the empty `NoopToolExecutor::default()` there; a registry is the project's named follow-on — project design §6).
