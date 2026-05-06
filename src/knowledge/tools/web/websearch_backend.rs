use async_trait::async_trait;

use super::search::{SearchBackend, SearchError, SearchResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResolvedProvider {
    Google { api_key: String, cx: String },
    Brave { api_key: String },
    DuckDuckGo,
    SearxNG { endpoint: String },
    SerpApi { api_key: String },
    Exa { api_key: String },
    Tavily { api_key: String },
    Arxiv,
}

#[derive(Debug)]
pub struct WebsearchBackend {
    pub(crate) provider: ResolvedProvider,
}

fn wrap_fallible<P>(
    name: &str,
    ctor: Result<P, websearch::error::SearchError>,
) -> Result<(String, Box<dyn websearch::SearchProvider>), SearchError>
where
    P: websearch::SearchProvider + 'static,
{
    ctor.map(|p| {
        (
            name.to_string(),
            Box::new(p) as Box<dyn websearch::SearchProvider>,
        )
    })
    .map_err(|e| SearchError::Provider {
        provider: name.to_string(),
        source: Box::new(e),
    })
}

impl WebsearchBackend {
    fn make_provider(&self) -> Result<(String, Box<dyn websearch::SearchProvider>), SearchError> {
        match &self.provider {
            ResolvedProvider::Google { api_key, cx } => wrap_fallible(
                "google",
                websearch::providers::google::GoogleProvider::new(api_key, cx),
            ),
            ResolvedProvider::Brave { api_key } => wrap_fallible(
                "brave",
                websearch::providers::brave::BraveProvider::new(api_key),
            ),
            ResolvedProvider::DuckDuckGo => Ok((
                "duckduckgo".to_string(),
                Box::new(websearch::providers::duckduckgo::DuckDuckGoProvider::new()),
            )),
            ResolvedProvider::SearxNG { endpoint } => wrap_fallible(
                "searxng",
                websearch::providers::searxng::SearxNGProvider::new(endpoint),
            ),
            ResolvedProvider::SerpApi { api_key } => wrap_fallible(
                "serpapi",
                websearch::providers::serpapi::SerpApiProvider::new(api_key),
            ),
            ResolvedProvider::Exa { api_key } => {
                wrap_fallible("exa", websearch::providers::exa::ExaProvider::new(api_key))
            }
            ResolvedProvider::Tavily { api_key } => wrap_fallible(
                "tavily",
                websearch::providers::tavily::TavilyProvider::new(api_key),
            ),
            ResolvedProvider::Arxiv => Ok((
                "arxiv".to_string(),
                Box::new(websearch::providers::arxiv::ArxivProvider::new()),
            )),
        }
    }
}

#[async_trait]
impl SearchBackend for WebsearchBackend {
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<SearchResult>, SearchError> {
        let (provider_name, provider) = self.make_provider()?;
        let options = websearch::SearchOptions {
            query: query.to_string(),
            max_results: Some(max_results as u32),
            provider,
            ..Default::default()
        };
        let results = websearch::web_search(options)
            .await
            .map_err(|e| SearchError::Provider {
                provider: provider_name,
                source: Box::new(e),
            })?;
        Ok(results
            .into_iter()
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                snippet: r.snippet.unwrap_or_default(),
            })
            .collect())
    }
}
