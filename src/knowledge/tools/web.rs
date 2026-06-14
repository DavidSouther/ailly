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
        // Destructure — identical to NoopToolExecutor (mod.rs:96). A non-tool_use
        // block is a structural harness violation, never model-recoverable.
        let ContentBlock::ToolUse { id, name: _, input } = call else {
            return Err(ToolError::NotAToolUse);
        };
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::SearchError;
    use super::SearchHit;
    use super::SearchProvider;
    use super::SearchQuery;
    use super::SearchResults;
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
}
