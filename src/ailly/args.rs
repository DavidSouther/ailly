use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::{Parser, ValueEnum};

use crate::knowledge::base::WorkflowName;

#[derive(Parser, Debug)]
#[command(
    name = "ailly",
    version,
    about = "Ailly CLI",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Base folder to search for content and system prompts. Defaults to the current directory.
    #[arg(short = 'r', long)]
    pub root: Option<PathBuf>,

    /// Additional knowledge root directories. Repeatable. Each `--knowledge`
    /// path is appended to the project root in argument order; same-named
    /// skills/workflows are resolved first-wins, project before extras.
    #[arg(long = "knowledge", value_name = "PATH")]
    pub knowledge: Vec<PathBuf>,

    /// Generate a final, single piece of content and print the response to standard out.
    #[arg(short = 'p', long, env = "AILLY_PROMPT")]
    pub prompt: Option<String>,

    /// Strip every recorded `[[response]]` entry from every turn file under `--root`,
    /// preserving the prompt and writing the file back idempotently.
    #[arg(long, conflicts_with = "prompt")]
    pub clean: bool,

    /// Run a workflow from `workflow.toml` at the conversation root, or list
    /// available workflows when invoked with no value. The argument is
    /// `WORKFLOW` to begin at the workflow's `start`, or `WORKFLOW:TASK` to
    /// override the queue and begin at `TASK` for one run.
    #[arg(
        short = 'w',
        long,
        env = "AILLY_WORKFLOW",
        value_name = "WORKFLOW[:TASK]",
        num_args = 0..=1,
        default_missing_value = ""
    )]
    pub workflow: Option<String>,

    /// List every workflow discoverable from this project's roots and exit.
    #[arg(long, conflicts_with_all = ["prompt", "clean", "workflow"])]
    pub list_workflows: bool,

    /// List every skill discoverable from this project's roots and exit.
    #[arg(long, conflicts_with_all = ["prompt", "clean", "workflow"])]
    pub list_skills: bool,

    /// List every tool the workflow runtime would register and exit.
    #[arg(long, conflicts_with_all = ["prompt", "clean", "workflow"])]
    pub list_tools: bool,

    /// Provide a workflow input as `KEY=VALUE`. Repeat the flag for multiple
    /// inputs. Persisted state from a prior run takes precedence over flag
    /// values for the same key.
    #[arg(long, value_name = "KEY=VALUE", action = clap::ArgAction::Append)]
    pub input: Vec<String>,

    /// Engine to drive inference. `noop` is available for testing.
    #[arg(long, env = "AILLY_ENGINE")]
    pub engine: Option<String>,

    /// Model to use within the engine. Default depends on the engine.
    #[arg(long, env = "AILLY_MODEL")]
    pub model: Option<String>,

    /// Set log level to info. Equivalent to `--log-level v`.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// env-filter directive controlling log output. Accepts `v`/`verbose` for info,
    /// numeric levels 0-4 (error, warn, info, debug, trace), or any `RUST_LOG`-style
    /// filter expression.
    #[arg(long, value_name = "FILTER")]
    pub log_level: Option<String>,

    /// Output format for log records.
    #[arg(long, value_enum, default_value_t = LogFormat::Pretty)]
    pub log_format: LogFormat,
}

