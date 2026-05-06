use super::websearch_backend::{ResolvedProvider, WebsearchBackend};

#[derive(Debug, thiserror::Error)]
pub enum WebsearchConfigError {
    #[error("missing provider field")]
    MissingProvider,
    #[error("unknown provider {provider}")]
    UnknownProvider { provider: String },
    #[error("missing api_key for provider {provider}")]
    MissingApiKey { provider: String },
    #[error("missing cx for provider {provider}")]
    MissingCx { provider: String },
    #[error("missing endpoint for provider {provider}")]
    MissingEndpoint { provider: String },
    #[error("undefined env var ${{{name}}}")]
    UndefinedEnvVar { name: String },
    #[error("invalid TOML for [tools.web_search]")]
    InvalidToml {
        #[source]
        source: toml::de::Error,
    },
}

#[derive(Default, Debug, Clone)]
pub struct WebsearchBackendBuilder {
    provider: Option<String>,
    api_key: Option<String>,
    cx: Option<String>,
    endpoint: Option<String>,
}

impl WebsearchBackendBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provider(mut self, value: impl Into<String>) -> Self {
        self.provider = Some(value.into());
        self
    }

    pub fn api_key(mut self, value: impl Into<String>) -> Self {
        self.api_key = Some(value.into());
        self
    }

    pub fn cx(mut self, value: impl Into<String>) -> Self {
        self.cx = Some(value.into());
        self
    }

    pub fn endpoint(mut self, value: impl Into<String>) -> Self {
        self.endpoint = Some(value.into());
        self
    }

    pub fn build(self) -> Result<WebsearchBackend, WebsearchConfigError> {
        let provider = self
            .provider
            .as_deref()
            .ok_or(WebsearchConfigError::MissingProvider)?;
        let resolved =
            match provider {
                "google" => {
                    let api_key = self.api_key.clone().ok_or_else(|| {
                        WebsearchConfigError::MissingApiKey {
                            provider: provider.to_string(),
                        }
                    })?;
                    let cx = self
                        .cx
                        .clone()
                        .ok_or_else(|| WebsearchConfigError::MissingCx {
                            provider: provider.to_string(),
                        })?;
                    ResolvedProvider::Google { api_key, cx }
                }
                "brave" => ResolvedProvider::Brave {
                    api_key: self.require_api_key(provider)?,
                },
                "serpapi" => ResolvedProvider::SerpApi {
                    api_key: self.require_api_key(provider)?,
                },
                "tavily" => ResolvedProvider::Tavily {
                    api_key: self.require_api_key(provider)?,
                },
                "exa" => ResolvedProvider::Exa {
                    api_key: self.require_api_key(provider)?,
                },
                "searxng" => {
                    let endpoint = self.endpoint.clone().ok_or_else(|| {
                        WebsearchConfigError::MissingEndpoint {
                            provider: provider.to_string(),
                        }
                    })?;
                    ResolvedProvider::SearxNG { endpoint }
                }
                "duckduckgo" => ResolvedProvider::DuckDuckGo,
                "arxiv" => ResolvedProvider::Arxiv,
                other => {
                    return Err(WebsearchConfigError::UnknownProvider {
                        provider: other.to_string(),
                    });
                }
            };
        Ok(WebsearchBackend { provider: resolved })
    }

    fn require_api_key(&self, provider: &str) -> Result<String, WebsearchConfigError> {
        self.api_key
            .clone()
            .ok_or_else(|| WebsearchConfigError::MissingApiKey {
                provider: provider.to_string(),
            })
    }
}

#[derive(Default, Debug, Clone)]
pub struct WebToolsConfig {
    pub web_search: Option<WebsearchBackendBuilder>,
}

