//! The two real tools, `web_search` and `web_fetch`, each implementing the
//! Feature 1 [`ToolExecutor`](super::ToolExecutor) contract.
//!
//! Each tool depends on a port trait ([`SearchProvider`], [`Fetcher`]), never
//! on `reqwest` directly, so the unit tests drive the tool against a
//! hand-rolled fake and never open a socket. The `reqwest`-backed production
//! adapters ([`BraveSearchProvider`], `ReqwestFetcher`) are the documented
//! `TokioScriptRunner` posture: a network adapter has no seam below it to mock,
//! so it is compiled-but-uncovered, its realness left to a future gated
//! integration test. Mirrors `crate::knowledge::script_runner`'s
//! `ScriptRunner` / `TokioScriptRunner` / `FakeScriptRunner` trio.

use async_trait::async_trait;

use crate::content::conversation::Content;
use crate::content::conversation::ContentBlock;
use crate::content::conversation::ToolUseId;
use crate::knowledge::tools::ToolError;
use crate::knowledge::tools::ToolExecutor;

/// Borrow the `id` and `input` of a `tool_use` block, or reject a non-tool_use
/// block as a structural harness violation. The guard both `web_search` and
/// `web_fetch` open `execute` with; mirrors the destructure in
/// `NoopToolExecutor::execute` (mod.rs), which also binds `name`.
///
/// # Errors
/// Returns [`ToolError::NotAToolUse`] when `call` is not a
/// [`ContentBlock::ToolUse`].
fn tool_use_parts(call: &ContentBlock) -> Result<(&ToolUseId, &serde_yaml_ng::Value), ToolError> {
    let ContentBlock::ToolUse { id, name: _, input } = call else {
        return Err(ToolError::NotAToolUse);
    };
    Ok((id, input))
}

/// Default cap on results requested when a `web_search` call omits `count`.
/// Matches the `default` in `web_search.json` (Feature 2, step 3).
const DEFAULT_RESULT_COUNT: u8 = 5;

/// One web search request. The complete call description; the provider only
/// executes it. Mirrors `ScriptSpec`'s "executor builds it, runner runs it"
/// split.
pub struct SearchQuery {
    pub query: String,
    /// Cap on results requested; the tool sets a small default.
    pub count: u8,
}

/// One result row, provider-normalised. A flat shape so the production adapter
/// maps any provider's JSON into it and the tool never sees provider-specific
/// fields.
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// The hits a search returned. Zero hits is a valid, non-error result.
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
}

/// Structural failure of the search itself (network, auth, bad JSON), distinct
/// from a search that ran and returned zero hits. Carries a `String`, not a
/// wrapped `reqwest::Error`, so the port signature and the fake stay
/// reqwest-free — exactly as `ScriptError` carries `std::io::Error`, not a
/// tokio handle.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("search request failed: {0}")]
    Request(String),
    #[error("search response was not the expected shape: {0}")]
    Decode(String),
}

/// The sole seam between `WebSearch::execute` and the network, so the unit test
/// drives the tool against a hand-rolled fake. Mirrors `ScriptRunner`.
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Run `query` and return its hits.
    ///
    /// # Errors
    /// Returns [`SearchError`] when the request fails or the response cannot be
    /// decoded. Zero hits is `Ok(SearchResults { hits: [] })`, not an error.
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, SearchError>;
}

/// Production adapter targeting the Brave Search API: a single authenticated
/// `GET https://api.search.brave.com/res/v1/web/search`, the key from
/// `BRAVE_SEARCH_API_KEY` read lazily (only when `search` is invoked, never at
/// construction). The sole real impl; unit tests use a hand-rolled
/// `FakeSearchProvider` instead, the documented `TokioScriptRunner` posture —
/// a network adapter has no seam below it to mock, so its realness is proven
/// only by a future gated integration test, not the unit suite.
pub struct BraveSearchProvider {
    client: reqwest::Client,
}

/// The Brave Search endpoint a live `web_search` run targets.
const BRAVE_SEARCH_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";

