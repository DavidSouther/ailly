use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rig::completion::ToolDefinition;
use rig::tool::Tool;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("no search backend configured")]
    NoBackendConfigured,
    #[error("provider {provider} failed")]
    Provider {
        provider: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("invalid search arguments: {reason}")]
    InvalidArgs { reason: String },
}

#[async_trait]
pub trait SearchBackend: Send + Sync {
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>, SearchError>;
}

pub struct RefuseSearchBackend;

#[async_trait]
impl SearchBackend for RefuseSearchBackend {
    async fn search(
        &self,
        _query: &str,
        _max_results: usize,
    ) -> Result<Vec<SearchResult>, SearchError> {
        Err(SearchError::NoBackendConfigured)
    }
}

pub struct MapSearchBackend {
    entries: HashMap<String, Vec<SearchResult>>,
}

impl MapSearchBackend {
    pub fn new<I, K>(entries: I) -> Self
    where
        I: IntoIterator<Item = (K, Vec<SearchResult>)>,
        K: Into<String>,
    {
        Self {
            entries: entries.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        }
    }
}

#[async_trait]
impl SearchBackend for MapSearchBackend {
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>, SearchError> {
        match self.entries.get(query) {
            Some(results) => {
                let take = max_results.min(results.len());
                Ok(results.iter().take(take).cloned().collect())
            }
            None => Err(SearchError::NoBackendConfigured),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct WebSearchArgs {
    pub query: String,
    #[serde(default)]
    pub max_results: Option<usize>,
}

pub struct WebSearch {
    backend: Arc<dyn SearchBackend>,
    default_max_results: usize,
}

impl WebSearch {
    pub const NAME: &'static str = "web.search";
    pub const MIN_MAX_RESULTS: usize = 1;
    pub const MAX_MAX_RESULTS: usize = 20;

    pub fn new(backend: Arc<dyn SearchBackend>) -> Self {
        Self {
            backend,
            default_max_results: 5,
        }
    }
}

impl Tool for WebSearch {
    const NAME: &'static str = "web.search";

    type Error = SearchError;
    type Args = WebSearchArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search the web for a query string and return a \
                JSON array of result objects with title, url, and snippet \
                fields. Use for query-driven discovery when a known URL \
                is not yet in hand."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": Self::MIN_MAX_RESULTS,
                        "maximum": Self::MAX_MAX_RESULTS,
                        "description": "Maximum number of results to return \
                            (clamped to the supported range)."
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let raw = args.max_results.unwrap_or(self.default_max_results);
        let max_results = raw.clamp(Self::MIN_MAX_RESULTS, Self::MAX_MAX_RESULTS);
        let results = self.backend.search(&args.query, max_results).await?;
        serde_json::to_string(&results).map_err(|e| SearchError::InvalidArgs {
            reason: format!("serialize search results: {e}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    use rig::tool::ToolDyn;

    use crate::knowledge::permissions::{
        AllowAllBackend, DenyAllBackend, PermissionBackend, PermissionGated,
    };

    use super::super::classifier::WebSearchClassifier;

    #[test]
    fn search_backend_is_dyn_safe() {
        let _: Arc<dyn SearchBackend> = Arc::new(RefuseSearchBackend);
    }

    #[tokio::test]
    async fn refuse_search_backend_errors_with_no_backend_configured() {
        let backend = RefuseSearchBackend;
        let err = backend
            .search("anything", 5)
            .await
            .expect_err("RefuseSearchBackend always errors");
        assert!(matches!(err, SearchError::NoBackendConfigured));
    }

    fn fixture() -> Vec<SearchResult> {
        vec![
            SearchResult {
                title: "first".to_string(),
                url: "https://example.test/first".to_string(),
                snippet: "primary".to_string(),
            },
            SearchResult {
                title: "second".to_string(),
                url: "https://example.test/second".to_string(),
                snippet: "secondary".to_string(),
            },
        ]
    }

    #[tokio::test]
    async fn map_search_backend_returns_recorded_results_on_hit() {
        let recorded = fixture();
        let backend = MapSearchBackend::new([("hit", recorded.clone())]);
        let results = backend.search("hit", 5).await.expect("hit returns results");
        assert_eq!(results, recorded);
    }

    #[tokio::test]
    async fn map_search_backend_errors_with_no_backend_configured_on_miss() {
        let backend = MapSearchBackend::new([("hit", fixture())]);
        let err = backend.search("miss", 5).await.expect_err("miss errors");
        assert!(matches!(err, SearchError::NoBackendConfigured));
    }

    #[tokio::test]
    async fn web_search_tool_serializes_results_as_json_string() {
        let backend: Arc<dyn SearchBackend> = Arc::new(MapSearchBackend::new([("q", fixture())]));
        let tool = WebSearch::new(backend);
        let body = Tool::call(
            &tool,
            WebSearchArgs {
                query: "q".to_string(),
                max_results: Some(2),
            },
        )
        .await
        .expect("call succeeds");
        let parsed: Vec<SearchResult> =
            serde_json::from_str(&body).expect("body is JSON Vec<SearchResult>");
        assert_eq!(parsed, fixture());
    }

    struct CapturingBackend {
        seen: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl SearchBackend for CapturingBackend {
        async fn search(
            &self,
            _query: &str,
            max_results: usize,
        ) -> Result<Vec<SearchResult>, SearchError> {
            self.seen.store(max_results, Ordering::SeqCst);
            Ok(Vec::new())
        }
    }

    async fn run_with_capturing(max_results: Option<usize>) -> usize {
        let seen = Arc::new(AtomicUsize::new(0));
        let backend = Arc::new(CapturingBackend {
            seen: Arc::clone(&seen),
        });
        let tool = WebSearch::new(backend);
        Tool::call(
            &tool,
            WebSearchArgs {
                query: "q".to_string(),
                max_results,
            },
        )
        .await
        .expect("call succeeds");
        seen.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn web_search_tool_clamps_max_results_to_upper_bound() {
        assert_eq!(
            run_with_capturing(Some(9999)).await,
            WebSearch::MAX_MAX_RESULTS
        );
    }

    #[tokio::test]
    async fn web_search_tool_uses_default_max_results_when_arg_absent() {
        assert_eq!(run_with_capturing(None).await, 5);
    }

    #[tokio::test]
    async fn web_search_tool_per_call_max_results_overrides_default() {
        assert_eq!(run_with_capturing(Some(3)).await, 3);
    }

    #[test]
    fn web_search_args_missing_query_field_fails_at_deserialize() {
        let result: Result<WebSearchArgs, _> =
            serde_json::from_value(serde_json::json!({ "max_results": 2 }));
        assert!(result.is_err());
    }

    #[test]
    fn web_search_tool_name_is_web_search() {
        assert_eq!(WebSearch::NAME, "web.search");
        assert_eq!(<WebSearch as Tool>::NAME, "web.search");
    }

    #[tokio::test]
    async fn web_search_through_permission_gated_with_allow_all_backend_forwards_to_inner() {
        let backend: Arc<dyn SearchBackend> = Arc::new(MapSearchBackend::new([("q", fixture())]));
        let bare_tool: Arc<dyn ToolDyn> = Arc::new(WebSearch::new(Arc::clone(&backend)));
        let bare_args = serde_json::json!({ "query": "q", "max_results": 2 }).to_string();
        let bare_body = bare_tool.call(bare_args.clone()).await.expect("bare ok");

        let permission_backend: Arc<dyn PermissionBackend> = Arc::new(AllowAllBackend);
        let gated: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
            WebSearch::NAME,
            Arc::new(WebSearch::new(Arc::clone(&backend))) as Arc<dyn ToolDyn>,
            Arc::new(WebSearchClassifier),
            permission_backend,
        ));
        let gated_body = gated.call(bare_args).await.expect("gated ok");
        assert_eq!(bare_body, gated_body);
    }

    #[tokio::test]
    async fn web_search_through_permission_gated_with_deny_all_backend_synthesizes_refusal() {
        struct CountingBackend {
            calls: Arc<AtomicUsize>,
        }
        #[async_trait]
        impl SearchBackend for CountingBackend {
            async fn search(
                &self,
                _query: &str,
                _max_results: usize,
            ) -> Result<Vec<SearchResult>, SearchError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(Vec::new())
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let backend: Arc<dyn SearchBackend> = Arc::new(CountingBackend {
            calls: Arc::clone(&calls),
        });
        let permission_backend: Arc<dyn PermissionBackend> = Arc::new(DenyAllBackend);
        let gated: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
            WebSearch::NAME,
            Arc::new(WebSearch::new(backend)) as Arc<dyn ToolDyn>,
            Arc::new(WebSearchClassifier),
            permission_backend,
        ));
        let body = gated
            .call(serde_json::json!({ "query": "q" }).to_string())
            .await
            .expect("gated returns synthesized refusal as Ok payload");
        assert!(
            body.contains("permission denied"),
            "expected refusal payload, got {body}"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "inner backend must not be consulted on Deny"
        );
    }
}