impl WebToolsConfig {
    pub fn from_toml(value: &toml::Value) -> Result<Self, WebsearchConfigError> {
        let table = value
            .get("tools")
            .and_then(|t| t.get("web_search"))
            .and_then(|t| t.as_table());
        let Some(table) = table else {
            return Ok(Self::default());
        };
        let provider = table
            .get("provider")
            .and_then(|v| v.as_str())
            .ok_or(WebsearchConfigError::MissingProvider)?;
        let provider = interpolate_env(provider)?;
        let mut builder = WebsearchBackendBuilder::new().provider(&provider);
        if let Some(v) = table.get("api_key").and_then(|v| v.as_str()) {
            builder = builder.api_key(interpolate_env(v)?);
        }
        if let Some(v) = table.get("cx").and_then(|v| v.as_str()) {
            builder = builder.cx(interpolate_env(v)?);
        }
        if let Some(v) = table.get("endpoint").and_then(|v| v.as_str()) {
            builder = builder.endpoint(interpolate_env(v)?);
        }
        Ok(Self {
            web_search: Some(builder),
        })
    }
}

/// Single-pass `${NAME}` substitution against the process environment.
///
/// Used by [`WebToolsConfig::from_toml`] to resolve secret references in
/// `[tools.web_search]` string fields so credentials stay out of the
/// committed config. Each `${NAME}` is replaced with `std::env::var("NAME")`;
/// an unset name returns [`WebsearchConfigError::UndefinedEnvVar`] so a
/// missing secret surfaces as a typed parse error rather than a later
/// provider auth failure. Substitution is single-pass: a resolved value is
/// not re-scanned for further `${...}` tokens. An unterminated `${` is
/// treated as a literal tail, mirroring the rule that any string with no
/// `${` passes through unchanged.
fn interpolate_env(input: &str) -> Result<String, WebsearchConfigError> {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
            let end = bytes[i + 2..]
                .iter()
                .position(|&b| b == b'}')
                .map(|p| i + 2 + p);
            match end {
                Some(end_idx) => {
                    let name = std::str::from_utf8(&bytes[i + 2..end_idx])
                        .map_err(|_| WebsearchConfigError::UndefinedEnvVar {
                            name: String::new(),
                        })?
                        .to_string();
                    let value = std::env::var(&name).map_err(|_| {
                        WebsearchConfigError::UndefinedEnvVar { name: name.clone() }
                    })?;
                    out.push_str(&value);
                    i = end_idx + 1;
                    continue;
                }
                None => {
                    out.push_str(&input[i..]);
                    break;
                }
            }
        }
        out.push(input[i..].chars().next().unwrap());
        i += input[i..].chars().next().unwrap().len_utf8();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> toml::Value {
        toml::from_str(s).expect("parse toml")
    }

    #[test]
    fn builder_requires_provider() {
        let err = WebsearchBackendBuilder::new()
            .build()
            .expect_err("must require provider");
        assert!(matches!(err, WebsearchConfigError::MissingProvider));
    }

    #[test]
    fn builder_brave_requires_api_key() {
        let err = WebsearchBackendBuilder::new()
            .provider("brave")
            .build()
            .expect_err("brave needs api_key");
        match err {
            WebsearchConfigError::MissingApiKey { provider } => {
                assert_eq!(provider, "brave");
            }
            other => panic!("expected MissingApiKey, got {other:?}"),
        }
    }

    #[test]
    fn builder_google_requires_api_key_and_cx() {
        let err = WebsearchBackendBuilder::new()
            .provider("google")
            .build()
            .expect_err("google needs api_key");
        assert!(matches!(err, WebsearchConfigError::MissingApiKey { .. }));
        let err = WebsearchBackendBuilder::new()
            .provider("google")
            .api_key("k")
            .build()
            .expect_err("google needs cx");
        assert!(matches!(err, WebsearchConfigError::MissingCx { .. }));
        WebsearchBackendBuilder::new()
            .provider("google")
            .api_key("k")
            .cx("c")
            .build()
            .expect("google with api_key + cx builds");
    }

    #[test]
    fn builder_searxng_requires_endpoint() {
        let err = WebsearchBackendBuilder::new()
            .provider("searxng")
            .build()
            .expect_err("searxng needs endpoint");
        match err {
            WebsearchConfigError::MissingEndpoint { provider } => {
                assert_eq!(provider, "searxng");
            }
            other => panic!("expected MissingEndpoint, got {other:?}"),
        }
        WebsearchBackendBuilder::new()
            .provider("searxng")
            .endpoint("https://searx.example/")
            .build()
            .expect("searxng with endpoint builds");
    }

    #[test]
    fn builder_duckduckgo_builds_with_no_credentials() {
        WebsearchBackendBuilder::new()
            .provider("duckduckgo")
            .build()
            .expect("duckduckgo builds");
    }

    #[test]
    fn builder_unknown_provider_returns_typed_error() {
        let err = WebsearchBackendBuilder::new()
            .provider("not-a-provider")
            .build()
            .expect_err("unknown provider");
        match err {
            WebsearchConfigError::UnknownProvider { provider } => {
                assert_eq!(provider, "not-a-provider");
            }
            other => panic!("expected UnknownProvider, got {other:?}"),
        }
    }

    #[test]
    fn config_table_with_env_interpolation_resolves_to_builder() {
        let var = "AILLY_TEST_WEB_BUILDER_API_KEY";
        unsafe {
            std::env::set_var(var, "resolved-key");
        }
        let toml_text = r#"
[tools.web_search]
provider = "brave"
api_key = "${AILLY_TEST_WEB_BUILDER_API_KEY}"
"#;
        let cfg = WebToolsConfig::from_toml(&parse(toml_text)).expect("parse interpolated config");
        unsafe {
            std::env::remove_var(var);
        }
        let builder = cfg.web_search.expect("web_search builder present");
        assert_eq!(builder.api_key.as_deref(), Some("resolved-key"));
        assert_eq!(builder.provider.as_deref(), Some("brave"));
    }

    #[test]
    fn config_table_with_undefined_env_var_returns_typed_error() {
        let toml_text = r#"
[tools.web_search]
provider = "brave"
api_key = "${AILLY_TEST_WEB_UNDEFINED_DO_NOT_SET_ME}"
"#;
        let err =
            WebToolsConfig::from_toml(&parse(toml_text)).expect_err("undefined env var fails");
        match err {
            WebsearchConfigError::UndefinedEnvVar { name } => {
                assert_eq!(name, "AILLY_TEST_WEB_UNDEFINED_DO_NOT_SET_ME");
            }
            other => panic!("expected UndefinedEnvVar, got {other:?}"),
        }
    }

    #[test]
    fn config_table_with_unknown_provider_returns_typed_error() {
        let toml_text = r#"
[tools.web_search]
provider = "totally-fake"
"#;
        let cfg = WebToolsConfig::from_toml(&parse(toml_text)).expect("parse config table");
        let err = cfg
            .web_search
            .expect("builder present")
            .build()
            .expect_err("unknown provider");
        assert!(matches!(err, WebsearchConfigError::UnknownProvider { .. }));
    }

    #[test]
    fn config_table_absent_from_toml_yields_none_web_search() {
        let toml_text = r#"
[other]
key = "value"
"#;
        let cfg = WebToolsConfig::from_toml(&parse(toml_text)).expect("parse without table");
        assert!(cfg.web_search.is_none());
    }

    #[test]
    fn config_table_missing_provider_field_returns_typed_error() {
        let toml_text = r#"
[tools.web_search]
api_key = "k"
"#;
        let err = WebToolsConfig::from_toml(&parse(toml_text)).expect_err("missing provider");
        assert!(matches!(err, WebsearchConfigError::MissingProvider));
    }

    #[test]
    fn interpolate_passthrough_for_literal_strings() {
        assert_eq!(interpolate_env("plain").unwrap(), "plain");
        assert_eq!(interpolate_env("a/b/c").unwrap(), "a/b/c");
        assert_eq!(interpolate_env("").unwrap(), "");
    }
}