impl BraveSearchProvider {
    /// Construct over a default `reqwest::Client`. The API key is read lazily
    /// per request, so constructing the provider in a no-search run never
    /// fails — mirrors `BraveSearchProvider::from_env` resolving the key only
    /// when invoked.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for BraveSearchProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// One row of the Brave `web.results[]` array. Only the three fields the flat
/// [`SearchHit`] needs are decoded; the rest of the provider's JSON is ignored.
#[derive(serde::Deserialize)]
struct BraveResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

#[derive(serde::Deserialize)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(serde::Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWeb>,
}

#[async_trait]
impl SearchProvider for BraveSearchProvider {
    async fn search(&self, query: SearchQuery) -> Result<SearchResults, SearchError> {
        let api_key = std::env::var("BRAVE_SEARCH_API_KEY")
            .map_err(|_| SearchError::Request(String::from("BRAVE_SEARCH_API_KEY not set")))?;
        let count = query.count.to_string();
        // Build the URL with `url::Url::parse_with_params` (reqwest re-exports
        // it) rather than `RequestBuilder::query`, which needs reqwest's `query`
        // feature and would pull a new `serde_urlencoded` into the lockfile —
        // the manifest change must stay lockfile-neutral.
        let url = reqwest::Url::parse_with_params(
            BRAVE_SEARCH_ENDPOINT,
            &[("q", query.query.as_str()), ("count", count.as_str())],
        )
        .map_err(|err| SearchError::Request(err.to_string()))?;
        let response = self
            .client
            .get(url)
            .header("X-Subscription-Token", api_key)
            .send()
            .await
            .map_err(|err| SearchError::Request(err.to_string()))?;
        let parsed = response
            .json::<BraveResponse>()
            .await
            .map_err(|err| SearchError::Decode(err.to_string()))?;
        let hits = parsed
            .web
            .map(|web| web.results)
            .unwrap_or_default()
            .into_iter()
            .map(|row| SearchHit {
                title: row.title,
                url: row.url,
                snippet: row.description,
            })
            .collect();
        Ok(SearchResults { hits })
    }
}

/// One web fetch request. The complete call description; the fetcher only
/// executes it. Mirrors [`SearchQuery`]'s "executor builds it, port runs it"
/// split.
pub struct FetchRequest {
    pub url: String,
}

/// One fetched response, port-normalised. A flat shape so the production
/// adapter maps a `reqwest::Response` into it and the tool never sees
/// reqwest-specific types.
pub struct FetchResponse {
    /// HTTP status code. A non-2xx status is reported, not an error — the tool
    /// decides `is_error` from it (step 5), mirroring how `ScriptRunner` treats
    /// a non-zero exit as data.
    pub status: u16,
    /// Decoded body text. v1 returns the raw response body; HTML→text
    /// extraction is deferred.
    pub body: String,
    /// `Content-Type` header, lower-cased, if present. Lets the tool note
    /// "(html)" in the result without parsing it.
    pub content_type: Option<String>,
}

/// Structural failure of the fetch itself (malformed URL, transport), distinct
/// from a fetch that ran and returned a non-2xx status. Carries a `String`, not
/// a wrapped `reqwest::Error`, so the port signature and the fake stay
/// reqwest-free — exactly as [`SearchError`] and `ScriptError` do.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("fetch request failed: {0}")]
    Request(String),
    #[error("invalid url {url:?}: {reason}")]
    InvalidUrl { url: String, reason: String },
}

/// The sole seam between `WebFetch::execute` and the network, so the unit test
/// drives the tool against a hand-rolled fake. Mirrors [`SearchProvider`] and
/// `ScriptRunner`.
#[async_trait]
pub trait Fetcher: Send + Sync {
    /// Fetch `request.url` and return its response.
    ///
    /// # Errors
    /// Returns [`FetchError`] on a malformed URL or a transport failure. A
    /// non-2xx HTTP status is **not** an error — it is an `Ok(FetchResponse)`
    /// whose `status` the tool reports, mirroring how `ScriptRunner` treats a
    /// non-zero exit as data, not an error.
    async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, FetchError>;
}

