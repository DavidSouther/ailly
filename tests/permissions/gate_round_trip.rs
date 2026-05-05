//! Feature test for the permissions infrastructure slice.
//!
//! User story (narrative):
//!
//! A harness author wraps `FsAbsent` in a `PermissionGated` decorator. They
//! pair it with a `ConstClassifier(Read)` and a `ClassRouterBackend` whose
//! initial routing allows `Read`-classified calls. They erase the gated tool
//! to `Arc<dyn ToolDyn>`, register it in `HashMapRegistry` under the same
//! `"fs.absent"` name, resolve it back through `ToolRegistry::resolve`, and
//! call it via `ToolDyn::call` against a virtual filesystem prepared so the
//! bare `FsAbsent` would return `"cleared"`. The harness expects the gated
//! result to equal the unwrapped `FsAbsent` result byte-for-byte, and the
//! tool's `definition` to round-trip unchanged through the decorator.
//!
//! The same harness then composes a second wrap of the same `FsAbsent` whose
//! backend denies `Read`-classified calls (`on_read = Deny`). They register
//! the deny-flavored decorator under a second name, resolve it through the
//! same registry, and call it with identical args. The harness expects the
//! result to be `Ok("permission denied: ...")` carrying the bucket name in
//! the reason, the inner `FsAbsent` to never observe the call (asserted by a
//! call-counting `ToolDyn` test double inserted in place of `FsAbsent` on a
//! third wrap), and the tool definition surfaced through the decorator to
//! still match the inner tool's definition. The registry interface is
//! unchanged: every consumer still sees `Arc<dyn ToolDyn>`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rig::completion::ToolDefinition;
use rig::tool::{Tool, ToolDyn};
use vfs::{MemoryFS, VfsPath};

use ailly::engine::{HashMapRegistry, ToolRegistry};
use ailly::permissions::{
    BasePolicy, ClassRouterBackend, Classification, ConstClassifier, PermissionGated,
};
use ailly::tools::FsAbsent;

fn fs_absent_args() -> String {
    serde_json::to_string(&serde_json::json!({
        "path": "design.md",
        "needle": "*Draft",
    }))
    .expect("serialize fs.absent args")
}

fn root_with_cleared_design() -> VfsPath {
    let fs: VfsPath = MemoryFS::new().into();
    let root = fs.join("root").expect("join root");
    root.create_dir_all().expect("create root dir");
    let design = root.join("design.md").expect("join design.md");
    design
        .create_file()
        .expect("create design.md")
        .write_all(b"# Design\n\nbody\n")
        .expect("write design.md body");
    root
}

struct CountingTool {
    calls: Arc<AtomicUsize>,
}

impl CountingTool {
    fn new() -> (Self, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                calls: calls.clone(),
            },
            calls,
        )
    }
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
            description: "test double that counts inner invocations".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok("inner-called".to_string())
    }
}

#[tokio::test]
async fn permission_gate_round_trips_through_registry_and_intercepts_deny() {
    let root = root_with_cleared_design();
    let inner: Arc<dyn ToolDyn> = Arc::new(FsAbsent::new(root.clone()));

    let baseline = inner
        .call(fs_absent_args())
        .await
        .expect("baseline FsAbsent::call succeeds");
    assert_eq!(
        baseline, "cleared",
        "baseline assumption: bare FsAbsent returns \"cleared\" against this fixture"
    );

    let inner_definition = inner.definition(String::new()).await;

    let allow_gate: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
        FsAbsent::NAME,
        Arc::clone(&inner),
        Arc::new(ConstClassifier(Classification::Read)),
        Arc::new(ClassRouterBackend {
            on_read: BasePolicy::Allow,
            on_safe: BasePolicy::Allow,
            on_unsafe: BasePolicy::Deny,
        }),
    ));

    let deny_gate: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
        "fs.absent.denied",
        Arc::clone(&inner),
        Arc::new(ConstClassifier(Classification::Read)),
        Arc::new(ClassRouterBackend {
            on_read: BasePolicy::Deny,
            on_safe: BasePolicy::Deny,
            on_unsafe: BasePolicy::Deny,
        }),
    ));

    let (counting_tool, counting_calls) = CountingTool::new();
    let counting_inner: Arc<dyn ToolDyn> = Arc::new(counting_tool);
    let counting_gate: Arc<dyn ToolDyn> = Arc::new(PermissionGated::new(
        "fs.absent.counted",
        counting_inner,
        Arc::new(ConstClassifier(Classification::Read)),
        Arc::new(ClassRouterBackend {
            on_read: BasePolicy::Deny,
            on_safe: BasePolicy::Deny,
            on_unsafe: BasePolicy::Deny,
        }),
    ));

    let mut registry = HashMapRegistry::default();
    registry.insert(FsAbsent::NAME, allow_gate);
    registry.insert("fs.absent.denied", deny_gate);
    registry.insert("fs.absent.counted", counting_gate);
    let registry: Arc<dyn ToolRegistry> = Arc::new(registry);

    let resolved_allow = registry
        .resolve(FsAbsent::NAME)
        .expect("registry resolves the allow-gated tool by name");
    let allow_result = resolved_allow
        .call(fs_absent_args())
        .await
        .expect("allow-gated call succeeds");
    assert_eq!(
        allow_result, baseline,
        "Allow path forwards verbatim to the inner tool"
    );

    let resolved_definition = resolved_allow.definition(String::new()).await;
    assert_eq!(
        resolved_definition.name, inner_definition.name,
        "decorator surfaces inner tool's definition name unchanged"
    );
    assert_eq!(
        resolved_definition.description, inner_definition.description,
        "decorator surfaces inner tool's definition description unchanged"
    );
    assert_eq!(
        resolved_definition.parameters, inner_definition.parameters,
        "decorator surfaces inner tool's definition parameters unchanged"
    );

    let resolved_deny = registry
        .resolve("fs.absent.denied")
        .expect("registry resolves the deny-gated tool by name");
    let deny_result = resolved_deny
        .call(fs_absent_args())
        .await
        .expect("deny gate returns Ok carrying a synthesized refusal");
    assert!(
        deny_result.starts_with("permission denied: "),
        "deny path returns the synthetic refusal prefix; got: {deny_result}"
    );
    assert!(
        deny_result.contains("read"),
        "deny reason names the classification bucket; got: {deny_result}"
    );

    let resolved_counted = registry
        .resolve("fs.absent.counted")
        .expect("registry resolves the counted-gated tool by name");
    let counted_result = resolved_counted
        .call(fs_absent_args())
        .await
        .expect("counted deny gate returns Ok with synthesized refusal");
    assert!(
        counted_result.starts_with("permission denied: "),
        "deny path on the counted tool synthesizes a refusal; got: {counted_result}"
    );
    assert_eq!(
        counting_calls.load(Ordering::SeqCst),
        0,
        "denied calls must not invoke the inner tool"
    );
}
