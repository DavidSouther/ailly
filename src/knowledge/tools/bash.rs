use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rig::completion::ToolDefinition;
use rig::tool::{Tool, ToolDyn};
use serde_json::Value;

use crate::knowledge::permissions::{Classification, Classifier, PermissionBackend, PermissionGated};

#[derive(Debug, Clone)]
pub struct Bash {
    cwd: PathBuf,
    timeout: Duration,
}

impl Bash {
    pub const NAME: &'static str = "bash";

    pub fn new(cwd: PathBuf, timeout: Duration) -> Self {
        Self { cwd, timeout }
    }

    pub fn register_with(
        &self,
        classifier: Arc<dyn Classifier>,
        backend: Arc<dyn PermissionBackend>,
    ) -> Arc<dyn ToolDyn> {
        let inner: Arc<dyn ToolDyn> = Arc::new(self.clone());
        Arc::new(PermissionGated::new(Self::NAME, inner, classifier, backend))
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct BashArgs {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BashError {
    #[error("spawning bash: {source}")]
    Spawn {
        #[source]
        source: std::io::Error,
    },
    #[error("waiting for bash: {source}")]
    Wait {
        #[source]
        source: std::io::Error,
    },
    #[error("command timed out after {elapsed:?}")]
    Timeout { elapsed: Duration },
}

impl Tool for Bash {
    const NAME: &'static str = Self::NAME;

    type Error = BashError;
    type Args = BashArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Execute a shell command via `bash -c`. Returns combined \
                stdout, optional stderr block, and an exit-code footer."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute via `bash -c`."
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional one-line human-readable explanation \
                            of why the command is being run."
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        use std::process::Stdio;
        use tokio::process::Command;

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(&args.command)
            .current_dir(&self.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = cmd.spawn().map_err(|source| BashError::Spawn { source })?;
        match tokio::time::timeout(self.timeout, child.wait_with_output()).await {
            Err(_) => Err(BashError::Timeout {
                elapsed: self.timeout,
            }),
            Ok(Err(source)) => Err(BashError::Wait { source }),
            Ok(Ok(output)) => {
                let code = output.status.code().unwrap_or(-1);
                Ok(format_bash_output(&output.stdout, &output.stderr, code))
            }
        }
    }
}

const OUTPUT_CAP: usize = 32 * 1024;

fn format_bash_output(stdout_bytes: &[u8], stderr_bytes: &[u8], code: i32) -> String {
    let stdout = String::from_utf8_lossy(stdout_bytes);

    let mut content = String::with_capacity(stdout.len() + stderr_bytes.len() + 64);
    content.push_str(&stdout);

    if !stderr_bytes.is_empty() {
        let stderr = String::from_utf8_lossy(stderr_bytes);
        content.push('\n');
        content.push_str("--- stderr ---\n");
        content.push_str(&stderr);
    }

    let mut truncated = false;
    if content.len() > OUTPUT_CAP {
        let mut idx = OUTPUT_CAP;
        while !content.is_char_boundary(idx) {
            idx -= 1;
        }
        content.truncate(idx);
        truncated = true;
    }

    while content.ends_with('\n') {
        content.pop();
    }

    if truncated {
        content.push('\n');
        content.push_str("[truncated]");
    }

    content.push('\n');
    content.push('\n');
    content.push_str(&format!("[exit: {code}]"));
    content
}

#[derive(Clone, Copy)]
enum Inspector {
    PathArgs,
    GitPushFlags,
    FindFlags,
}

struct BuiltinRule {
    head: &'static str,
    classification: Classification,
    inspector: Option<Inspector>,
}

const BUILTINS: &[BuiltinRule] = &[
    // Two-word heads first; the lookup tries the two-word join before falling back to argv[0].
    BuiltinRule {
        head: "git status",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "git log",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "git diff",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "git show",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "git branch",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "git remote",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "git add",
        classification: Classification::Safe,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "git commit",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "git restore",
        classification: Classification::Safe,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "git checkout",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "git fetch",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "git pull",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "git push",
        classification: Classification::Safe,
        inspector: Some(Inspector::GitPushFlags),
    },
    BuiltinRule {
        head: "git clone",
        classification: Classification::Unsafe,
        inspector: None,
    },
    BuiltinRule {
        head: "cargo build",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "cargo test",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "cargo check",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "cargo fmt",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "cargo clippy",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "cargo publish",
        classification: Classification::Unsafe,
        inspector: None,
    },
    BuiltinRule {
        head: "npm install",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "npm run",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "npm test",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "npm publish",
        classification: Classification::Unsafe,
        inspector: None,
    },
    // Single-word heads.
    BuiltinRule {
        head: "ls",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "pwd",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "echo",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "printf",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "cat",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "head",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "tail",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "wc",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "file",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "stat",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "which",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "whereis",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "tree",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "date",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "env",
        classification: Classification::Read,
        inspector: None,
    },
    BuiltinRule {
        head: "grep",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "rg",
        classification: Classification::Read,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "find",
        classification: Classification::Read,
        inspector: Some(Inspector::FindFlags),
    },
    BuiltinRule {
        head: "mkdir",
        classification: Classification::Safe,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "touch",
        classification: Classification::Safe,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "cp",
        classification: Classification::Safe,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "mv",
        classification: Classification::Safe,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "rm",
        classification: Classification::Safe,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "chmod",
        classification: Classification::Safe,
        inspector: Some(Inspector::PathArgs),
    },
    BuiltinRule {
        head: "make",
        classification: Classification::Safe,
        inspector: None,
    },
    BuiltinRule {
        head: "curl",
        classification: Classification::Unsafe,
        inspector: None,
    },
    BuiltinRule {
        head: "wget",
        classification: Classification::Unsafe,
        inspector: None,
    },
    BuiltinRule {
        head: "ssh",
        classification: Classification::Unsafe,
        inspector: None,
    },
    BuiltinRule {
        head: "scp",
        classification: Classification::Unsafe,
        inspector: None,
    },
    BuiltinRule {
        head: "rsync",
        classification: Classification::Unsafe,
        inspector: None,
    },
    BuiltinRule {
        head: "nc",
        classification: Classification::Unsafe,
        inspector: None,
    },
    BuiltinRule {
        head: "sudo",
        classification: Classification::Unsafe,
        inspector: None,
    },
    BuiltinRule {
        head: "su",
        classification: Classification::Unsafe,
        inspector: None,
    },
];

pub struct BashClassifier {
    cwd: PathBuf,
    overrides: Vec<(globset::GlobMatcher, Classification)>,
    builtins: &'static [BuiltinRule],
}

impl BashClassifier {
    pub fn with_defaults(cwd: PathBuf) -> Self {
        Self {
            cwd,
            overrides: Vec::new(),
            builtins: BUILTINS,
        }
    }

    pub fn builder(cwd: PathBuf) -> BashClassifierBuilder {
        BashClassifierBuilder {
            cwd,
            overrides: Vec::new(),
        }
    }
}

impl Classifier for BashClassifier {
    fn classify(&self, args: &Value) -> Classification {
        let command = match args.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return Classification::Unsafe,
        };

        let tokens = match shlex::split(command) {
            Some(t) if !t.is_empty() => t,
            _ => return Classification::Unsafe,
        };

        if has_compound_marker(command) {
            return Classification::Unsafe;
        }

        if let Some(class) = match_override(&self.overrides, &tokens) {
            return class;
        }

        let (rule, args_start) = match find_builtin(self.builtins, &tokens) {
            Some(found) => found,
            None => return Classification::Unsafe,
        };

        let rest = &tokens[args_start..];
        apply_inspector(rule, rest, &self.cwd)
    }
}

fn match_override(
    overrides: &[(globset::GlobMatcher, Classification)],
    tokens: &[String],
) -> Option<Classification> {
    if overrides.is_empty() {
        return None;
    }
    let rejoined = shlex::try_join(tokens.iter().map(|s| s.as_str())).ok()?;
    overrides
        .iter()
        .find(|(matcher, _)| matcher.is_match(&rejoined))
        .map(|(_, class)| *class)
}

fn has_compound_marker(command: &str) -> bool {
    let bytes = command.as_bytes();
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;
    while i < bytes.len() {
        let c = bytes[i];
        if !in_single && !in_double && c == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if !in_double && c == b'\'' {
            in_single = !in_single;
            i += 1;
            continue;
        }
        if !in_single && c == b'"' {
            in_double = !in_double;
            i += 1;
            continue;
        }
        if in_single || in_double {
            i += 1;
            continue;
        }
        if c == b'`' {
            return true;
        }
        if c == b'$' && bytes.get(i + 1) == Some(&b'(') {
            return true;
        }
        if c == b'&' && bytes.get(i + 1) == Some(&b'&') {
            return true;
        }
        if c == b'|' && bytes.get(i + 1) == Some(&b'|') {
            return true;
        }
        if c == b'|' {
            return true;
        }
        if c == b';' {
            return true;
        }
        if c == b'>' {
            let mut j = i + 1;
            if bytes.get(j) == Some(&b'>') {
                j += 1;
            }
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if bytes.get(j) == Some(&b'/') {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn find_builtin<'a>(
    table: &'a [BuiltinRule],
    tokens: &[String],
) -> Option<(&'a BuiltinRule, usize)> {
    if tokens.len() >= 2 {
        let head = format!("{} {}", tokens[0], tokens[1]);
        if let Some(rule) = table.iter().find(|r| r.head == head) {
            return Some((rule, 2));
        }
    }
    if let Some(rule) = table.iter().find(|r| r.head == tokens[0]) {
        return Some((rule, 1));
    }
    None
}

fn apply_inspector(rule: &BuiltinRule, rest: &[String], cwd: &Path) -> Classification {
    match rule.inspector {
        None => rule.classification,
        Some(Inspector::PathArgs) => {
            if rest
                .iter()
                .any(|t| !t.starts_with('-') && path_escapes_cwd(t, cwd))
            {
                Classification::Unsafe
            } else {
                rule.classification
            }
        }
        Some(Inspector::GitPushFlags) => {
            if rest
                .iter()
                .any(|t| matches!(t.as_str(), "--force" | "-f" | "--force-with-lease"))
            {
                Classification::Unsafe
            } else {
                rule.classification
            }
        }
        Some(Inspector::FindFlags) => {
            if rest
                .iter()
                .any(|t| matches!(t.as_str(), "-delete" | "-exec" | "-execdir"))
            {
                Classification::Unsafe
            } else {
                rule.classification
            }
        }
    }
}

fn path_escapes_cwd(token: &str, cwd: &Path) -> bool {
    let candidate = if Path::new(token).is_absolute() {
        PathBuf::from(token)
    } else {
        cwd.join(token)
    };
    let normalized = normalize_path(&candidate);
    let cwd_norm = normalize_path(cwd);
    !normalized.starts_with(&cwd_norm)
}

fn normalize_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub struct BashClassifierBuilder {
    cwd: PathBuf,
    overrides: Vec<(globset::GlobMatcher, Classification)>,
}

impl BashClassifierBuilder {
    pub fn allow_read(mut self, pattern: &str) -> Self {
        self.push(pattern, Classification::Read);
        self
    }

    pub fn allow_safe(mut self, pattern: &str) -> Self {
        self.push(pattern, Classification::Safe);
        self
    }

    pub fn deny_unsafe(mut self, pattern: &str) -> Self {
        self.push(pattern, Classification::Unsafe);
        self
    }

    pub fn build(self) -> BashClassifier {
        BashClassifier {
            cwd: self.cwd,
            overrides: self.overrides,
            builtins: BUILTINS,
        }
    }

    fn push(&mut self, pattern: &str, class: Classification) {
        let matcher = globset::Glob::new(pattern)
            .expect("operator-supplied glob pattern must compile")
            .compile_matcher();
        self.overrides.push((matcher, class));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use std::time::Duration;

    use crate::engine::{HashMapRegistry, ToolRegistry};
    use crate::knowledge::permissions::{BasePolicy, ClassRouterBackend, Classifier, PermissionBackend};

    fn args(command: &str) -> String {
        serde_json::to_string(&serde_json::json!({ "command": command }))
            .expect("serialise bash args")
    }

    fn cwd_for_test() -> std::path::PathBuf {
        std::env::current_dir().expect("cwd available for test")
    }

    fn bash_with(timeout: Duration) -> Bash {
        Bash::new(cwd_for_test(), timeout)
    }

    fn bash_args(command: &str) -> BashArgs {
        BashArgs {
            command: command.to_string(),
            description: None,
        }
    }

    async fn run(bash: &Bash, args: BashArgs) -> Result<String, BashError> {
        <Bash as rig::tool::Tool>::call(bash, args).await
    }

    #[tokio::test]
    async fn runs_simple_command_and_returns_combined_output() {
        let bash = bash_with(Duration::from_secs(5));

        let result = run(&bash, bash_args("echo hi")).await.expect("ok");

        assert_eq!(result, "hi\n\n[exit: 0]");
    }

    #[tokio::test]
    async fn non_zero_exit_returns_ok_with_exit_marker() {
        let bash = bash_with(Duration::from_secs(5));

        let result = run(&bash, bash_args("exit 7"))
            .await
            .expect("ok on non-zero");

        assert!(
            result.ends_with("[exit: 7]"),
            "non-zero exit must surface in footer; got {result}"
        );
    }

    #[tokio::test]
    async fn stderr_is_captured_under_boundary_marker() {
        let bash = bash_with(Duration::from_secs(5));

        let result = run(&bash, bash_args("printf out; printf err 1>&2"))
            .await
            .expect("ok");

        assert!(
            result.contains("--- stderr ---\nerr"),
            "stderr must appear under boundary marker; got {result}"
        );
        assert!(result.starts_with("out"));
        assert!(result.ends_with("[exit: 0]"));
    }

    #[tokio::test]
    async fn empty_stderr_omits_boundary_marker() {
        let bash = bash_with(Duration::from_secs(5));

        let result = run(&bash, bash_args("echo only-stdout")).await.expect("ok");

        assert!(
            !result.contains("--- stderr ---"),
            "empty stderr must not emit boundary marker; got {result}"
        );
    }

    #[tokio::test]
    async fn cwd_is_captured_construction_arg() {
        let captured = std::env::temp_dir();
        let bash = Bash::new(captured.clone(), Duration::from_secs(5));

        let result = run(&bash, bash_args("pwd")).await.expect("ok");

        let captured_str = captured.to_string_lossy();
        let captured_str = captured_str.trim_end_matches('/');
        assert!(
            result.contains(captured_str),
            "pwd output must reflect captured cwd {captured_str}; got {result}"
        );
    }

    #[tokio::test]
    async fn timeout_returns_typed_timeout_error() {
        let bash = bash_with(Duration::from_millis(50));

        let err = run(&bash, bash_args("sleep 5"))
            .await
            .expect_err("times out");

        match err {
            BashError::Timeout { elapsed } => {
                assert_eq!(elapsed, Duration::from_millis(50));
            }
            other => panic!("expected Timeout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn output_is_truncated_at_cap_with_marker_then_exit_footer() {
        let bash = bash_with(Duration::from_secs(30));

        let result = run(&bash, bash_args("yes A | head -c 50000"))
            .await
            .expect("ok");

        assert!(
            result.contains("[truncated]"),
            "oversized output must include [truncated]; got len {}",
            result.len()
        );
        assert!(
            result.ends_with("[exit: 0]"),
            "[exit:] footer must follow [truncated]; got tail {:?}",
            &result[result.len().saturating_sub(64)..]
        );
        let truncated_at = result.find("[truncated]").unwrap();
        let exit_at = result.find("[exit: 0]").unwrap();
        assert!(truncated_at < exit_at);
    }

    #[tokio::test]
    async fn non_utf8_bytes_are_replaced_lossy() {
        let bash = bash_with(Duration::from_secs(5));

        let result = run(&bash, bash_args("printf '\\xff\\xfe'"))
            .await
            .expect("ok");

        assert!(
            result.contains('\u{FFFD}'),
            "invalid UTF-8 must be replaced with U+FFFD; got {result:?}"
        );
    }

    #[tokio::test]
    async fn description_field_is_optional_and_does_not_affect_execution() {
        let bash = bash_with(Duration::from_secs(5));

        let with_desc = run(
            &bash,
            BashArgs {
                command: "echo hi".to_string(),
                description: Some("explanatory note".to_string()),
            },
        )
        .await
        .expect("ok with description");
        let without_desc = run(&bash, bash_args("echo hi")).await.expect("ok without");

        assert_eq!(with_desc, without_desc);
    }

    fn classifier_for_test() -> BashClassifier {
        BashClassifier::with_defaults(cwd_for_test())
    }

    fn classify(classifier: &BashClassifier, command: &str) -> Classification {
        classifier.classify(&serde_json::json!({ "command": command }))
    }

    #[test]
    fn read_command_classifies_read() {
        let c = classifier_for_test();
        for cmd in ["ls", "pwd", "cat README.md", "git status", "git log"] {
            assert_eq!(
                classify(&c, cmd),
                Classification::Read,
                "{cmd} should classify as Read"
            );
        }
    }

    #[test]
    fn safe_command_classifies_safe() {
        let c = classifier_for_test();
        for cmd in ["cargo build", "cargo test", "git commit", "make"] {
            assert_eq!(
                classify(&c, cmd),
                Classification::Safe,
                "{cmd} should classify as Safe"
            );
        }
    }

    #[test]
    fn unsafe_command_classifies_unsafe() {
        let c = classifier_for_test();
        for cmd in [
            "curl https://example.com",
            "sudo rm -rf /tmp/x",
            "git clone https://example.com/repo",
            "cargo publish",
        ] {
            assert_eq!(
                classify(&c, cmd),
                Classification::Unsafe,
                "{cmd} should classify as Unsafe"
            );
        }
    }

    #[test]
    fn git_push_force_demotes_to_unsafe() {
        let c = classifier_for_test();
        for flag in ["--force", "-f", "--force-with-lease"] {
            let cmd = format!("git push origin main {flag}");
            assert_eq!(
                classify(&c, &cmd),
                Classification::Unsafe,
                "{cmd} should demote to Unsafe"
            );
        }
    }

    #[test]
    fn git_push_without_force_is_safe() {
        let c = classifier_for_test();
        assert_eq!(classify(&c, "git push origin main"), Classification::Safe);
    }

    #[test]
    fn git_pull_and_git_fetch_classify_safe() {
        let c = classifier_for_test();
        assert_eq!(classify(&c, "git pull"), Classification::Safe);
        assert_eq!(classify(&c, "git fetch origin"), Classification::Safe);
    }

    #[test]
    fn find_with_delete_or_exec_demotes_to_unsafe() {
        let c = classifier_for_test();
        for flag in ["-delete", "-exec rm {} ;", "-execdir rm {} ;"] {
            let cmd = format!("find . -name foo {flag}");
            assert_eq!(
                classify(&c, &cmd),
                Classification::Unsafe,
                "{cmd} should demote to Unsafe"
            );
        }
        // Sanity check: plain find without these flags stays Read.
        assert_eq!(classify(&c, "find . -name foo"), Classification::Read);
    }

    #[test]
    fn path_arg_outside_cwd_demotes() {
        let c = classifier_for_test();
        assert_eq!(
            classify(&c, "cat /etc/passwd"),
            Classification::Unsafe,
            "absolute path outside cwd must demote"
        );
        assert_eq!(
            classify(&c, "cat ../sibling/file"),
            Classification::Unsafe,
            "parent-traversing relative path must demote"
        );
        // Sanity: path inside cwd stays Read.
        assert_eq!(classify(&c, "cat README.md"), Classification::Read);
    }

    #[test]
    fn unknown_head_command_falls_through_unsafe() {
        let c = classifier_for_test();
        assert_eq!(
            classify(&c, "totally-not-a-known-command --do-stuff"),
            Classification::Unsafe
        );
    }

    #[test]
    fn compound_command_classifies_unsafe() {
        let c = classifier_for_test();
        for cmd in [
            "cat foo && cat bar",
            "cat foo || cat bar",
            "cat foo; cat bar",
            "cat foo | grep bar",
            "echo $(date)",
            "echo `date`",
            "echo hi > /tmp/out",
            "echo hi >> /tmp/out",
        ] {
            assert_eq!(
                classify(&c, cmd),
                Classification::Unsafe,
                "{cmd} should classify as Unsafe (compound)"
            );
        }
        // Sanity: a `;` inside quotes is not a compound marker.
        assert_eq!(
            classify(&c, "echo 'hello;world'"),
            Classification::Read,
            "quoted ; must not trip compound detection"
        );
    }

    #[test]
    fn glob_override_takes_precedence_over_builtin() {
        let c = BashClassifier::builder(cwd_for_test())
            .deny_unsafe("ls*")
            .build();
        assert_eq!(
            classify(&c, "ls"),
            Classification::Unsafe,
            "override must win over the built-in Read bucket for ls"
        );
    }

    #[test]
    fn glob_override_first_match_wins() {
        let c = BashClassifier::builder(cwd_for_test())
            .allow_read("cargo*")
            .deny_unsafe("cargo*")
            .build();
        assert_eq!(
            classify(&c, "cargo build"),
            Classification::Read,
            "first matching override must win"
        );
    }

    #[test]
    fn lex_failure_classifies_unsafe() {
        let c = classifier_for_test();
        // An unterminated double-quote makes shlex::split return None.
        assert_eq!(classify(&c, "echo \"unterminated"), Classification::Unsafe);
    }

    #[tokio::test]
    async fn bash_registered_helper_round_trips_through_class_router_backend() {
        let cwd = std::env::current_dir().expect("cwd available for test");
        let classifier: Arc<dyn Classifier> = Arc::new(BashClassifier::with_defaults(cwd.clone()));
        let backend: Arc<dyn PermissionBackend> = Arc::new(ClassRouterBackend {
            on_read: BasePolicy::Allow,
            on_safe: BasePolicy::Deny,
            on_unsafe: BasePolicy::Deny,
        });

        let bash = Bash::new(cwd, Duration::from_secs(5)).register_with(classifier, backend);

        let mut registry = HashMapRegistry::default();
        registry.insert(Bash::NAME, bash);
        let registry: Arc<dyn ToolRegistry> = Arc::new(registry);

        let resolved = registry
            .resolve(Bash::NAME)
            .expect("registry resolves bash by name");

        let allowed = resolved
            .call(args("ls"))
            .await
            .expect("allowed Read-bucket call returns Ok");
        assert!(
            allowed.contains("[exit: 0]"),
            "allowed call must include the exit footer; got {allowed}"
        );

        let denied = resolved
            .call(args("cargo build"))
            .await
            .expect("denied Safe-bucket call resolves to Ok with synthesised refusal");
        assert!(
            denied.starts_with("permission denied: "),
            "denied call must surface the synthesised refusal prefix; got {denied}"
        );
        assert!(
            !denied.contains("[exit:"),
            "denied call must not contain executor output; got {denied}"
        );
    }
}
