use std::time::Duration;

use futures::StreamExt;
use rig::completion::ToolDefinition;
use rig::tool::Tool;

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("invalid URL {url}")]
    InvalidUrl {
        url: String,
        #[source]
        source: url::ParseError,
    },
    #[error("transport error")]
    Transport {
        #[source]
        source: reqwest::Error,
    },
    #[error("disallowed content type {content_type}")]
    DisallowedContentType { content_type: String },
    #[error("body exceeded {limit} bytes")]
    BodyTooLarge { limit: usize },
    #[error("decoding response body as UTF-8")]
    Decode {
        #[source]
        source: std::str::Utf8Error,
    },
    #[error("building HTTP client")]
    BuildClient {
        #[source]
        source: reqwest::Error,
    },
}

#[derive(Debug, serde::Deserialize)]
pub struct WebFetchArgs {
    pub url: String,
}

pub struct WebFetch {
    client: reqwest::Client,
    max_body_bytes: usize,
    allowed_content_types: Vec<String>,
}

impl WebFetch {
    pub const NAME: &'static str = "web.fetch";
    pub const DEFAULT_MAX_BODY_BYTES: usize = 1 << 20;

    pub fn new() -> Result<Self, FetchError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(format!("ailly/{}", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|source| FetchError::BuildClient { source })?;
        Ok(Self {
            client,
            max_body_bytes: Self::DEFAULT_MAX_BODY_BYTES,
            allowed_content_types: vec![
                "text/".to_string(),
                "application/json".to_string(),
                "application/xml".to_string(),
                "application/xhtml+xml".to_string(),
            ],
        })
    }
}

impl Tool for WebFetch {
    const NAME: &'static str = "web.fetch";

    type Error = FetchError;
    type Args = WebFetchArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetch a known URL over HTTP(S) and return the \
                response body as a UTF-8 string. The fetch is bounded by \
                a body-size cap, an allow-list of textual content types, \
                and a redirect limit."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Absolute URL to fetch."
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let parsed = reqwest::Url::parse(&args.url).map_err(|source| FetchError::InvalidUrl {
            url: args.url.clone(),
            source,
        })?;
        let response = self
            .client
            .get(parsed)
            .send()
            .await
            .map_err(|source| FetchError::Transport { source })?;

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let allowed = self
            .allowed_content_types
            .iter()
            .any(|prefix| content_type.starts_with(prefix.as_str()));
        if !allowed {
            return Err(FetchError::DisallowedContentType { content_type });
        }