/// Production adapter over `reqwest::Client`: validate the URL, issue a single
/// `GET`, and lower the response into [`FetchResponse`]. The sole real impl;
/// unit tests use a hand-rolled `FakeFetcher` instead, the documented
/// `TokioScriptRunner` posture — a network adapter has no seam below it to
/// mock, so its realness is proven only by a future gated integration test, not
/// the unit suite.
pub struct ReqwestFetcher {
    client: reqwest::Client,
}

impl ReqwestFetcher {
    /// Construct over a default `reqwest::Client`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for ReqwestFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Fetcher for ReqwestFetcher {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, FetchError> {
        // Validate the URL up front so a malformed input is a structural
        // FetchError rather than a transport failure deep in reqwest.
        let url = reqwest::Url::parse(&request.url).map_err(|err| FetchError::InvalidUrl {
            url: request.url.clone(),
            reason: err.to_string(),
        })?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|err| FetchError::Request(err.to_string()))?;
        let status = response.status().as_u16();
        // Read the Content-Type before consuming the body; lower-cased so the
        // tool can match on it without case juggling.
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_lowercase);
        // A non-2xx status is data, not an error: the body still rides through
        // so the tool can report what the server said.
        let body = response
            .text()
            .await
            .map_err(|err| FetchError::Request(err.to_string()))?;
        Ok(FetchResponse {
            status,
            body,
            content_type,
        })
    }
}

/// Build an `is_error: None` `tool_result` echoing the call `id`. The success
/// constructor over `ContentBlock::ToolResult`; pairs with [`error_result`].
fn ok_result(id: &ToolUseId, text: String) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.clone(),
        content: Content::from(text),
        is_error: None,
    }
}

/// Build an `is_error: Some(true)` `tool_result` echoing the call `id`. The
/// model-recoverable failure constructor: a bad argument or a provider error
/// rides back to the model as a `tool_result`, not a run-aborting `ToolError`.
fn error_result(id: &ToolUseId, text: &str) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: id.clone(),
        content: Content::from(text.to_owned()),
        is_error: Some(true),
    }
}

