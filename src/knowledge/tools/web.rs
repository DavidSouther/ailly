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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::SearchError;
    use super::SearchHit;
    use super::SearchProvider;
    use super::SearchQuery;
    use super::SearchResults;

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
}