        let limit = self.max_body_bytes;
        let mut buf: Vec<u8> = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|source| FetchError::Transport { source })?;
            if buf.len().saturating_add(chunk.len()) > limit {
                return Err(FetchError::BodyTooLarge { limit });
            }
            buf.extend_from_slice(&chunk);
        }
        String::from_utf8(buf).map_err(|e| FetchError::Decode {
            source: e.utf8_error(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use rig::tool::ToolDyn;

    use crate::knowledge::permissions::{AllowAllBackend, DenyAllBackend, PermissionBackend, PermissionGated};

    use super::super::classifier::WebFetchClassifier;

    async fn make_server() -> mockito::ServerGuard {
        mockito::Server::new_async().await
    }

    #[tokio::test]
    async fn web_fetch_returns_body_for_text_html_content_type() {
        let mut server = make_server().await;
        let body = "<!doctype html>\n<title>x</title>\n";
        let _m = server
            .mock("GET", "/p")
            .with_status(200)
            .with_header("content-type", "text/html; charset=utf-8")
            .with_body(body)
            .create_async()
            .await;
        let url = format!("{}/p", server.url());
        let tool = WebFetch::new().expect("build");
        let got = Tool::call(&tool, WebFetchArgs { url })
            .await
            .expect("fetch ok");
        assert_eq!(got, body);
    }

    #[tokio::test]
    async fn web_fetch_returns_body_for_application_json_content_type() {
        let mut server = make_server().await;
        let body = r#"{"hello":"world"}"#;
        let _m = server
            .mock("GET", "/j")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;
        let url = format!("{}/j", server.url());
        let tool = WebFetch::new().expect("build");
        let got = Tool::call(&tool, WebFetchArgs { url })
            .await
            .expect("fetch ok");
        assert_eq!(got, body);
    }

    #[tokio::test]
    async fn web_fetch_rejects_disallowed_content_type() {
        let mut server = make_server().await;
        let _m = server
            .mock("GET", "/i")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(&[0u8, 1, 2, 3][..])
            .create_async()
            .await;
        let url = format!("{}/i", server.url());
        let tool = WebFetch::new().expect("build");
        let err = Tool::call(&tool, WebFetchArgs { url })
            .await
            .expect_err("rejects image/png");
        match err {
            FetchError::DisallowedContentType { content_type } => {
                assert!(content_type.starts_with("image/png"));
            }
            other => panic!("expected DisallowedContentType, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn web_fetch_caps_body_at_max_bytes_and_returns_typed_error() {
        let mut server = make_server().await;
        let big = "x".repeat(64);
        let _m = server
            .mock("GET", "/big")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(&big)
            .create_async()
            .await;
        let url = format!("{}/big", server.url());
        let tool = WebFetch {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .user_agent("ailly-test/0".to_string())
                .build()
                .unwrap(),
            max_body_bytes: 16,
            allowed_content_types: vec!["text/".to_string()],
        };
        let err = Tool::call(&tool, WebFetchArgs { url })
            .await
            .expect_err("body cap fires");
        assert!(matches!(err, FetchError::BodyTooLarge { limit: 16 }));
    }

    #[tokio::test]
    async fn web_fetch_returns_decode_error_for_invalid_utf8() {
        let mut server = make_server().await;
        let bad = vec![0xff, 0xfe, 0xfd];
        let _m = server
            .mock("GET", "/u")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(bad)
            .create_async()
            .await;
        let url = format!("{}/u", server.url());
        let tool = WebFetch::new().expect("build");
        let err = Tool::call(&tool, WebFetchArgs { url })
            .await
            .expect_err("decode fails");
        assert!(matches!(err, FetchError::Decode { .. }));
    }

    #[tokio::test]
    async fn web_fetch_invalid_url_returns_invalid_url_error_at_call_time() {
        let tool = WebFetch::new().expect("build");
        let err = Tool::call(
            &tool,
            WebFetchArgs {
                url: "not-a-url".to_string(),
            },
        )
        .await
        .expect_err("parse fails");
        match err {
            FetchError::InvalidUrl { url, .. } => assert_eq!(url, "not-a-url"),
            other => panic!("expected InvalidUrl, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn web_fetch_includes_descriptive_user_agent() {
        let mut server = make_server().await;
        let _m = server
            .mock("GET", "/ua")
            .match_header(
                "user-agent",
                mockito::Matcher::Regex("ailly/.+".to_string()),
            )
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("ok")
            .expect(1)
            .create_async()
            .await;
        let url = format!("{}/ua", server.url());
        let tool = WebFetch::new().expect("build");
        let got = Tool::call(&tool, WebFetchArgs { url })
            .await
            .expect("fetch ok");
        assert_eq!(got, "ok");
        _m.assert_async().await;
    }

    #[tokio::test]
    async fn web_fetch_follows_redirects_within_default_limit() {
        let mut server = make_server().await;
        let _redir = server
            .mock("GET", "/start")
            .with_status(302)
            .with_header("location", "/end")
            .create_async()
            .await;
        let _end = server
            .mock("GET", "/end")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("arrived")
            .create_async()
            .await;
        let url = format!("{}/start", server.url());
        let tool = WebFetch::new().expect("build");
        let got = Tool::call(&tool, WebFetchArgs { url })
            .await
            .expect("redirect followed");
        assert_eq!(got, "arrived");
    }

    #[tokio::test]
    async fn web_fetch_propagates_transport_error_on_connection_refused() {
        let tool = WebFetch::new().expect("build");
        let err = Tool::call(
            &tool,
            WebFetchArgs {
                url: "http://127.0.0.1:1/closed".to_string(),
            },
        )
        .await
        .expect_err("connection refused");
        assert!(matches!(err, FetchError::Transport { .. }));
    }

    #[tokio::test]
    async fn web_fetch_through_permission_gated_with_allow_all_backend_forwards_to_inner() {
        let mut server = make_server().await;
        let body = "gated allow body";
        let _m = server
            .mock("GET", "/g")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(body)
            .expect(2)
            .create_async()
            .await;
        let url = format!("{}/g", server.url());

        let bare: Arc<dyn ToolDyn> = Arc::new(WebFetch::new().expect("build"));
        let bare_args = serde_json::json!({ "url": url }).to_string();
        let bare_body = bare.call(bare_args.clone()).await.expect("bare ok");

        let permission_backend: Arc<dyn PermissionBackend> = Arc::new(AllowAllBackend);
        let gated: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
            WebFetch::NAME,
            Arc::new(WebFetch::new().expect("build")) as Arc<dyn ToolDyn>,
            Arc::new(WebFetchClassifier),
            permission_backend,
        ));
        let gated_body = gated.call(bare_args).await.expect("gated ok");
        assert_eq!(bare_body, gated_body);
        _m.assert_async().await;
    }

    #[tokio::test]
    async fn web_fetch_through_permission_gated_with_deny_all_backend_synthesizes_refusal() {
        let mut server = make_server().await;
        let _m = server
            .mock("GET", "/x")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("must not be hit")
            .expect(0)
            .create_async()
            .await;
        let url = format!("{}/x", server.url());
        let permission_backend: Arc<dyn PermissionBackend> = Arc::new(DenyAllBackend);
        let gated: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
            WebFetch::NAME,
            Arc::new(WebFetch::new().expect("build")) as Arc<dyn ToolDyn>,
            Arc::new(WebFetchClassifier),
            permission_backend,
        ));
        let body = gated
            .call(serde_json::json!({ "url": url }).to_string())
            .await
            .expect("gated returns synthesized refusal");
        assert!(body.contains("permission denied"));
        _m.assert_async().await;
    }
}