impl Cli {
    /// Resolve the project root, falling back to the current working directory.
    pub fn root(&self) -> PathBuf {
        self.root
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    /// Build a [`Project`] from the resolved project root and any
    /// `--knowledge` paths. The project root becomes the first knowledge
    /// root; additional `--knowledge` entries follow in argument order.
    /// Conversation mode is assumed; workflow-mode callers override
    /// `conversations` themselves.
    pub fn project(&self) -> anyhow::Result<crate::project::Project> {
        use crate::project::{ConversationRoot, KnowledgeRoot, Project, ProjectRoot};
        let raw_root = self.root();
        if !raw_root.exists() {
            return Err(anyhow!("root path does not exist: {}", raw_root.display()));
        }
        let project_root = ProjectRoot::from_physical(&raw_root)?;
        let conversations = ConversationRoot::from(project_root.clone());
        let mut knowledge: Vec<KnowledgeRoot> = vec![KnowledgeRoot::from(project_root.clone())];
        for raw in &self.knowledge {
            if !raw.exists() {
                return Err(anyhow!("knowledge path does not exist: {}", raw.display()));
            }
            knowledge.push(KnowledgeRoot::from_physical(raw)?);
        }
        Ok(Project {
            root: project_root,
            conversations,
            knowledge,
            bash_cwd: raw_root,
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum LogFormat {
    Pretty,
    Json,
}

/// Parse the `-w` argument into a workflow name and optional task override.
///
/// `"basic"` becomes `("basic", None)`. `"basic:second"` becomes
/// `("basic", Some("second"))`. An empty string is rejected.
pub fn parse_workflow_arg(raw: &str) -> Result<(WorkflowName, Option<String>)> {
    if raw.is_empty() {
        return Err(anyhow!("--workflow requires a non-empty value"));
    }
    match raw.split_once(':') {
        Some((wf, task)) if !wf.is_empty() && !task.is_empty() => {
            Ok((wf.into(), Some(task.to_string())))
        }
        Some(_) => Err(anyhow!(
            "--workflow {raw:?}: expected `WORKFLOW` or `WORKFLOW:TASK` with both parts non-empty"
        )),
        None => Ok((raw.into(), None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn clap_rejects_clean_combined_with_prompt() {
        let result = Cli::try_parse_from(["ailly", "--clean", "--prompt", "foo"]);
        let err = result.expect_err("--clean must conflict with --prompt at parse time");
        let msg = err.to_string();
        assert!(
            msg.contains("--clean") && msg.contains("--prompt"),
            "expected conflict message naming both flags, got: {msg}"
        );
    }

    #[test]
    fn clap_parses_workflow_short_flag() {
        let cli = Cli::try_parse_from(["ailly", "-w", "basic"]).expect("parses");
        assert_eq!(cli.workflow.as_deref(), Some("basic"));
    }

    #[test]
    fn parse_workflow_arg_splits_on_single_colon() {
        let (wf, task) = parse_workflow_arg("basic").unwrap();
        assert_eq!(wf, WorkflowName::new("basic"));
        assert!(task.is_none());

        let (wf, task) = parse_workflow_arg("basic:second").unwrap();
        assert_eq!(wf, WorkflowName::new("basic"));
        assert_eq!(task.as_deref(), Some("second"));
    }

    #[test]
    fn parse_workflow_arg_rejects_empty() {
        let err = parse_workflow_arg("").unwrap_err();
        assert!(err.to_string().contains("non-empty"));
    }

    #[test]
    fn clap_accepts_w_with_no_value_as_empty_string() {
        let cli = Cli::try_parse_from(["ailly", "-w"]).expect("parses");
        assert_eq!(cli.workflow.as_deref(), Some(""));
    }

    #[test]
    fn clap_rejects_list_workflows_combined_with_workflow() {
        let result = Cli::try_parse_from(["ailly", "--list-workflows", "-w", "basic"]);
        assert!(result.is_err(), "expected conflict error: {result:?}");
    }

    #[test]
    fn clap_rejects_list_skills_combined_with_workflow() {
        let result = Cli::try_parse_from(["ailly", "--list-skills", "-w", "basic"]);
        assert!(result.is_err(), "expected conflict error: {result:?}");
    }

    #[test]
    fn clap_rejects_list_tools_combined_with_prompt() {
        let result = Cli::try_parse_from(["ailly", "--list-tools", "--prompt", "x"]);
        assert!(result.is_err(), "expected conflict error: {result:?}");
    }

    #[test]
    fn clap_collects_repeated_knowledge_flags_in_argument_order() {
        let cli = Cli::try_parse_from([
            "ailly",
            "--knowledge",
            "/tmp/a",
            "--knowledge",
            "/tmp/b",
            "--knowledge",
            "/tmp/c",
        ])
        .expect("parses");
        let paths: Vec<&str> = cli.knowledge.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(paths, ["/tmp/a", "/tmp/b", "/tmp/c"]);
    }
}