/// Format `SearchResults` into a compact, model-readable text block: one
/// `title — url` line plus the snippet per hit. Zero hits renders an explicit
/// "no results" line so the model can tell an empty search from a failed one.
fn render_hits(results: &SearchResults) -> String {
    if results.hits.is_empty() {
        return String::from("No results.");
    }
    results
        .hits
        .iter()
        .map(|hit| format!("{} — {}\n{}", hit.title, hit.url, hit.snippet))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The `web_search` tool: reads a `query` argument, runs it through the
/// injected [`SearchProvider`], and shapes the hits into a `tool_result`. The
/// provider is the sole network seam — production wires
/// [`BraveSearchProvider`]; the unit tests wire a hand-rolled fake.
pub struct WebSearch {
    provider: Box<dyn SearchProvider>,
}

impl WebSearch {
    /// Construct over an injected [`SearchProvider`]. Production passes
    /// [`BraveSearchProvider`]; tests pass a fake.
    #[must_use]
    pub fn new(provider: Box<dyn SearchProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl ToolExecutor for WebSearch {
    async fn execute(&self, call: &ContentBlock) -> Result<ContentBlock, ToolError> {
        let (id, input) = tool_use_parts(call)?;
        // A missing/non-string `query` is the model's mistake: surface it as an
        // is_error result it can retry, not a run-aborting ToolError.
        let Some(query) = input.get("query").and_then(serde_yaml_ng::Value::as_str) else {
            return Ok(error_result(id, "web_search requires a string `query`"));
        };
        let count = input
            .get("count")
            .and_then(serde_yaml_ng::Value::as_u64)
            .and_then(|n| u8::try_from(n).ok())
            .unwrap_or(DEFAULT_RESULT_COUNT);
        match self
            .provider
            .search(SearchQuery {
                query: query.to_owned(),
                count,
            })
            .await
        {
            Ok(results) => Ok(ok_result(id, render_hits(&results))),
            Err(err) => Ok(error_result(id, &err.to_string())),
        }
    }
}

/// Format a [`FetchResponse`] into a model-readable text block: a status line
/// (with the content-type annotated when present) followed by the body, so a
/// downstream model can tell HTML from JSON from plain text and read what the
/// server actually returned.
fn render_response(response: &FetchResponse) -> String {
    let header = match &response.content_type {
        Some(content_type) => format!("HTTP {} ({content_type})", response.status),
        None => format!("HTTP {}", response.status),
    };
    format!("{header}\n\n{}", response.body)
}

/// The `web_fetch` tool: reads a `url` argument, fetches it through the
/// injected [`Fetcher`], and shapes the response into a `tool_result`. The
/// fetcher is the sole network seam — production wires [`ReqwestFetcher`]; the
/// unit tests wire a hand-rolled fake.
pub struct WebFetch {
    fetcher: Box<dyn Fetcher>,
}

impl WebFetch {
    /// Construct over an injected [`Fetcher`]. Production passes
    /// [`ReqwestFetcher`]; tests pass a fake.
    #[must_use]
    pub fn new(fetcher: Box<dyn Fetcher>) -> Self {
        Self { fetcher }
    }
}

#[async_trait]
impl ToolExecutor for WebFetch {
    async fn execute(&self, call: &ContentBlock) -> Result<ContentBlock, ToolError> {
        let (id, input) = tool_use_parts(call)?;
        // A missing/non-string `url` is the model's mistake: surface it as an
        // is_error result it can retry, not a run-aborting ToolError.
        let Some(url) = input.get("url").and_then(serde_yaml_ng::Value::as_str) else {
            return Ok(error_result(id, "web_fetch requires a string `url`"));
        };
        match self
            .fetcher
            .fetch(FetchRequest {
                url: url.to_owned(),
            })
            .await
        {
            // A 2xx status is a success; any other status is data the model can
            // act on, surfaced as an is_error result with the status reported.
            Ok(response) if (200..300).contains(&response.status) => {
                Ok(ok_result(id, render_response(&response)))
            }
            Ok(response) => Ok(error_result(id, &render_response(&response))),
            Err(err) => Ok(error_result(id, &err.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::FetchError;
    use super::FetchRequest;
    use super::FetchResponse;
    use super::Fetcher;
    use super::SearchError;
    use super::SearchHit;
    use super::SearchProvider;
    use super::SearchQuery;
    use super::SearchResults;
    use super::WebFetch;
    use super::WebSearch;
    use crate::content::conversation::Content;
    use crate::content::conversation::ContentBlock;
    use crate::content::conversation::ToolUseId;
    use crate::knowledge::tools::ToolExecutor;

    /// Hand-rolled search fake (the `FakeScriptRunner` precedent): returns
    /// canned results in order and records every [`SearchQuery`] it was handed,
    /// so a test drives the port deterministically and inspects the wiring.
    struct FakeSearchProvider {
        canned: Mutex<VecDeque<Result<SearchResults, SearchError>>>,
        calls: Mutex<Vec<SearchQuery>>,
    }

    impl FakeSearchProvider {
        fn new(outcomes: Vec<Result<SearchResults, SearchError>>) -> Self {
            Self {
                canned: Mutex::new(outcomes.into()),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl SearchProvider for FakeSearchProvider {
        async fn search(&self, query: SearchQuery) -> Result<SearchResults, SearchError> {
            self.calls.lock().expect("calls lock").push(query);
            self.canned
                .lock()
                .expect("canned lock")
                .pop_front()
                .expect("FakeSearchProvider: a canned result is available")
        }
    }

    /// Lets a test keep an `Arc` handle to inspect `calls` after the fake is
    /// boxed into the tool — needed by the missing-query test, which asserts
    /// the provider was never reached.
    #[async_trait::async_trait]
    impl SearchProvider for std::sync::Arc<FakeSearchProvider> {
        async fn search(&self, query: SearchQuery) -> Result<SearchResults, SearchError> {
            (**self).search(query).await
        }
    }

    #[tokio::test]
    async fn fake_search_provider_drives_through_the_port() {
        let provider = FakeSearchProvider::new(vec![Ok(SearchResults {
            hits: vec![SearchHit {
                title: String::from("Rust"),
                url: String::from("https://www.rust-lang.org"),
                snippet: String::from("A language empowering everyone."),
            }],
        })]);

        // Drive the port through `&dyn SearchProvider` to prove it is
        // object-safe and the fake is drivable.
        let port: &dyn SearchProvider = &provider;
        let results = port
            .search(SearchQuery {
                query: String::from("rust lang"),
                count: 5,
            })
            .await
            .expect("the canned Ok result is served");

        assert_eq!(results.hits.len(), 1);
        assert_eq!(results.hits[0].url, "https://www.rust-lang.org");
        assert_eq!(
            provider.calls.lock().expect("calls lock")[0].query,
            "rust lang"
        );
    }

    /// Build a `web_search` `tool_use` block carrying `input`. A YAML mapping
    /// keyed by `&str` so a test sets or omits `query`/`count` directly.
    fn tool_use(id: &str, input: serde_yaml_ng::Value) -> ContentBlock {
        ContentBlock::ToolUse {
            id: ToolUseId::from(id),
            name: String::from("web_search"),
            input,
        }
    }

    /// Pull the text out of a `ToolResult`, asserting the result echoes `id`
    /// and carries `is_error`. Centralises the shape check the three tests
    /// share.
    fn assert_tool_result(
        block: &ContentBlock,
        expected_id: &str,
        expected_is_error: Option<bool>,
    ) -> String {
        let ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } = block
        else {
            panic!("expected a ToolResult block, got {block:?}");
        };
        assert_eq!(*tool_use_id, ToolUseId::from(expected_id));
        assert_eq!(*is_error, expected_is_error);
        match content {
            Content::Text(text) => text.clone(),
            Content::Blocks(_) => panic!("expected text content, got blocks"),
        }
    }

    #[tokio::test]
    async fn web_search_shapes_hits_into_tool_result() {
        let provider = FakeSearchProvider::new(vec![Ok(SearchResults {
            hits: vec![
                SearchHit {
                    title: String::from("Rust"),
                    url: String::from("https://www.rust-lang.org"),
                    snippet: String::from("A language empowering everyone."),
                },
                SearchHit {
                    title: String::from("The Rust Book"),
                    url: String::from("https://doc.rust-lang.org/book/"),
                    snippet: String::from("The official guide."),
                },
            ],
        })]);
        let tool = WebSearch::new(Box::new(provider));

        let input = serde_yaml_ng::from_str("query: rust lang").expect("valid yaml input");
        let result = tool
            .execute(&tool_use("toolu_search_1", input))
            .await
            .expect("a search tool call always yields a ToolResult, never a ToolError");

        let text = assert_tool_result(&result, "toolu_search_1", None);
        // Exact projection of a known hit (one-character-bug check): the
        // `title — url` line must match byte-for-byte, not merely be non-empty.
        assert!(
            text.contains("Rust — https://www.rust-lang.org"),
            "first hit projection missing: {text}"
        );
        assert!(
            text.contains("The Rust Book — https://doc.rust-lang.org/book/"),
            "second hit projection missing: {text}"
        );
    }

    #[tokio::test]
    async fn web_search_missing_query_is_error_result() {
        let provider = FakeSearchProvider::new(vec![]);
        // Record the calls vec by handle before the provider is boxed into the
        // tool, so the test can assert the provider was never reached.
        let calls = std::sync::Arc::new(provider);
        let tool = WebSearch::new(Box::new(std::sync::Arc::clone(&calls)));

        // `input` carries `count` but no `query`.
        let input = serde_yaml_ng::from_str("count: 3").expect("valid yaml input");
        let result = tool
            .execute(&tool_use("toolu_search_2", input))
            .await
            .expect("a bad argument yields an is_error ToolResult, not a ToolError");

        assert_tool_result(&result, "toolu_search_2", Some(true));
        assert!(
            calls.calls.lock().expect("calls lock").is_empty(),
            "the provider must not be called when `query` is missing"
        );
    }

    #[tokio::test]
    async fn web_search_provider_error_is_error_result() {
        let provider =
            FakeSearchProvider::new(vec![Err(SearchError::Request(String::from("brave 503")))]);
        let tool = WebSearch::new(Box::new(provider));

        let input = serde_yaml_ng::from_str("query: rust lang").expect("valid yaml input");
        let result = tool
            .execute(&tool_use("toolu_search_3", input))
            .await
            .expect("a provider error rides through as an is_error ToolResult");

        let text = assert_tool_result(&result, "toolu_search_3", Some(true));
        assert!(
            text.contains("503"),
            "the provider error message must ride through: {text}"
        );
    }

    #[test]
    fn web_search_json_round_trips_into_tool_definition() {
        // The real byte shape Feature 3's `kind: tools` block parses. JSON is a
        // YAML subset, so the assembly resolver's `serde_yaml_ng` parser reads
        // it — the exact path `tool_definition_round_trips_lookup_policy_fixture`
        // (conversation.rs) proves for the insurance-claim fixtures. `name` is
        // the contract Feature 3's evals assert on (`must_call_tool: web_search`);
        // `required` must list the `query` argument `execute` reads.
        let fixture = include_str!("../../../e2e/research/context/tools/web_search.json");

        let tool: crate::content::conversation::ToolDefinition =
            serde_yaml_ng::from_str(fixture).expect("web_search.json parses");

        assert_eq!(tool.name, "web_search");
        let required = tool.input_schema["required"]
            .as_sequence()
            .expect("input_schema.required is a sequence");
        assert!(
            required.contains(&serde_yaml_ng::Value::from("query")),
            "web_search input_schema.required must list `query`: {required:?}"
        );
    }

    /// Hand-rolled fetch fake (the `FakeScriptRunner` precedent): returns
    /// canned responses in order and records every [`FetchRequest`] it was
    /// handed, so a test drives the port deterministically and inspects the
    /// wiring.
    struct FakeFetcher {
        canned: Mutex<VecDeque<Result<FetchResponse, FetchError>>>,
        calls: Mutex<Vec<FetchRequest>>,
    }

    impl FakeFetcher {
        fn new(outcomes: Vec<Result<FetchResponse, FetchError>>) -> Self {
            Self {
                canned: Mutex::new(outcomes.into()),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl Fetcher for FakeFetcher {
        async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, FetchError> {
            self.calls.lock().expect("calls lock").push(request);
            self.canned
                .lock()
                .expect("canned lock")
                .pop_front()
                .expect("FakeFetcher: a canned response is available")
        }
    }

    #[tokio::test]
    async fn fake_fetcher_drives_through_the_port() {
        let fetcher = FakeFetcher::new(vec![Ok(FetchResponse {
            status: 200,
            body: String::from("<page>hello</page>"),
            content_type: Some(String::from("text/html")),
        })]);

        // Drive the port through `&dyn Fetcher` to prove it is object-safe and
        // the fake is drivable. A non-2xx status would be `Ok` data too; this
        // round-trip only proves the seam, the executor (step 5) reads `status`.
        let port: &dyn Fetcher = &fetcher;
        let response = port
            .fetch(FetchRequest {
                url: String::from("https://example.com/"),
            })
            .await
            .expect("the canned Ok response is served");

        assert_eq!(response.status, 200);
        assert_eq!(response.body, "<page>hello</page>");
        assert_eq!(
            fetcher.calls.lock().expect("calls lock")[0].url,
            "https://example.com/"
        );
    }

    /// Lets a test keep an `Arc` handle to inspect `calls` after the fake is
    /// boxed into the tool — needed by the missing-url test, which asserts the
    /// fetcher was never reached.
    #[async_trait::async_trait]
    impl Fetcher for std::sync::Arc<FakeFetcher> {
        async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, FetchError> {
            (**self).fetch(request).await
        }
    }

    /// Build a `web_fetch` `tool_use` block carrying `input`. A YAML mapping
    /// keyed by `&str` so a test sets or omits `url` directly. The executor
    /// ignores `name`, so this mirrors [`tool_use`] for the fetch tool.
    fn fetch_tool_use(id: &str, input: serde_yaml_ng::Value) -> ContentBlock {
        ContentBlock::ToolUse {
            id: ToolUseId::from(id),
            name: String::from("web_fetch"),
            input,
        }
    }

    #[tokio::test]
    async fn web_fetch_shapes_body_into_tool_result() {
        let fetcher = FakeFetcher::new(vec![Ok(FetchResponse {
            status: 200,
            body: String::from("<page>hello</page>"),
            content_type: Some(String::from("text/html")),
        })]);
        let tool = WebFetch::new(Box::new(fetcher));

        let input = serde_yaml_ng::from_str("url: https://example.com/").expect("valid yaml input");
        let result = tool
            .execute(&fetch_tool_use("toolu_fetch_1", input))
            .await
            .expect("a fetch tool call always yields a ToolResult, never a ToolError");

        let text = assert_tool_result(&result, "toolu_fetch_1", None);
        // The result must carry both the body and the status so a downstream
        // model can read what the server returned.
        assert!(
            text.contains("<page>hello</page>"),
            "fetched body missing from result: {text}"
        );
        assert!(
            text.contains("200"),
            "fetch status missing from result: {text}"
        );
    }

    #[tokio::test]
    async fn web_fetch_non_2xx_is_error_result() {
        let fetcher = FakeFetcher::new(vec![Ok(FetchResponse {
            status: 404,
            body: String::from("Not Found"),
            content_type: Some(String::from("text/plain")),
        })]);
        let tool = WebFetch::new(Box::new(fetcher));

        let input =
            serde_yaml_ng::from_str("url: https://example.com/missing").expect("valid yaml input");
        let result = tool
            .execute(&fetch_tool_use("toolu_fetch_2", input))
            .await
            .expect("a non-2xx status rides through as an is_error ToolResult");

        let text = assert_tool_result(&result, "toolu_fetch_2", Some(true));
        assert!(
            text.contains("404"),
            "the non-2xx status must be reported in the result: {text}"
        );
    }

    #[tokio::test]
    async fn web_fetch_missing_url_is_error_result() {
        let fetcher = FakeFetcher::new(vec![]);
        // Record the calls vec by handle before the fetcher is boxed into the
        // tool, so the test can assert the fetcher was never reached.
        let calls = std::sync::Arc::new(fetcher);
        let tool = WebFetch::new(Box::new(std::sync::Arc::clone(&calls)));

        // `input` is a mapping with no `url`.
        let input = serde_yaml_ng::from_str("count: 3").expect("valid yaml input");
        let result = tool
            .execute(&fetch_tool_use("toolu_fetch_3", input))
            .await
            .expect("a bad argument yields an is_error ToolResult, not a ToolError");

        assert_tool_result(&result, "toolu_fetch_3", Some(true));
        assert!(
            calls.calls.lock().expect("calls lock").is_empty(),
            "the fetcher must not be called when `url` is missing"
        );
    }

    #[test]
    fn web_fetch_json_round_trips_into_tool_definition() {
        // The real byte shape Feature 3's `kind: tools` block parses, mirroring
        // `web_search_json_round_trips_into_tool_definition`. JSON is a YAML
        // subset, so the assembly resolver's `serde_yaml_ng` parser reads it.
        // `name` is the contract Feature 3's evals assert on
        // (`must_call_tool: web_fetch`); `required` must list the `url`
        // argument `execute` reads.
        let fixture = include_str!("../../../e2e/research/context/tools/web_fetch.json");

        let tool: crate::content::conversation::ToolDefinition =
            serde_yaml_ng::from_str(fixture).expect("web_fetch.json parses");

        assert_eq!(tool.name, "web_fetch");
        let required = tool.input_schema["required"]
            .as_sequence()
            .expect("input_schema.required is a sequence");
        assert!(
            required.contains(&serde_yaml_ng::Value::from("url")),
            "web_fetch input_schema.required must list `url`: {required:?}"
        );
    }
}
