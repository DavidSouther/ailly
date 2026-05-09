use std::collections::BTreeMap;
use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;

#[derive(Debug, thiserror::Error)]
pub enum KnowledgeError {
    #[error("no knowledge backend configured")]
    NoBackendConfigured,
    #[error("no answer recorded for question {question:?}")]
    NoAnswer { question: String },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[async_trait::async_trait]
pub trait KnowledgeBase: Send + Sync {
    async fn ask(&self, question: &str, default: Option<&str>) -> Result<String, KnowledgeError>;
}

pub struct RefuseKnowledgeBase;

#[async_trait::async_trait]
impl KnowledgeBase for RefuseKnowledgeBase {
    async fn ask(&self, _question: &str, _default: Option<&str>) -> Result<String, KnowledgeError> {
        Err(KnowledgeError::NoBackendConfigured)
    }
}

pub struct DefaultOnlyKnowledgeBase;

#[async_trait::async_trait]
impl KnowledgeBase for DefaultOnlyKnowledgeBase {
    async fn ask(&self, question: &str, default: Option<&str>) -> Result<String, KnowledgeError> {
        match default {
            Some(d) => Ok(d.to_string()),
            None => Err(KnowledgeError::NoAnswer {
                question: question.to_string(),
            }),
        }
    }
}

pub struct MapKnowledgeBase {
    answers: BTreeMap<String, String>,
}

impl MapKnowledgeBase {
    pub fn new<I, K, V>(answers: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            answers: answers
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }
}

#[async_trait::async_trait]
impl KnowledgeBase for MapKnowledgeBase {
    async fn ask(&self, question: &str, _default: Option<&str>) -> Result<String, KnowledgeError> {
        self.answers
            .get(question)
            .cloned()
            .ok_or_else(|| KnowledgeError::NoAnswer {
                question: question.to_string(),
            })
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ClarifyArgs {
    pub question: String,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ClarifyError {
    #[error(transparent)]
    Knowledge(#[from] KnowledgeError),
}

pub struct ClarifyTool {
    kb: Arc<dyn KnowledgeBase>,
}

impl ClarifyTool {
    pub const NAME: &'static str = "user-clarify";

    pub fn new(kb: Arc<dyn KnowledgeBase>) -> Self {
        Self { kb }
    }
}

impl Tool for ClarifyTool {
    const NAME: &'static str = "user-clarify";

    type Error = ClarifyError;
    type Args = ClarifyArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get clarification on a single focused question. \
                Returns the answer (or a previously recorded answer to the \
                same question). Use exactly one question per call. Do not \
                concatenate."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question to ask the user. \
                            One concise question per call."
                    },
                    "default": {
                        "type": "string",
                        "description": "Optional suggested answer the \
                            backend may surface during elicitation, or \
                            use as a fallback."
                    }
                },
                "required": ["question"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(self.kb.ask(&args.question, args.default.as_deref()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn knowledge_base_is_dyn_safe() {
        let kb: Arc<dyn KnowledgeBase> = Arc::new(RefuseKnowledgeBase);
        let result = kb.ask("q", None).await;
        assert!(matches!(result, Err(KnowledgeError::NoBackendConfigured)));
    }

    #[tokio::test]
    async fn refuse_knowledge_base_errors_without_default() {
        let kb = RefuseKnowledgeBase;
        let result = kb.ask("anything", None).await;
        assert!(matches!(result, Err(KnowledgeError::NoBackendConfigured)));
    }

    #[tokio::test]
    async fn refuse_knowledge_base_errors_with_default() {
        let kb = RefuseKnowledgeBase;
        let result = kb.ask("anything", Some("foo")).await;
        assert!(matches!(result, Err(KnowledgeError::NoBackendConfigured)));
    }

    #[tokio::test]
    async fn default_only_returns_default_when_supplied() {
        let kb = DefaultOnlyKnowledgeBase;
        let result = kb.ask("q", Some("dflt")).await;
        assert_eq!(result.unwrap(), "dflt");
    }

    #[tokio::test]
    async fn default_only_errors_with_no_answer_when_default_absent() {
        let kb = DefaultOnlyKnowledgeBase;
        let result = kb.ask("q", None).await;
        match result {
            Err(KnowledgeError::NoAnswer { question }) => assert_eq!(question, "q"),
            other => panic!("expected NoAnswer carrying question, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn map_knowledge_base_returns_recorded_answer_on_hit() {
        let kb = MapKnowledgeBase::new([("q1", "a1")]);
        let result = kb.ask("q1", None).await;
        assert_eq!(result.unwrap(), "a1");
    }

    #[tokio::test]
    async fn map_knowledge_base_errors_with_no_answer_on_miss() {
        let kb = MapKnowledgeBase::new([("q1", "a1")]);
        let result = kb.ask("q2", None).await;
        match result {
            Err(KnowledgeError::NoAnswer { question }) => assert_eq!(question, "q2"),
            other => panic!("expected NoAnswer carrying question, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn clarify_tool_round_trips_question_and_default_through_kb() {
        let kb: Arc<dyn KnowledgeBase> = Arc::new(MapKnowledgeBase::new([("q", "a")]));
        let tool = ClarifyTool::new(kb);
        let raw = r#"{"question":"q","default":"d"}"#;
        let args: ClarifyArgs = serde_json::from_str(raw).expect("deserialize args");

        let answer = tool.call(args).await.expect("call returns recorded answer");

        assert_eq!(answer, "a");
    }

    #[tokio::test]
    async fn clarify_tool_accepts_args_without_default_field() {
        let kb: Arc<dyn KnowledgeBase> = Arc::new(MapKnowledgeBase::new([("q", "a")]));
        let tool = ClarifyTool::new(kb);
        let args: ClarifyArgs =
            serde_json::from_str(r#"{"question":"q"}"#).expect("deserialize args");
        assert!(args.default.is_none());

        let answer = tool
            .call(args)
            .await
            .expect("call dispatches with default = None");

        assert_eq!(answer, "a");
    }

    #[test]
    fn clarify_args_missing_question_field_fails_at_deserialize() {
        let result = serde_json::from_str::<ClarifyArgs>(r#"{"unrelated":"field"}"#);
        let err = result.expect_err("missing question must fail deserialization");
        assert!(
            err.to_string().contains("question"),
            "deserialize error should mention the missing question field; got: {err}"
        );
    }

    #[test]
    fn clarify_error_no_answer_display_carries_question_literal() {
        let err = ClarifyError::Knowledge(KnowledgeError::NoAnswer {
            question: "what".to_string(),
        });
        let rendered = err.to_string();
        assert!(
            rendered.contains("what"),
            "ClarifyError Display should carry the question literal; got: {rendered}"
        );
    }

    #[test]
    fn clarify_tool_name_is_user_clarify() {
        assert_eq!(ClarifyTool::NAME, "user-clarify");
        assert_eq!(<ClarifyTool as Tool>::NAME, "user-clarify");
    }
}
