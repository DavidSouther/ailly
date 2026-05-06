use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use rig::wasm_compat::WasmBoxedFuture;
use serde_json::Value;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Classification {
    Read,
    Safe,
    Unsafe,
}

impl fmt::Display for Classification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Classification::Read => "read",
            Classification::Safe => "safe",
            Classification::Unsafe => "unsafe",
        };
        f.write_str(name)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Decision {
    Allow,
    Deny { reason: String },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BasePolicy {
    Allow,
    Deny,
}

pub trait Classifier: Send + Sync {
    fn classify(&self, args: &Value) -> Classification;
}

#[async_trait]
pub trait PermissionBackend: Send + Sync {
    async fn decide(&self, name: &str, classification: Classification) -> Decision;
}

pub struct ConstClassifier(pub Classification);

impl Classifier for ConstClassifier {
    fn classify(&self, _args: &Value) -> Classification {
        self.0
    }
}

pub struct AllowAllBackend;

#[async_trait]
impl PermissionBackend for AllowAllBackend {
    async fn decide(&self, _name: &str, _classification: Classification) -> Decision {
        Decision::Allow
    }
}

pub struct DenyAllBackend;

#[async_trait]
impl PermissionBackend for DenyAllBackend {
    async fn decide(&self, _name: &str, _classification: Classification) -> Decision {
        Decision::Deny {
            reason: "all tool calls denied by policy".to_string(),
        }
    }
}

pub struct ClassRouterBackend {
    pub on_read: BasePolicy,
    pub on_safe: BasePolicy,
    pub on_unsafe: BasePolicy,
}

#[async_trait]
impl PermissionBackend for ClassRouterBackend {
    async fn decide(&self, _name: &str, classification: Classification) -> Decision {
        let policy = match classification {
            Classification::Read => self.on_read,
            Classification::Safe => self.on_safe,
            Classification::Unsafe => self.on_unsafe,
        };
        match policy {
            BasePolicy::Allow => Decision::Allow,
            BasePolicy::Deny => Decision::Deny {
                reason: format!("policy denies {classification}-classified calls"),
            },
        }
    }
}

pub struct PermissionGated {
    name: String,
    inner: Arc<dyn ToolDyn>,
    classifier: Arc<dyn Classifier>,
    backend: Arc<dyn PermissionBackend>,
}

impl PermissionGated {
    pub fn new(
        name: impl Into<String>,
        inner: Arc<dyn ToolDyn>,
        classifier: Arc<dyn Classifier>,
        backend: Arc<dyn PermissionBackend>,
    ) -> Self {
        Self {
            name: name.into(),
            inner,
            classifier,
            backend,
        }
    }
}

impl ToolDyn for PermissionGated {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn definition<'a>(&'a self, prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        self.inner.definition(prompt)
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let parsed: Value = serde_json::from_str(&args).unwrap_or(Value::Null);
            let class = self.classifier.classify(&parsed);
            match self.backend.decide(&self.name, class).await {
                Decision::Allow => self.inner.call(args).await,
                Decision::Deny { reason } => Ok(format!("permission denied: {reason}")),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rig::tool::Tool;

    use crate::engine::{HashMapRegistry, ToolRegistry};

    struct EchoTool;

    #[derive(Debug, serde::Deserialize)]
    struct EchoArgs {
        msg: String,
    }

    #[derive(Debug, thiserror::Error)]
    #[error("echo error")]
    struct EchoError;

    impl Tool for EchoTool {
        const NAME: &'static str = "permissions.test.echo";
        type Error = EchoError;
        type Args = EchoArgs;
        type Output = String;

        async fn definition(&self, _prompt: String) -> ToolDefinition {
            ToolDefinition {
                name: Self::NAME.to_string(),
                description: "echoes its msg arg".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "msg": { "type": "string" } },
                    "required": ["msg"],
                }),
            }
        }

        async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
            Ok(args.msg)
        }
    }

    struct CountingTool {
        calls: Arc<AtomicUsize>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct CountingArgs {}

    #[derive(Debug, thiserror::Error)]
    #[error("counting tool error")]
    struct CountingError;

    impl Tool for CountingTool {
        const NAME: &'static str = "permissions.test.counting";
        type Error = CountingError;
        type Args = CountingArgs;
        type Output = String;

        async fn definition(&self, _prompt: String) -> ToolDefinition {
            ToolDefinition {
                name: Self::NAME.to_string(),
                description: "counts invocations".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            }
        }

        async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok("inner-called".to_string())
        }
    }

    struct NullAssertingClassifier;

    impl Classifier for NullAssertingClassifier {
        fn classify(&self, args: &Value) -> Classification {
            assert_eq!(*args, Value::Null, "classifier sees Null on parse failure");
            Classification::Unsafe
        }
    }

    fn echo_args() -> String {
        serde_json::to_string(&serde_json::json!({ "msg": "hello" })).unwrap()
    }

    #[test]
    fn classification_display_returns_snake_case_for_each_variant() {
        assert_eq!(Classification::Read.to_string(), "read");
        assert_eq!(Classification::Safe.to_string(), "safe");
        assert_eq!(Classification::Unsafe.to_string(), "unsafe");
    }

    #[test]
    fn classification_round_trips_through_hashmap() {
        let mut map: HashMap<Classification, &'static str> = HashMap::new();
        map.insert(Classification::Read, "r");
        map.insert(Classification::Safe, "s");
        map.insert(Classification::Unsafe, "u");

        assert_eq!(map.get(&Classification::Read), Some(&"r"));
        assert_eq!(map.get(&Classification::Safe), Some(&"s"));
        assert_eq!(map.get(&Classification::Unsafe), Some(&"u"));
    }

    #[test]
    fn decision_deny_carries_reason_and_supports_equality() {
        let denied = Decision::Deny {
            reason: "no read".to_string(),
        };

        assert_eq!(
            denied,
            Decision::Deny {
                reason: "no read".to_string()
            }
        );
        assert_ne!(denied, Decision::Allow);
        assert_ne!(
            denied,
            Decision::Deny {
                reason: "different".to_string()
            }
        );
    }

    #[test]
    fn base_policy_supports_equality_and_copy() {
        let allow = BasePolicy::Allow;
        let copied = allow;

        assert_eq!(allow, copied);
        assert_ne!(BasePolicy::Allow, BasePolicy::Deny);
    }

    #[test]
    fn const_classifier_returns_captured_value_for_arbitrary_args() {
        let classifier = ConstClassifier(Classification::Safe);

        let object = serde_json::json!({"k": 1, "v": [true, false]});
        let array = serde_json::json!([1, 2, 3]);
        let null = Value::Null;
        let number = serde_json::json!(42);
        let string = serde_json::json!("hello");
        let boolean = serde_json::json!(true);

        assert_eq!(classifier.classify(&object), Classification::Safe);
        assert_eq!(classifier.classify(&array), Classification::Safe);
        assert_eq!(classifier.classify(&null), Classification::Safe);
        assert_eq!(classifier.classify(&number), Classification::Safe);
        assert_eq!(classifier.classify(&string), Classification::Safe);
        assert_eq!(classifier.classify(&boolean), Classification::Safe);
    }

    #[tokio::test]
    async fn allow_all_backend_returns_allow_for_every_variant() {
        let backend = AllowAllBackend;

        for class in [
            Classification::Read,
            Classification::Safe,
            Classification::Unsafe,
        ] {
            let decision = backend.decide("any.tool", class).await;
            assert_eq!(decision, Decision::Allow, "class {class}");
        }
    }

    #[tokio::test]
    async fn deny_all_backend_returns_deny_with_non_empty_reason_for_every_variant() {
        let backend = DenyAllBackend;

        for class in [
            Classification::Read,
            Classification::Safe,
            Classification::Unsafe,
        ] {
            let decision = backend.decide("any.tool", class).await;
            match decision {
                Decision::Deny { reason } => {
                    assert!(!reason.is_empty(), "deny reason must not be empty");
                }
                other => panic!("expected Deny for class {class}, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn class_router_backend_routes_each_field_combination_with_bucket_in_reason() {
        let combos = [
            (BasePolicy::Allow, BasePolicy::Allow, BasePolicy::Allow),
            (BasePolicy::Allow, BasePolicy::Allow, BasePolicy::Deny),
            (BasePolicy::Allow, BasePolicy::Deny, BasePolicy::Allow),
            (BasePolicy::Allow, BasePolicy::Deny, BasePolicy::Deny),
            (BasePolicy::Deny, BasePolicy::Allow, BasePolicy::Allow),
            (BasePolicy::Deny, BasePolicy::Allow, BasePolicy::Deny),
            (BasePolicy::Deny, BasePolicy::Deny, BasePolicy::Allow),
            (BasePolicy::Deny, BasePolicy::Deny, BasePolicy::Deny),
        ];

        for (on_read, on_safe, on_unsafe) in combos {
            let backend = ClassRouterBackend {
                on_read,
                on_safe,
                on_unsafe,
            };

            for (class, policy, bucket) in [
                (Classification::Read, on_read, "read"),
                (Classification::Safe, on_safe, "safe"),
                (Classification::Unsafe, on_unsafe, "unsafe"),
            ] {
                let decision = backend.decide("any.tool", class).await;
                match (policy, decision) {
                    (BasePolicy::Allow, Decision::Allow) => {}
                    (BasePolicy::Deny, Decision::Deny { reason }) => {
                        assert!(
                            reason.contains(bucket),
                            "deny reason for {class} must mention bucket {bucket}; got {reason}"
                        );
                    }
                    (policy, decision) => {
                        panic!("policy {policy:?} for class {class} produced {decision:?}")
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn permission_gated_call_on_allow_forwards_byte_identical() {
        let inner: Arc<dyn ToolDyn> = Arc::new(EchoTool);
        let baseline = inner.call(echo_args()).await.expect("baseline ok");

        let gated: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
            EchoTool::NAME,
            Arc::clone(&inner),
            Arc::new(ConstClassifier(Classification::Read)),
            Arc::new(AllowAllBackend),
        ));

        let result = gated.call(echo_args()).await.expect("gated ok");
        assert_eq!(result, baseline);
    }

    #[tokio::test]
    async fn permission_gated_call_on_deny_synthesizes_refusal_and_skips_inner() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner: Arc<dyn ToolDyn> = Arc::new(CountingTool {
            calls: Arc::clone(&calls),
        });

        let gated: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
            CountingTool::NAME,
            inner,
            Arc::new(ConstClassifier(Classification::Read)),
            Arc::new(DenyAllBackend),
        ));

        let result = gated.call("{}".to_string()).await.expect("gated ok");
        assert!(
            result.starts_with("permission denied: "),
            "expected synthesized refusal; got {result}"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "inner must not run on deny"
        );
    }

    #[tokio::test]
    async fn permission_gated_call_with_malformed_json_classifies_null_and_denies_deterministically()
     {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner: Arc<dyn ToolDyn> = Arc::new(CountingTool {
            calls: Arc::clone(&calls),
        });

        let gated: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
            CountingTool::NAME,
            inner,
            Arc::new(NullAssertingClassifier),
            Arc::new(DenyAllBackend),
        ));

        let result = gated
            .call("not json at all {".to_string())
            .await
            .expect("gated ok");
        assert!(result.starts_with("permission denied: "));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn permission_gated_definition_round_trips_inner_definition() {
        let inner: Arc<dyn ToolDyn> = Arc::new(EchoTool);
        let inner_def = inner.definition(String::new()).await;

        let gated: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
            EchoTool::NAME,
            Arc::clone(&inner),
            Arc::new(ConstClassifier(Classification::Read)),
            Arc::new(AllowAllBackend),
        ));

        let gated_def = gated.definition(String::new()).await;
        assert_eq!(gated_def.name, inner_def.name);
        assert_eq!(gated_def.description, inner_def.description);
        assert_eq!(gated_def.parameters, inner_def.parameters);
    }

    #[tokio::test]
    async fn permission_gated_round_trips_through_registry() {
        let inner: Arc<dyn ToolDyn> = Arc::new(EchoTool);
        let gated: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
            EchoTool::NAME,
            Arc::clone(&inner),
            Arc::new(ConstClassifier(Classification::Read)),
            Arc::new(AllowAllBackend),
        ));

        let mut registry = HashMapRegistry::default();
        registry.insert(EchoTool::NAME, gated);
        let registry: Arc<dyn ToolRegistry> = Arc::new(registry);

        let resolved = registry.resolve(EchoTool::NAME).expect("resolves");
        let result = resolved.call(echo_args()).await.expect("resolved ok");
        assert_eq!(result, "hello");
    }
}
