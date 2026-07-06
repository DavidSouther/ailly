//! Per-assertion executor: `Assertion::check` and the verdict it produces.
//!
//! Sync families are implemented inline; LLM/subprocess families return
//! `Deferred` until the orchestrator wires their collaborator. The dispatch
//! table is a total `match` over `Assertion`.

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::content::conversation::Content;
use crate::content::conversation::ContentBlock;
use crate::content::conversation::Conversation;
use crate::content::conversation::Message;
use crate::content::conversation::Meta;
use crate::content::conversation::Role;
use crate::content::conversation::Trace;
use crate::content::evaluation::Assertion;
use crate::content::evaluation::Op;
use crate::content::evaluation::ScriptBody;
use crate::content::evaluation::ScriptRuntime;
use crate::content::evaluation::TokenMetric;
use crate::engine::engine::CompletionRequest;
use crate::engine::engine::EngineProvider;
use crate::knowledge::script_runner::ExitDisposition;
use crate::knowledge::script_runner::SCRIPT_TIMEOUT_DEFAULT;
use crate::knowledge::script_runner::ScriptOutput;
use crate::knowledge::script_runner::ScriptRunner;
use crate::knowledge::script_runner::ScriptSpec;

/// Verdict of an `Assertion::check` call. Closed set; callers `match`
/// exhaustively. Adding a variant is a deliberate contract change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionOutcome {
    /// The assertion's expectation was satisfied.
    Pass,
    /// Checked and not satisfied. `reason` names the expectation and the
    /// observed value; serialized verbatim by the eval-report writer.
    Fail { reason: String },
    /// A required collaborator was absent from `EvaluationContext`. The
    /// orchestrator will route the assertion to its proper executor.
    Deferred,
    /// The assertion is structurally invalid (e.g. uncompilable regex,
    /// unparseable `JSONPath`). Distinct from `Fail` so suite-authoring bugs
    /// surface separately from data-driven failures.
    Malformed { reason: String },
    /// LLM judge call failed for environmental or transport reasons (auth,
    /// rate-limit, timeout, network). Distinct from `Malformed`, which is
    /// reserved for suite-authoring bugs, and from `Deferred`, which means the
    /// LLM judge was absent.
    Errored { reason: String },
}

/// Collaborators required by LLM-based assertion variants. Fields are
/// `Option<&dyn Trait>` so callers add a `None` for every collaborator not yet
/// wired. Future steps add fields without breaking `Assertion::check`'s
/// signature.
pub struct EvaluationContext<'a> {
    /// Engine consulted by `Judge` (and, when wired, `TextSemanticMatch`).
    /// `None` ⇒ those variants return `Deferred`.
    pub engine: Option<&'a dyn EngineProvider>,
    /// Subprocess runner consulted by the `Script` / `Program` families.
    /// Required together with `project_root`; either missing ⇒ those variants
    /// return `Deferred`.
    pub script_runner: Option<&'a dyn ScriptRunner>,
    /// Project root the `Script` / `Program` families run subprocesses in and
    /// confine `ScriptBody::Path` against. Required together with
    /// `script_runner`; either missing ⇒ those variants return `Deferred`.
    pub project_root: Option<&'a Path>,
}

impl EvaluationContext<'_> {
    /// The no-collaborators context. Tests of the sync families and any
    /// orchestrator slice that does not yet need LLM or subprocess execution
    /// pass this.
    #[must_use]
    pub const fn empty() -> EvaluationContext<'static> {
        EvaluationContext {
            engine: None,
            script_runner: None,
            project_root: None,
        }
    }
}

impl Assertion {
    /// Check this assertion against `conversation` in `ctx`. Total over every
    /// variant. The 12 sync families ignore `ctx`; the 5 LLM/subprocess
    /// families return `Deferred` when their collaborator is `None`.
    ///
    /// Invariant: never panics, never performs I/O on its own, never
    /// dereferences `Content::Blocks` on a `Deferred` path.
    pub async fn check(
        &self,
        conversation: &Conversation,
        ctx: &EvaluationContext<'_>,
    ) -> AssertionOutcome {
        match self {
            Assertion::Judge { prompt } => {
                // A synchronous `check` has no cross-case accumulator, so it
                // hands the judge an empty `program_outputs` slice (today's
                // byte-identical body). The orchestrator passes the real slice.
                check_judge(prompt, conversation, ctx, &[]).await.outcome
            }
            Assertion::Script {
                runtime,
                script,
                pass_env,
            } => {
                check_script(runtime, script, pass_env, conversation, ctx)
                    .await
                    .outcome
            }
            Assertion::Program { script, pass_env } => {
                check_program(script, pass_env, conversation, ctx)
                    .await
                    .outcome
            }
            Assertion::Tool { .. } | Assertion::TextSemanticMatch { .. } => {
                AssertionOutcome::Deferred
            }

            Assertion::TextContains {
                value,
                case_sensitive,
            } => check_text_contains(conversation, value, case_sensitive.unwrap_or(true)),
            Assertion::TextNotContains {
                value,
                case_sensitive,
            } => check_text_not_contains(conversation, value, case_sensitive.unwrap_or(true)),
            Assertion::TextMatches { pattern, flags } => {
                check_text_matches(conversation, pattern, flags.as_deref())
            }
            Assertion::TextEquals { value } => check_text_equals(conversation, value),

            Assertion::MustCallTool { tool, with_args } => {
                check_must_call_tool(conversation, tool, with_args.as_ref())
            }
            Assertion::MustNotCallTool { tool } => check_must_not_call_tool(conversation, tool),
            Assertion::ToolCallCount { tool, op, value } => {
                check_tool_call_count(conversation, tool.as_deref(), op, *value)
            }
            Assertion::ToolCallOrder { sequence } => check_tool_call_order(conversation, sequence),
            Assertion::ToolCallCollection { tools } => {
                check_tool_call_collection(conversation, tools)
            }

            Assertion::JsonPath { path, op, value } => {
                check_json_path(conversation, path, op, value)
            }
            Assertion::ResponseField { path, exists } => {
                check_response_field(conversation, path, *exists)
            }

            Assertion::Tokens { metric, op, value } => {
                check_tokens(conversation, metric, op, *value)
            }
            Assertion::LatencyMs { op, value } => check_latency_ms(conversation, op, *value),
        }
    }
}

/// Extract the text content of the final filled assistant turn.
///
/// Walks `conversation.session` from the back, skipping blank assistant slots
/// (`body == None`). From the first filled assistant message:
///   - `Content::Text(s)` returns `s`.
///   - `Content::Blocks(blocks)` concatenates every `ContentBlock::Text { text
///     }` in declaration order, joined by `"\n"`. `ToolUse`, `ToolResult`,
///     `Thinking`, and `Image` blocks contribute nothing.
///
/// Returns `None` if no filled assistant turn exists; every text assertion
/// translates `None` into a `Fail` with a named reason so the missing data
/// never silently passes.
fn final_assistant_text(conversation: &Conversation) -> Option<String> {
    final_text_for_role(conversation, Role::Assistant)
}

/// Common reason returned by every text assertion when no filled assistant
/// turn exists. Named so the upstream cause is immediate from the report.
const NO_FILLED_ASSISTANT: &str = "no filled assistant turn present";

/// Text content of the final filled user turn. Mirrors `final_assistant_text`
/// exactly — same blank-slot skip, same block concatenation by newline. Used
/// by `check_judge` to feed the judge the candidate's prompt alongside its
/// reply.
fn final_user_text(conversation: &Conversation) -> Option<String> {
    final_text_for_role(conversation, Role::User)
}

/// Walk `conversation.session` from the back, find the first filled message
/// with the given role, and lower its body to plain text via
/// [`flatten_content`]. Returns `None` if no filled message of that role
/// exists.
fn final_text_for_role(conversation: &Conversation, role: Role) -> Option<String> {
    for message in conversation.session.iter().rev() {
        if message.role != role {
            continue;
        }
        let Some(body) = message.body.as_ref() else {
            continue;
        };
        return Some(flatten_content(body));
    }
    None
}

/// Lower a `Content` body to plain text. `Content::Text` returns the string
/// directly; `Content::Blocks` concatenates every `ContentBlock::Text` block
/// in declaration order joined by `"\n"`. `ToolUse`, `ToolResult`,
/// `Thinking`, and `Image` blocks contribute nothing — matching the existing
/// text-assertion lowering rule.
fn flatten_content(content: &Content) -> String {
    match content {
        Content::Text(text) => text.clone(),
        Content::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                ContentBlock::ToolUse { .. }
                | ContentBlock::ToolResult { .. }
                | ContentBlock::Thinking { .. }
                | ContentBlock::Image { .. } => None,
            })
            .collect::<Vec<&str>>()
            .join("\n"),
    }
}

/// System message sent on every `Assertion::Judge` call. The "last GRADE line
/// is binding" sentence pairs with the greedy-last regex in
/// [`grade_regex`].
const JUDGE_SYSTEM_PROMPT: &str =
    "You are an evaluator. Read the rubric and the candidate response.
Reason step-by-step about whether the candidate response satisfies
the rubric, then end your reply with one line:
  GRADE: P    (rubric satisfied)
  GRADE: F    (rubric violated)
  GRADE: I    (cannot decide)
The last GRADE line is binding. Do not print GRADE inside your reasoning.";

/// Lazily-compiled greedy-last regex over the judge reply. The `(?is)` flags
/// enable case-insensitive matching and dot-matches-newline, and the leading
/// `.*` is greedy so the rightmost `GRADE: <verdict>` line wins. This is
/// Inspect AI's prompt-injection mitigation: a candidate response that
/// embeds an earlier `GRADE: P` cannot override the judge's final verdict.
fn grade_regex() -> &'static regex::Regex {
    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        // Group 1 spans the binding `GRADE:` token so its start marks where the
        // CoT prefix ends; group 2 is the verdict letter. The greedy `.*`
        // forces group 1 onto the *last* `GRADE:` occurrence.
        regex::Regex::new(r"(?is).*(GRADE\s*:\s*([PFI]))")
            .expect("judge grade regex is a compile-time constant")
    })
}

/// Maximum byte length of the chain-of-thought prefix carried in `Fail` /
/// `Malformed` reasons. Keeps eval reports readable when a judge produces
/// pages of reasoning before the verdict.
const COT_REASON_CAP: usize = 200;

/// Outcome + in-memory transcript of one `Assertion::Judge` call.
///
/// The transcript is `Some` only when the engine call completed and a reply
/// was lowered to text — `None` for the deferred path (no engine in context)
/// and the missing-turn paths (no engine call was made). The orchestrator is
/// the sole consumer of `transcript`; `Assertion::check` discards it.
pub(crate) struct JudgeCall {
    pub outcome: AssertionOutcome,
    pub transcript: Option<Conversation>,
}

/// Verdict + the raw stdout a `program`/`script` subprocess printed, captured
/// so a later fanout judge can read it as `program_outputs`. `captured` is
/// `Some` only when the subprocess actually ran and printed non-empty stdout;
/// every deferred/malformed/errored path before the run leaves it `None`, as
/// does an empty-stdout pass (an empty capture contributes nothing to the judge
/// prompt). Carries the same redacted, trimmed text `classify_exit` folds into
/// a `Fail` reason, kept on the `Pass` path too. The orchestrator is the sole
/// consumer of `captured`; `Assertion::check` discards it by reading
/// `.outcome`, exactly as it does for `JudgeCall`.
pub(crate) struct CheckerCall {
    pub outcome: AssertionOutcome,
    pub captured: Option<String>,
}

/// Run one `Assertion::Judge { prompt }` against a filled conversation.
///
/// Steps (per design §Judge executor):
///   1. If `ctx.engine` is `None`, return `Deferred` with no transcript.
///   2. Resolve `final_user_text` and `final_assistant_text`. A missing turn
///      yields `Fail { reason: "judge: no filled <X> turn" }` and no transcript
///      — the call never went out.
///   3. Build the judge messages: one system message carrying
///      `JUDGE_SYSTEM_PROMPT`, one user message carrying the labelled `RUBRIC:
///      / USER QUESTION: / CANDIDATE RESPONSE:` block. When `program_outputs`
///      is non-empty, a trailing `PROGRAM_OUTPUTS:` block of `[{class}] {text}`
///      entries (joined by `\n`) is appended; an empty slice leaves the body
///      byte-identical to the heading-free form.
///   4. Call `engine.complete`. On `Err`, return `Errored` carrying the
///      stringified engine error; no transcript. `Errored` is distinct from a
///      data-driven `Fail`: it marks an environmental/transport failure (auth,
///      rate-limit, timeout, network) rather than a violated rubric.
///   5. Lower the response content to text using the same rule as
///      `final_assistant_text`.
///   6. Parse the reply with `grade_regex`: `P` → `Pass`; `F` → `Fail` with a
///      `CoT` prefix capped at `COT_REASON_CAP` bytes; `I` → `Malformed {
///      reason: "judge inconclusive: …" }`; no match → `Malformed { reason:
///      "judge produced no GRADE line: …" }`.
///   7. Build the transcript: system + user + filled assistant carrying the
///      reply content and the engine's `trace`. `meta.model` is the
///      conversation-under-test's `meta.model`; `meta.assembly =
///      Some("judge")`; `meta.binding` is carried verbatim.
pub(crate) async fn check_judge(
    prompt: &str,
    conversation: &Conversation,
    ctx: &EvaluationContext<'_>,
    program_outputs: &[(String, String)],
) -> JudgeCall {
    let Some(engine) = ctx.engine else {
        return JudgeCall {
            outcome: AssertionOutcome::Deferred,
            transcript: None,
        };
    };

    let Some(user_text) = final_user_text(conversation) else {
        return JudgeCall {
            outcome: AssertionOutcome::Fail {
                reason: String::from("judge: no filled user turn"),
            },
            transcript: None,
        };
    };
    let Some(assistant_text) = final_assistant_text(conversation) else {
        return JudgeCall {
            outcome: AssertionOutcome::Fail {
                reason: String::from("judge: no filled assistant turn"),
            },
            transcript: None,
        };
    };

    let mut user_body = format!(
        "RUBRIC:\n{prompt}\n\nUSER QUESTION:\n{user_text}\n\nCANDIDATE RESPONSE:\n{assistant_text}",
    );
    // When earlier `program`/`script` cases captured stdout for this
    // conversation, append it under a `PROGRAM_OUTPUTS:` heading so the judge
    // can consume the prior scorer results instead of re-scoring. An empty
    // accumulator leaves the body byte-identical to the heading-free form every
    // existing judge produces (the insurance-claim / patterns-eval judges and
    // every `eval_judge.rs` assertion). The heading is uppercase to match the
    // sibling `RUBRIC:` / `USER QUESTION:` / `CANDIDATE RESPONSE:` headings.
    if !program_outputs.is_empty() {
        user_body.push_str("\n\nPROGRAM_OUTPUTS:\n");
        let block = program_outputs
            .iter()
            .map(|(class, text)| format!("[{class}] {text}"))
            .collect::<Vec<_>>()
            .join("\n");
        user_body.push_str(&block);
    }
    let system_message = judge_message(Role::System, JUDGE_SYSTEM_PROMPT.to_owned(), None);
    let user_message = judge_message(Role::User, user_body.clone(), None);
    let judge_msgs = [system_message.clone(), user_message.clone()];

    let response = match engine
        .complete(CompletionRequest {
            model: conversation.meta.model.clone(),
            messages: &judge_msgs,
            debug: false,
        })
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return JudgeCall {
                outcome: AssertionOutcome::Errored {
                    reason: format!("judge engine complete: {err}"),
                },
                transcript: None,
            };
        }
    };

    let outcome = parse_judge_reply(&flatten_content(&response.content));

    let assistant_message = Message {
        role: Role::Assistant,
        body: Some(response.content),
        cache: false,
        trace: Some(response.trace),
        _phase: PhantomData,
    };
    let transcript = Conversation {
        meta: Meta {
            model: conversation.meta.model.clone(),
            debug: false,
            assembly: Some(String::from("judge")),
            binding: conversation.meta.binding.clone(),
        },
        session: vec![system_message, user_message, assistant_message],
    };

    JudgeCall {
        outcome,
        transcript: Some(transcript),
    }
}

/// Build one judge transcript message with no cache flag and an optional
/// trace. Used for the synthetic system and user turns (no trace) and is
/// reused by the assistant turn once the engine returns.
fn judge_message(role: Role, text: String, trace: Option<Trace>) -> Message {
    Message {
        role,
        body: Some(Content::Text(text)),
        cache: false,
        trace,
        _phase: PhantomData,
    }
}

/// Parse a judge reply into one of `Pass` / `Fail` / `Malformed`. Greedy-last
/// over `GRADE: [PFI]`; everything before the matched verdict is the `CoT`
/// prefix used in the `Fail` and `Malformed { inconclusive }` reasons.
fn parse_judge_reply(reply: &str) -> AssertionOutcome {
    let Some(captures) = grade_regex().captures(reply) else {
        let snippet = trimmed_prefix(reply, COT_REASON_CAP);
        return AssertionOutcome::Malformed {
            reason: format!("judge produced no GRADE line: {snippet}"),
        };
    };
    let verdict = captures
        .get(2)
        .expect("GRADE regex always captures one letter")
        .as_str()
        .to_ascii_uppercase();
    let grade_start = captures
        .get(1)
        .expect("GRADE regex always captures the verdict token")
        .start();
    let cot_prefix = trimmed_prefix(reply[..grade_start].trim_end(), COT_REASON_CAP);
    match verdict.as_str() {
        "P" => AssertionOutcome::Pass,
        "F" => AssertionOutcome::Fail { reason: cot_prefix },
        "I" => AssertionOutcome::Malformed {
            reason: format!("judge inconclusive: {cot_prefix}"),
        },
        _ => unreachable!("regex pattern restricts verdict to [PFI]"),
    }
}

/// Trim a string to at most `cap` bytes on a UTF-8 boundary, then trim
/// surrounding whitespace. Used to keep judge `Fail` reasons bounded.
fn trimmed_prefix(text: &str, cap: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= cap {
        return trimmed.to_owned();
    }
    let mut end = cap;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed[..end].trim_end().to_owned()
}

/// A subprocess command resolved and ready to run: the program, its argv, and
/// a tempfile handle (for a materialized `Contents` body) whose lifetime must
/// outlive the run. Built by `check_script` / `check_program`, consumed by
/// `run_checker`.
struct PreparedScript {
    program: String,
    args: Vec<String>,
    tempfile_guard: Option<tempfile::NamedTempFile>,
}

/// Run one `Assertion::Script` as a subprocess and map its exit to a verdict.
///
/// Steps (design §`check_script` flow):
///   1. let-else over `(ctx.script_runner, ctx.project_root)`; either `None` ⇒
///      `Deferred` with no work.
///   2. Resolve the runtime binary from the trusted parent env (`AILLY_PYTHON`
///      / `AILLY_NODE`, defaulting to `python3` / `node`).
///   3. Resolve the script body to argv: `Contents` empty-after-trim ⇒
///      `Malformed`, else written to a `.py`/`.js` tempfile whose lifetime
///      spans `run()`; `Path` is joined onto and confined within the project
///      root, escapes and missing paths ⇒ `Malformed`.
///   4. stdin is the final assistant text ONLY; the user question rides
///      `AILLY_USER_QUESTION`. Either turn missing ⇒ `Fail`.
///   5. Build the cleared-env allowlist plus any per-assertion `pass_env`
///      opt-ins and call `runner.run`; a `ScriptError` ⇒ `Errored`.
///   6. Classify the exit (see [`classify_exit`]), redacting handed secrets.
///   7. Warn-log any non-empty stderr once, with handed secrets redacted.
pub(crate) async fn check_script(
    runtime: &ScriptRuntime,
    script: &ScriptBody,
    pass_env: &[String],
    conversation: &Conversation,
    ctx: &EvaluationContext<'_>,
) -> CheckerCall {
    let (Some(runner), Some(project_root)) = (ctx.script_runner, ctx.project_root) else {
        return CheckerCall {
            outcome: AssertionOutcome::Deferred,
            captured: None,
        };
    };

    let program = resolve_runtime_binary(runtime);

    // Resolve the body to argv, keeping any tempfile alive across the run.
    let (args, tempfile_guard): (Vec<String>, Option<tempfile::NamedTempFile>) = match script {
        ScriptBody::Contents { contents } => {
            if contents.trim().is_empty() {
                return CheckerCall {
                    outcome: AssertionOutcome::Malformed {
                        reason: String::from("script: contents is empty"),
                    },
                    captured: None,
                };
            }
            match write_contents_tempfile(runtime, contents) {
                Ok(file) => (vec![file.path().to_string_lossy().into_owned()], Some(file)),
                Err(err) => {
                    return CheckerCall {
                        outcome: AssertionOutcome::Errored {
                            reason: format!("script: failed to write temporary script: {err}"),
                        },
                        captured: None,
                    };
                }
            }
        }
        ScriptBody::Path { path } => match resolve_script_path(project_root, path) {
            Ok(resolved) => (vec![resolved.to_string_lossy().into_owned()], None),
            Err(outcome) => {
                return CheckerCall {
                    outcome,
                    captured: None,
                };
            }
        },
    };

    run_checker(
        PreparedScript {
            program,
            args,
            tempfile_guard,
        },
        pass_env,
        conversation,
        runner,
        project_root,
    )
    .await
}

/// Run one `Assertion::Program` as a subprocess. The `script` field names the
/// program to spawn, resolved by [`resolve_program`]: a bare name (no path
/// separator) is passed through for allowlisted-`PATH` lookup, a relative path
/// bearing a separator is joined onto the project root, and an absolute path is
/// passed through as-is. It runs with no args, no runtime resolution, and no
/// script-body materialization. A non-existent binary surfaces as
/// `ScriptError::Spawn` ⇒ `Errored`, not `Malformed`.
pub(crate) async fn check_program(
    script: &str,
    pass_env: &[String],
    conversation: &Conversation,
    ctx: &EvaluationContext<'_>,
) -> CheckerCall {
    let (Some(runner), Some(project_root)) = (ctx.script_runner, ctx.project_root) else {
        return CheckerCall {
            outcome: AssertionOutcome::Deferred,
            captured: None,
        };
    };
    run_checker(
        PreparedScript {
            program: resolve_program(project_root, script),
            args: Vec::new(),
            tempfile_guard: None,
        },
        pass_env,
        conversation,
        runner,
        project_root,
    )
    .await
}

/// Resolve an `Assertion::Program` program name to the string handed to the
/// runner. `std`/`tokio` `Command` resolve a separator-bearing relative program
/// against the *caller's* cwd, not `current_dir`, so a relative path must be
/// joined onto the project root here to match the documented behaviour. An
/// absolute path is trusted verbatim; a bare name (no separator) is left alone
/// so the allowlisted `PATH` resolves it. `Program` is trusted suite-author
/// input, so no containment check is applied — unlike `ScriptBody::Path`.
fn resolve_program(project_root: &Path, script: &str) -> String {
    let p = Path::new(script);
    if p.is_absolute() {
        return script.to_owned();
    }
    match p.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            project_root.join(p).to_string_lossy().into_owned()
        }
        _ => script.to_owned(), // bare name: resolve via the allowlisted PATH
    }
}

/// Shared tail for the `Script` and `Program` families: feed the candidate
/// response on stdin, build the cleared child env (plus `pass_env`), run the
/// prepared subprocess, and classify its exit with handed secrets redacted.
/// `prepared.tempfile_guard` keeps a materialized `Contents` script alive
/// across the run.
async fn run_checker(
    prepared: PreparedScript,
    pass_env: &[String],
    conversation: &Conversation,
    runner: &dyn ScriptRunner,
    project_root: &Path,
) -> CheckerCall {
    let Some(assistant_text) = final_assistant_text(conversation) else {
        return CheckerCall {
            outcome: AssertionOutcome::Fail {
                reason: String::from("script: no filled assistant turn"),
            },
            captured: None,
        };
    };
    let Some(user_question) = final_user_text(conversation) else {
        return CheckerCall {
            outcome: AssertionOutcome::Fail {
                reason: String::from("script: no filled user turn"),
            },
            captured: None,
        };
    };

    let (env, handed) = build_child_env(project_root, &user_question, pass_env);

    let spec = ScriptSpec {
        program: prepared.program,
        args: prepared.args,
        cwd: project_root.to_path_buf(),
        env,
        stdin: assistant_text.into_bytes(),
        timeout: SCRIPT_TIMEOUT_DEFAULT,
    };

    let output = match runner.run(spec).await {
        Ok(output) => output,
        Err(err) => {
            return CheckerCall {
                outcome: AssertionOutcome::Errored {
                    reason: format!("script: {err}"),
                },
                captured: None,
            };
        }
    };

    // The tempfile (if any) outlived the subprocess; release it now.
    drop(prepared.tempfile_guard);

    if !output.stderr.is_empty() {
        tracing::warn!(
            stderr = %redact(&String::from_utf8_lossy(&output.stderr), &handed),
            "script: stderr captured",
        );
    }

    // Capture the same redacted, trimmed stdout `classify_exit` would fold into
    // a `Fail` reason, but keep it on the `Pass` path too so a later fanout
    // judge can read it as `program_outputs`. Empty stdout contributes nothing.
    let captured = {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = trimmed_prefix(&redact(&stdout, &handed), COT_REASON_CAP);
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    };

    CheckerCall {
        outcome: classify_exit(&output, &handed),
        captured,
    }
}

/// Resolve the interpreter binary for `runtime` from the trusted parent env.
/// `AILLY_PYTHON` / `AILLY_NODE` override the `python3` / `node` defaults;
/// they name an interpreter, not a candidate-controlled path, so they are not
/// containment-checked.
fn resolve_runtime_binary(runtime: &ScriptRuntime) -> String {
    match runtime {
        ScriptRuntime::Python => {
            std::env::var("AILLY_PYTHON").unwrap_or_else(|_| String::from("python3"))
        }
        ScriptRuntime::Node => std::env::var("AILLY_NODE").unwrap_or_else(|_| String::from("node")),
    }
}

/// Write `contents` to a `.py` / `.js` tempfile. The returned handle deletes
/// the file on drop, so the caller keeps it alive until the subprocess exits.
fn write_contents_tempfile(
    runtime: &ScriptRuntime,
    contents: &str,
) -> std::io::Result<tempfile::NamedTempFile> {
    use std::io::Write;

    let suffix = match runtime {
        ScriptRuntime::Python => ".py",
        ScriptRuntime::Node => ".js",
    };
    let mut file = tempfile::Builder::new().suffix(suffix).tempfile()?;
    file.write_all(contents.as_bytes())?;
    file.flush()?;
    Ok(file)
}

/// Resolve and confine a `ScriptBody::Path` against the project root.
/// `Path::join` does not confine — an absolute argument replaces the base and
/// `..` is not normalized — so the join is canonicalized and verified to stay
/// inside the canonicalized project root. The root is canonicalized too, and a
/// failure there is fail-closed (`Errored`): comparing a canonical child
/// against a non-canonical root would let a path escape through an
/// unnormalized root. The runner is never called on a containment, existence,
/// or root-canonicalization failure.
fn resolve_script_path(project_root: &Path, path: &str) -> Result<PathBuf, AssertionOutcome> {
    let joined = project_root.join(path);
    let Ok(canonical) = joined.canonicalize() else {
        return Err(AssertionOutcome::Malformed {
            reason: format!("script: path does not exist: {}", joined.display()),
        });
    };
    let Ok(root) = project_root.canonicalize() else {
        return Err(AssertionOutcome::Errored {
            reason: format!(
                "script: project root cannot be canonicalized: {}",
                project_root.display()
            ),
        });
    };
    if canonical.starts_with(&root) {
        Ok(canonical)
    } else {
        Err(AssertionOutcome::Malformed {
            reason: format!("script: path escapes project root: {}", canonical.display()),
        })
    }
}

/// Environment variable `(name, value)` pairs handed to a child subprocess.
type EnvVars = Vec<(String, String)>;

/// Build the COMPLETE child env and the set of handed `(name, value)` secrets
/// to redact. The base allowlist is `PATH` (so the interpreter resolves),
/// `AILLY_PROJECT_ROOT`, `AILLY_USER_QUESTION` (omitted with a warn when it
/// carries a NUL byte), and `HOME` / `TMPDIR` / `LANG` when present. On top of
/// that, each name in `pass_env` present in the trusted parent env is copied in
/// (and recorded for redaction); an absent name is skipped with a warn. Names
/// are literal — no glob or prefix expansion.
///
/// `HOME` / `TMPDIR` / `LANG` are passed for interpreter compatibility, so the
/// cleared env protects environment-borne secrets but NOT filesystem-rooted
/// credentials: a checker can still read e.g. `~/.aws/credentials` off disk.
fn build_child_env(
    project_root: &Path,
    user_question: &str,
    pass_env: &[String],
) -> (EnvVars, EnvVars) {
    let mut env: Vec<(String, String)> = Vec::new();
    if let Ok(path) = std::env::var("PATH") {
        env.push((String::from("PATH"), path));
    }
    env.push((
        String::from("AILLY_PROJECT_ROOT"),
        project_root.display().to_string(),
    ));
    if user_question.contains('\0') {
        tracing::warn!("script: user question contains NUL byte; omitting AILLY_USER_QUESTION");
    } else {
        env.push((
            String::from("AILLY_USER_QUESTION"),
            user_question.to_owned(),
        ));
    }
    for key in ["HOME", "TMPDIR", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            env.push((String::from(key), value));
        }
    }

    let mut handed: Vec<(String, String)> = Vec::new();
    for name in pass_env {
        match std::env::var(name) {
            Ok(value) => {
                handed.push((name.clone(), value.clone()));
                env.push((name.clone(), value));
            }
            Err(_) => {
                tracing::warn!(
                    var = %name,
                    "pass_env: requested variable not present in environment",
                );
            }
        }
    }

    (env, handed)
}

/// Replace every occurrence of each handed secret value with `«redacted:NAME»`
/// before captured output reaches a report reason or a tracing field. Empty
/// values are skipped so an empty secret cannot blanket the text.
fn redact(text: &str, handed: &[(String, String)]) -> String {
    let mut out = text.to_owned();
    for (name, value) in handed {
        if value.is_empty() {
            continue;
        }
        out = out.replace(value.as_str(), &format!("«redacted:{name}»"));
    }
    out
}

/// Map a completed `ScriptOutput` to a verdict. The non-zero exit splits three
/// ways so a checker that *malfunctions* is never reported as a candidate that
/// *failed* (design §step 6):
///   - `Exited(0)`                                    ⇒ `Pass`
///   - `Exited(n≠0)`, stdout non-empty                ⇒ `Fail` (stdout is the
///     reason)
///   - `Exited(n≠0)`, stdout empty, stderr non-empty  ⇒ `Errored` (broken
///     checker)
///   - `Exited(n≠0)`, both streams empty              ⇒ `Fail` ("exit n (no
///     output)")
///   - `TimedOut`                                     ⇒ `Errored`
///   - `Signaled`                                     ⇒ `Errored`
fn classify_exit(output: &ScriptOutput, handed: &[(String, String)]) -> AssertionOutcome {
    match output.exit {
        ExitDisposition::Exited(0) => AssertionOutcome::Pass,
        ExitDisposition::Exited(code) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.trim().is_empty() {
                    AssertionOutcome::Fail {
                        reason: format!("script: exit {code} (no output)"),
                    }
                } else {
                    AssertionOutcome::Errored {
                        reason: format!(
                            "script: exit {code}, empty stdout; stderr: {}",
                            trimmed_prefix(&redact(&stderr, handed), COT_REASON_CAP),
                        ),
                    }
                }
            } else {
                AssertionOutcome::Fail {
                    reason: format!(
                        "script: exit {code}: {}",
                        trimmed_prefix(&redact(&stdout, handed), COT_REASON_CAP),
                    ),
                }
            }
        }
        ExitDisposition::TimedOut => AssertionOutcome::Errored {
            reason: format!(
                "script: timed out after {}s",
                SCRIPT_TIMEOUT_DEFAULT.as_secs()
            ),
        },
        ExitDisposition::Signaled => AssertionOutcome::Errored {
            reason: String::from("script: killed by signal"),
        },
    }
}

fn check_text_contains(
    conversation: &Conversation,
    value: &str,
    case_sensitive: bool,
) -> AssertionOutcome {
    let Some(haystack) = final_assistant_text(conversation) else {
        return AssertionOutcome::Fail {
            reason: format!("text_contains: {NO_FILLED_ASSISTANT}"),
        };
    };
    let matched = if case_sensitive {
        haystack.contains(value)
    } else {
        haystack.to_lowercase().contains(&value.to_lowercase())
    };
    if matched {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "text_contains: expected substring {value:?} not found in {}-byte response",
                haystack.len(),
            ),
        }
    }
}

fn check_text_not_contains(
    conversation: &Conversation,
    value: &str,
    case_sensitive: bool,
) -> AssertionOutcome {
    let Some(haystack) = final_assistant_text(conversation) else {
        return AssertionOutcome::Fail {
            reason: format!("text_not_contains: {NO_FILLED_ASSISTANT}"),
        };
    };
    let offset = if case_sensitive {
        haystack.find(value)
    } else {
        haystack.to_lowercase().find(&value.to_lowercase())
    };
    match offset {
        None => AssertionOutcome::Pass,
        Some(at) => AssertionOutcome::Fail {
            reason: format!(
                "text_not_contains: forbidden substring {value:?} found at byte offset {at}"
            ),
        },
    }
}

fn check_text_matches(
    conversation: &Conversation,
    pattern: &str,
    flags: Option<&str>,
) -> AssertionOutcome {
    let compiled = if let Some(flags) = flags.filter(|s| !s.is_empty()) {
        regex::Regex::new(&format!("(?{flags}){pattern}"))
    } else {
        regex::Regex::new(pattern)
    };
    let regex = match compiled {
        Ok(regex) => regex,
        Err(err) => {
            return AssertionOutcome::Malformed {
                reason: format!("text_matches: regex compile error: {err}"),
            };
        }
    };
    let Some(haystack) = final_assistant_text(conversation) else {
        return AssertionOutcome::Fail {
            reason: format!("text_matches: {NO_FILLED_ASSISTANT}"),
        };
    };
    if regex.is_match(&haystack) {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "text_matches: pattern {pattern:?} did not match {}-byte response",
                haystack.len(),
            ),
        }
    }
}

fn check_text_equals(conversation: &Conversation, value: &str) -> AssertionOutcome {
    let Some(actual) = final_assistant_text(conversation) else {
        return AssertionOutcome::Fail {
            reason: format!("text_equals: {NO_FILLED_ASSISTANT}"),
        };
    };
    if actual == value {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "text_equals: expected {value:?} ({} bytes) but got {actual:?} ({} bytes)",
                value.len(),
                actual.len(),
            ),
        }
    }
}

/// Collect every `ContentBlock::ToolUse` in `conversation.session` in
/// (message order, block order within message). Blank assistant slots
/// contribute nothing. `Content::Text` messages contribute nothing. Tool-use
/// is by spec assistant-authored but the extractor does not filter on role —
/// only on block kind — so a future conversation that records tool calls
/// under a different role still flows through.
fn extract_tool_uses(conversation: &Conversation) -> Vec<&ContentBlock> {
    let mut out = Vec::new();
    for message in &conversation.session {
        let Some(Content::Blocks(blocks)) = message.body.as_ref() else {
            continue;
        };
        for block in blocks {
            if matches!(block, ContentBlock::ToolUse { .. }) {
                out.push(block);
            }
        }
    }
    out
}

/// Project a `ContentBlock::ToolUse`'s `name` field, panicking if called on
/// any other variant. Callers must filter via `extract_tool_uses` first.
fn tool_use_name(block: &ContentBlock) -> &str {
    match block {
        ContentBlock::ToolUse { name, .. } => name,
        ContentBlock::Text { .. }
        | ContentBlock::ToolResult { .. }
        | ContentBlock::Thinking { .. }
        | ContentBlock::Image { .. } => {
            unreachable!("extract_tool_uses only yields ToolUse blocks")
        }
    }
}

/// Recursive subset match on `serde_yaml_ng::Value`:
///   - Scalars: equal by value.
///   - Maps: every key in `expected` must appear in `actual` with a value that
///     subset-matches.
///   - Sequences: deep equality (no canonical "subset of a sequence").
///
/// Subset rather than strict equality so a test author can pin
/// `{ claim_id: C-1 }` without enumerating every context field the real call
/// adds.
fn yaml_subset_match(expected: &serde_yaml_ng::Value, actual: &serde_yaml_ng::Value) -> bool {
    match (expected, actual) {
        (
            serde_yaml_ng::Value::Mapping(expected_map),
            serde_yaml_ng::Value::Mapping(actual_map),
        ) => expected_map.iter().all(|(key, expected_value)| {
            actual_map
                .get(key)
                .is_some_and(|actual_value| yaml_subset_match(expected_value, actual_value))
        }),
        _ => expected == actual,
    }
}

/// Lift `observed` and the assertion's `i64` value to `i128` so neither a
/// saturating sum nor a negative assertion value causes a wrapping subtract.
fn compare_i128(observed: i128, op: &Op, value: i64) -> bool {
    let value = i128::from(value);
    match op {
        Op::Eq => observed == value,
        Op::NotEq => observed != value,
        Op::Lt => observed < value,
        Op::LtEq => observed <= value,
        Op::Gt => observed > value,
        Op::GtEq => observed >= value,
    }
}

fn op_symbol(op: &Op) -> &'static str {
    match op {
        Op::Eq => "==",
        Op::NotEq => "!=",
        Op::Lt => "<",
        Op::LtEq => "<=",
        Op::Gt => ">",
        Op::GtEq => ">=",
    }
}

fn check_must_call_tool(
    conversation: &Conversation,
    tool: &str,
    with_args: Option<&serde_yaml_ng::Value>,
) -> AssertionOutcome {
    let calls = extract_tool_uses(conversation);
    let observed_names: Vec<&str> = calls.iter().copied().map(tool_use_name).collect();
    let mut name_matched = false;
    for block in &calls {
        let ContentBlock::ToolUse { name, input, .. } = block else {
            continue;
        };
        if name != tool {
            continue;
        }
        name_matched = true;
        match with_args {
            None => return AssertionOutcome::Pass,
            Some(expected) if yaml_subset_match(expected, input) => return AssertionOutcome::Pass,
            Some(_) => {}
        }
    }
    let reason = if name_matched {
        format!(
            "must_call_tool: tool {tool:?} invoked but no call matched the expected `with_args` subset",
        )
    } else {
        format!(
            "must_call_tool: tool {tool:?} was not invoked (observed tool calls: {observed_names:?})",
        )
    };
    AssertionOutcome::Fail { reason }
}

fn check_must_not_call_tool(conversation: &Conversation, tool: &str) -> AssertionOutcome {
    let calls = extract_tool_uses(conversation);
    if let Some(index) = calls.iter().position(|block| tool_use_name(block) == tool) {
        return AssertionOutcome::Fail {
            reason: format!(
                "must_not_call_tool: forbidden tool {tool:?} invoked first at call index {index}",
            ),
        };
    }
    AssertionOutcome::Pass
}

fn check_tool_call_count(
    conversation: &Conversation,
    tool: Option<&str>,
    op: &Op,
    value: i64,
) -> AssertionOutcome {
    let calls = extract_tool_uses(conversation);
    let count = match tool {
        None => calls.len(),
        Some(name) => calls
            .iter()
            .filter(|block| tool_use_name(block) == name)
            .count(),
    };
    let observed_i128 = i128::try_from(count).unwrap_or(i128::MAX);
    if compare_i128(observed_i128, op, value) {
        AssertionOutcome::Pass
    } else {
        let tool_display = tool.unwrap_or("any");
        AssertionOutcome::Fail {
            reason: format!(
                "tool_call_count: expected count {} {value}, observed {count} (tool: {tool_display})",
                op_symbol(op),
            ),
        }
    }
}

/// Convert a `serde_yaml_ng::Value` to a `serde_json::Value` by round-tripping
/// through a JSON string. The YAML value comes from a parsed assertion field,
/// so the resulting JSON shape is the suite author's literal — strings,
/// numbers, bools, null, and homogeneous containers all flow through.
fn yaml_to_json(value: &serde_yaml_ng::Value) -> serde_json::Value {
    let json_text = serde_json::to_string(value)
        .expect("serde_yaml_ng::Value serializes to JSON via its serde::Serialize impl");
    serde_json::from_str(&json_text)
        .expect("a JSON string emitted by serde_json round-trips to a serde_json::Value")
}

/// First-result of a `JSONPath` query against `conversation` rendered as JSON.
/// `Ok(None)` ⇒ path is valid but matched nothing; `Ok(Some(value))` ⇒ first
/// matched JSON value; `Err(outcome)` ⇒ path failed to parse and the caller
/// returns the `Malformed` outcome verbatim.
fn jsonpath_first(
    conversation: &Conversation,
    path: &str,
) -> Result<Option<serde_json::Value>, AssertionOutcome> {
    let parsed =
        serde_json_path::JsonPath::parse(path).map_err(|err| AssertionOutcome::Malformed {
            reason: format!("json_path: parse error on path {path:?}: {err}"),
        })?;
    let json = serde_json::to_value(conversation)
        .expect("Conversation derives Serialize and lowers to JSON without loss");
    let matched = parsed.query(&json).all();
    Ok(matched.first().map(|value| (*value).clone()))
}

fn compare_json_values(
    observed: &serde_json::Value,
    op: &Op,
    expected: &serde_json::Value,
) -> ComparisonResult {
    match op {
        Op::Eq => ComparisonResult::Decided(observed == expected),
        Op::NotEq => ComparisonResult::Decided(observed != expected),
        Op::Lt | Op::LtEq | Op::Gt | Op::GtEq => match (observed.as_f64(), expected.as_f64()) {
            (Some(lhs), Some(rhs)) => ComparisonResult::Decided(match op {
                Op::Lt => lhs < rhs,
                Op::LtEq => lhs <= rhs,
                Op::Gt => lhs > rhs,
                Op::GtEq => lhs >= rhs,
                Op::Eq | Op::NotEq => unreachable!("outer match excluded these arms"),
            }),
            _ => ComparisonResult::TypeMismatch,
        },
    }
}

enum ComparisonResult {
    Decided(bool),
    TypeMismatch,
}

fn check_json_path(
    conversation: &Conversation,
    path: &str,
    op: &Op,
    value: &serde_yaml_ng::Value,
) -> AssertionOutcome {
    let observed = match jsonpath_first(conversation, path) {
        Ok(observed) => observed,
        Err(outcome) => return outcome,
    };
    let Some(observed) = observed else {
        return AssertionOutcome::Fail {
            reason: format!("json_path: {path}: no value matched the path"),
        };
    };
    let expected = yaml_to_json(value);
    match compare_json_values(&observed, op, &expected) {
        ComparisonResult::Decided(true) => AssertionOutcome::Pass,
        ComparisonResult::Decided(false) => AssertionOutcome::Fail {
            reason: format!(
                "json_path: {path}: expected {} {expected}, observed {observed}",
                op_symbol(op),
            ),
        },
        ComparisonResult::TypeMismatch => AssertionOutcome::Fail {
            reason: format!(
                "json_path: {path}: ordered op {} requires numeric values, observed {observed}",
                op_symbol(op),
            ),
        },
    }
}

fn check_response_field(conversation: &Conversation, path: &str, exists: bool) -> AssertionOutcome {
    let observed = match jsonpath_first(conversation, path) {
        Ok(observed) => observed,
        Err(outcome) => return outcome,
    };
    match (exists, observed.is_some()) {
        (true, true) | (false, false) => AssertionOutcome::Pass,
        (true, false) => AssertionOutcome::Fail {
            reason: format!("response_field: {path}: expected value present, found none"),
        },
        (false, true) => AssertionOutcome::Fail {
            reason: format!("response_field: {path}: expected no value, but path matched"),
        },
    }
}

/// Sum a `Trace`-derived `u64` field across every message in
/// `conversation.session` that carries a `trace`. Blank assistant slots
/// contribute zero. The spec sums across messages, not across roles, so a
/// system or user message that happens to carry a `trace` is included.
fn sum_traces<F: Fn(&Trace) -> u64>(conversation: &Conversation, project: F) -> u128 {
    conversation
        .session
        .iter()
        .filter_map(|message| message.trace.as_ref())
        .map(|trace| u128::from(project(trace)))
        .sum()
}

fn check_tokens(
    conversation: &Conversation,
    metric: &TokenMetric,
    op: &Op,
    value: i64,
) -> AssertionOutcome {
    let observed = match metric {
        TokenMetric::Input => sum_traces(conversation, |t| t.tokens.input),
        TokenMetric::Output => sum_traces(conversation, |t| t.tokens.output),
        TokenMetric::Total => sum_traces(conversation, |t| {
            t.tokens.input.saturating_add(t.tokens.output)
        }),
    };
    let observed_i128 = i128::try_from(observed).unwrap_or(i128::MAX);
    if compare_i128(observed_i128, op, value) {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "tokens: expected {} {value} ({metric:?}), observed {observed}",
                op_symbol(op),
            ),
        }
    }
}

fn check_latency_ms(conversation: &Conversation, op: &Op, value: i64) -> AssertionOutcome {
    let observed = sum_traces(conversation, |t| t.latency_ms);
    let observed_i128 = i128::try_from(observed).unwrap_or(i128::MAX);
    if compare_i128(observed_i128, op, value) {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "latency_ms: expected {} {value}, observed {observed}",
                op_symbol(op),
            ),
        }
    }
}

fn check_tool_call_order(conversation: &Conversation, sequence: &[String]) -> AssertionOutcome {
    let calls = extract_tool_uses(conversation);
    let observed_names: Vec<&str> = calls.iter().copied().map(tool_use_name).collect();
    let mut cursor = 0;
    for name in &observed_names {
        if cursor >= sequence.len() {
            break;
        }
        if *name == sequence[cursor] {
            cursor += 1;
        }
    }
    if cursor >= sequence.len() {
        AssertionOutcome::Pass
    } else {
        AssertionOutcome::Fail {
            reason: format!(
                "tool_call_order: expected subsequence {sequence:?}, stalled at {:?} (observed {observed_names:?})",
                sequence[cursor],
            ),
        }
    }
}

/// Order-insensitive multiset check: every name in `tools` must appear in
/// the observed tool-use calls at least as many times as it is listed.
/// Subset-of-multiset, not exact equality — extra calls (of any tool) are
/// tolerated, and order carries no meaning.
fn check_tool_call_collection(conversation: &Conversation, tools: &[String]) -> AssertionOutcome {
    let mut required: BTreeMap<&str, usize> = BTreeMap::new();
    for tool in tools {
        *required.entry(tool.as_str()).or_insert(0) += 1;
    }

    let calls = extract_tool_uses(conversation);
    let mut observed: BTreeMap<&str, usize> = BTreeMap::new();
    for block in &calls {
        *observed.entry(tool_use_name(block)).or_insert(0) += 1;
    }

    let shortfalls: Vec<(&str, usize, usize)> = required
        .iter()
        .filter_map(|(&name, &need)| {
            let have = observed.get(name).copied().unwrap_or(0);
            (have < need).then_some((name, need, have))
        })
        .collect();

    if shortfalls.is_empty() {
        AssertionOutcome::Pass
    } else {
        let detail = shortfalls
            .iter()
            .map(|(name, need, have)| format!("{name:?} (need {need}, got {have})"))
            .collect::<Vec<_>>()
            .join(", ");
        AssertionOutcome::Fail {
            reason: format!(
                "tool_call_collection: missing required call(s): {detail}; observed calls: {:?}",
                calls.iter().map(|b| tool_use_name(b)).collect::<Vec<_>>(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::marker::PhantomData;
    use std::sync::Mutex;

    use super::*;
    use crate::content::conversation::BindingMap;
    use crate::content::conversation::Message;
    use crate::content::conversation::Meta;
    use crate::content::conversation::ModelId;
    use crate::content::conversation::SpanId;
    use crate::content::conversation::TokenCounts;
    use crate::content::conversation::ToolUseId;
    use crate::content::evaluation::Assertion;
    use crate::content::evaluation::ScriptBody;
    use crate::content::evaluation::ScriptRuntime;
    use crate::content::evaluation::ToolCallSpec;
    use crate::engine::engine::NoopEngine;
    use crate::knowledge::script_runner::ScriptError;

    fn conversation_with(session: Vec<Message>) -> Conversation {
        Conversation {
            meta: Meta {
                model: ModelId::from("noop"),
                debug: false,
                assembly: None,
                binding: BindingMap::new(),
            },
            session,
        }
    }

    fn user_text(text: &str) -> Message {
        Message {
            role: Role::User,
            body: Some(Content::Text(String::from(text))),
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    fn assistant_text(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            body: Some(Content::Text(String::from(text))),
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    fn assistant_blocks(blocks: Vec<ContentBlock>) -> Message {
        Message {
            role: Role::Assistant,
            body: Some(Content::Blocks(blocks)),
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    fn assistant_blank() -> Message {
        Message {
            role: Role::Assistant,
            body: None,
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    fn tool_use(name: &str) -> ContentBlock {
        ContentBlock::ToolUse {
            id: ToolUseId::from("tool_1"),
            name: String::from(name),
            input: serde_yaml_ng::Value::Null,
        }
    }

    fn ctx() -> EvaluationContext<'static> {
        EvaluationContext::empty()
    }

    #[tokio::test]
    async fn text_contains_case_insensitive_matches_uppercase_substring() {
        let conv = conversation_with(vec![assistant_text("Policy Number Required.")]);
        let assertion = Assertion::TextContains {
            value: String::from("policy number"),
            case_sensitive: Some(false),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_contains_fails_when_substring_absent_with_named_reason() {
        let conv = conversation_with(vec![assistant_text("approved.")]);
        let assertion = Assertion::TextContains {
            value: String::from("policy"),
            case_sensitive: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("policy"), "reason missing needle: {reason}");
                assert!(
                    reason.contains("text_contains"),
                    "reason missing op: {reason}",
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_not_contains_passes_when_substring_absent() {
        let conv = conversation_with(vec![assistant_text("policy.")]);
        let assertion = Assertion::TextNotContains {
            value: String::from("auto-approve"),
            case_sensitive: None,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_not_contains_fails_when_substring_present_with_first_offset() {
        let conv = conversation_with(vec![assistant_text("zzz auto-approve here.")]);
        let assertion = Assertion::TextNotContains {
            value: String::from("auto-approve"),
            case_sensitive: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("auto-approve"));
                assert!(
                    reason.contains("offset 4"),
                    "reason missing offset: {reason}"
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_matches_with_invalid_regex_returns_malformed_with_compile_error() {
        let conv = conversation_with(vec![assistant_text("anything")]);
        let assertion = Assertion::TextMatches {
            pattern: String::from("[unclosed"),
            flags: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Malformed { reason } => {
                assert!(reason.contains("text_matches"), "got {reason}");
                assert!(reason.to_lowercase().contains("regex"), "got {reason}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_matches_with_flags_compiles_into_inline_group() {
        let conv = conversation_with(vec![assistant_text("POLICY number required")]);
        let assertion = Assertion::TextMatches {
            pattern: String::from("polic\\w+"),
            flags: Some(String::from("i")),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_matches_fails_when_pattern_does_not_match() {
        let conv = conversation_with(vec![assistant_text("nothing relevant")]);
        let assertion = Assertion::TextMatches {
            pattern: String::from("policy"),
            flags: None,
        };
        assert!(matches!(
            assertion.check(&conv, &ctx()).await,
            AssertionOutcome::Fail { .. }
        ));
    }

    #[tokio::test]
    async fn text_equals_passes_on_byte_exact_match() {
        let conv = conversation_with(vec![assistant_text("ok")]);
        let assertion = Assertion::TextEquals {
            value: String::from("ok"),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_equals_fails_with_diff_summary_when_lengths_match_but_bytes_differ() {
        let conv = conversation_with(vec![assistant_text("ab")]);
        let assertion = Assertion::TextEquals {
            value: String::from("ba"),
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(
                    reason.contains("\"ba\"") && reason.contains("\"ab\""),
                    "got {reason}"
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_assertions_against_unfilled_conversation_fail_with_named_reason() {
        let conv = conversation_with(vec![user_text("ask"), assistant_blank()]);
        let assertion = Assertion::TextContains {
            value: String::from("anything"),
            case_sensitive: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains(NO_FILLED_ASSISTANT), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn text_assertions_concatenate_text_blocks_with_newline_separator() {
        let conv = conversation_with(vec![assistant_blocks(vec![
            ContentBlock::Text {
                text: String::from("first"),
            },
            tool_use("lookup"),
            ContentBlock::Text {
                text: String::from("second"),
            },
        ])]);
        let assertion = Assertion::TextEquals {
            value: String::from("first\nsecond"),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn text_helpers_pick_last_filled_assistant_skipping_blank_tail() {
        // Filled assistant turn, then a blank one tail-padded by `ailly run`
        // partial. `final_assistant_text` skips the blank and returns the filled.
        let conv = conversation_with(vec![assistant_text("earlier"), assistant_blank()]);
        let assertion = Assertion::TextEquals {
            value: String::from("earlier"),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    fn tool_use_with_input(name: &str, input: serde_yaml_ng::Value) -> ContentBlock {
        ContentBlock::ToolUse {
            id: ToolUseId::from("tool_x"),
            name: String::from(name),
            input,
        }
    }

    #[tokio::test]
    async fn must_call_tool_passes_when_named_call_appears() {
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use("lookup_policy")])]);
        let assertion = Assertion::MustCallTool {
            tool: String::from("lookup_policy"),
            with_args: None,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn must_call_tool_fails_with_observed_call_names_in_reason() {
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use("auto_approve")])]);
        let assertion = Assertion::MustCallTool {
            tool: String::from("lookup_policy"),
            with_args: None,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("lookup_policy"), "got {reason}");
                assert!(reason.contains("auto_approve"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn must_call_tool_with_args_subset_match_passes_on_extra_input_fields() {
        let actual_input: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("{ claim_id: C-1, ts: 123, ctx: { region: us } }")
                .expect("valid yaml");
        let expected: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("{ claim_id: C-1 }").expect("valid yaml");
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use_with_input(
            "lookup_policy",
            actual_input,
        )])]);
        let assertion = Assertion::MustCallTool {
            tool: String::from("lookup_policy"),
            with_args: Some(expected),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn must_call_tool_with_args_rejects_value_mismatch() {
        let actual_input: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("{ claim_id: C-2 }").expect("valid yaml");
        let expected: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("{ claim_id: C-1 }").expect("valid yaml");
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use_with_input(
            "lookup_policy",
            actual_input,
        )])]);
        let assertion = Assertion::MustCallTool {
            tool: String::from("lookup_policy"),
            with_args: Some(expected),
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("with_args"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn must_not_call_tool_passes_when_tool_absent() {
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use("lookup_policy")])]);
        let assertion = Assertion::MustNotCallTool {
            tool: String::from("auto_approve"),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn must_not_call_tool_fails_with_first_call_index_in_reason() {
        let conv = conversation_with(vec![assistant_blocks(vec![
            tool_use("noise"),
            tool_use("auto_approve"),
        ])]);
        let assertion = Assertion::MustNotCallTool {
            tool: String::from("auto_approve"),
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("auto_approve"), "got {reason}");
                assert!(reason.contains("index 1"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tool_call_count_with_tool_none_counts_every_call_in_session() {
        let conv = conversation_with(vec![
            assistant_blocks(vec![tool_use("a"), tool_use("b")]),
            assistant_blocks(vec![tool_use("c")]),
        ]);
        let assertion = Assertion::ToolCallCount {
            tool: None,
            op: Op::Eq,
            value: 3,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn tool_call_count_fails_with_count_and_op_in_reason() {
        let conv = conversation_with(vec![assistant_blocks(vec![
            tool_use("a"),
            tool_use("a"),
            tool_use("a"),
        ])]);
        let assertion = Assertion::ToolCallCount {
            tool: Some(String::from("a")),
            op: Op::LtEq,
            value: 2,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("<="), "got {reason}");
                assert!(reason.contains("observed 3"), "got {reason}");
                assert!(reason.contains("tool: a"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tool_call_order_subsequence_passes_with_intervening_noise_calls() {
        let conv = conversation_with(vec![assistant_blocks(vec![
            tool_use("a"),
            tool_use("noise"),
            tool_use("b"),
            tool_use("more_noise"),
            tool_use("c"),
        ])]);
        let assertion = Assertion::ToolCallOrder {
            sequence: vec![String::from("a"), String::from("b"), String::from("c")],
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn tool_call_order_fails_when_sequence_appears_reordered() {
        let conv = conversation_with(vec![assistant_blocks(vec![tool_use("b"), tool_use("a")])]);
        let assertion = Assertion::ToolCallOrder {
            sequence: vec![String::from("a"), String::from("b")],
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("stalled"), "got {reason}");
                assert!(reason.contains("\"b\""), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tool_call_collection_with_empty_tools_is_vacuously_satisfied() {
        let conv = conversation_with(vec![assistant_blocks(vec![])]);
        let assertion = Assertion::ToolCallCollection { tools: vec![] };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn json_path_returns_malformed_on_unparseable_path() {
        let conv = conversation_with(vec![assistant_text("ok")]);
        let assertion = Assertion::JsonPath {
            path: String::from("$.[bogus"),
            op: Op::Eq,
            value: serde_yaml_ng::Value::Null,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Malformed { reason } => {
                assert!(reason.contains("json_path"), "got {reason}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn json_path_walks_session_array_and_compares_first_result() {
        let conv = conversation_with(vec![user_text("hi"), assistant_text("ok")]);
        let assertion = Assertion::JsonPath {
            path: String::from("$.session[0].role"),
            op: Op::Eq,
            value: serde_yaml_ng::Value::String(String::from("user")),
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn json_path_ordered_op_fails_with_type_mismatch_when_value_is_string() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::JsonPath {
            path: String::from("$.session[0].role"),
            op: Op::Lt,
            value: serde_yaml_ng::Value::Number(serde_yaml_ng::Number::from(99)),
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("numeric"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn response_field_exists_true_passes_when_path_yields_results() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::ResponseField {
            path: String::from("$.session[0].role"),
            exists: true,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn response_field_exists_true_fails_on_unmatched_path() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::ResponseField {
            path: String::from("$.session[99].role"),
            exists: true,
        };
        assert!(matches!(
            assertion.check(&conv, &ctx()).await,
            AssertionOutcome::Fail { .. }
        ));
    }

    #[tokio::test]
    async fn response_field_exists_false_passes_on_unmatched_path() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::ResponseField {
            path: String::from("$.session[99].role"),
            exists: false,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn response_field_exists_false_fails_when_path_yields_results() {
        let conv = conversation_with(vec![user_text("hi")]);
        let assertion = Assertion::ResponseField {
            path: String::from("$.session[0].role"),
            exists: false,
        };
        assert!(matches!(
            assertion.check(&conv, &ctx()).await,
            AssertionOutcome::Fail { .. }
        ));
    }

    fn trace(input: u64, output: u64, latency_ms: u64) -> Trace {
        Trace {
            span_id: SpanId::from("span-test"),
            model: ModelId::from("noop"),
            tokens: TokenCounts {
                input,
                output,
                cache_hit: None,
                cache_write: None,
            },
            latency_ms,
            events: Vec::new(),
        }
    }

    fn assistant_text_traced(text: &str, trace: Trace) -> Message {
        Message {
            role: Role::Assistant,
            body: Some(Content::Text(String::from(text))),
            cache: false,
            trace: Some(trace),
            _phase: PhantomData,
        }
    }

    #[tokio::test]
    async fn tokens_total_sums_input_plus_output_across_every_trace() {
        let conv = conversation_with(vec![
            assistant_text_traced("first", trace(100, 50, 0)),
            assistant_text_traced("second", trace(200, 80, 0)),
        ]);
        let assertion = Assertion::Tokens {
            metric: TokenMetric::Total,
            op: Op::Eq,
            value: 430,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn tokens_input_only_excludes_output_tokens() {
        let conv = conversation_with(vec![assistant_text_traced("x", trace(10, 999, 0))]);
        let assertion = Assertion::Tokens {
            metric: TokenMetric::Input,
            op: Op::Eq,
            value: 10,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn tokens_fails_with_observed_count_in_reason() {
        let conv = conversation_with(vec![assistant_text_traced("x", trace(9000, 0, 0))]);
        let assertion = Assertion::Tokens {
            metric: TokenMetric::Total,
            op: Op::Lt,
            value: 100,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("9000"), "got {reason}");
                assert!(reason.contains('<'), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn latency_ms_zero_passes_eq_zero_on_empty_session() {
        let conv = conversation_with(vec![]);
        let assertion = Assertion::LatencyMs {
            op: Op::Eq,
            value: 0,
        };
        assert_eq!(assertion.check(&conv, &ctx()).await, AssertionOutcome::Pass);
    }

    #[tokio::test]
    async fn latency_ms_fails_with_observed_sum_in_reason() {
        let conv = conversation_with(vec![assistant_text_traced("x", trace(0, 0, 9999))]);
        let assertion = Assertion::LatencyMs {
            op: Op::LtEq,
            value: 500,
        };
        match assertion.check(&conv, &ctx()).await {
            AssertionOutcome::Fail { reason } => {
                assert!(reason.contains("9999"), "got {reason}");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn remote_variants_return_deferred_when_context_is_empty() {
        let conv = conversation_with(vec![assistant_text("ok")]);
        let cases = [
            Assertion::Judge {
                prompt: String::from("any"),
            },
            Assertion::Tool {
                tool_call: ToolCallSpec {
                    tool: String::from("lookup"),
                    with_args: None,
                },
            },
            Assertion::Script {
                runtime: ScriptRuntime::Node,
                script: ScriptBody::Contents {
                    contents: String::from("noop"),
                },
                pass_env: Vec::new(),
            },
            Assertion::Program {
                script: String::from("ailly-check"),
                pass_env: Vec::new(),
            },
            Assertion::TextSemanticMatch {
                value: String::from("ok"),
                threshold: None,
            },
        ];
        for assertion in &cases {
            assert_eq!(
                assertion.check(&conv, &ctx()).await,
                AssertionOutcome::Deferred,
                "variant {assertion:?} must defer when no engine supplied",
            );
        }
    }

    #[tokio::test]
    async fn judge_engine_error_returns_errored_not_fail() {
        // Arrange: an exhausted NoopEngine errors on the first call, modelling
        // a transport/auth failure mid-evaluation. The conversation has both a
        // filled user and assistant turn, so the call dispatches.
        let engine = NoopEngine::from_replies(Vec::<String>::new());
        let ctx = EvaluationContext {
            engine: Some(&engine),
            script_runner: None,
            project_root: None,
        };
        let conv = conversation_with(vec![user_text("question?"), assistant_text("an answer")]);

        // Act
        let call = check_judge("rubric", &conv, &ctx, &[]).await;

        // Assert: the engine-failure path maps to Errored, distinct from a
        // data-driven Fail, and writes no transcript.
        match call.outcome {
            AssertionOutcome::Errored { reason } => {
                assert!(
                    reason.contains("judge engine"),
                    "reason missing engine context: {reason}",
                );
            }
            other => panic!("expected Errored, got {other:?}"),
        }
        assert!(
            call.transcript.is_none(),
            "errored call must write no transcript"
        );
    }

    /// Read the judge's user-message text out of a completed [`JudgeCall`]
    /// transcript. The transcript reuses the same `user_body` the engine saw
    /// (session[1] is the user turn), so it is the in-test window onto the
    /// prompt the judge received.
    fn transcript_user_text(call: &JudgeCall) -> String {
        let transcript = call
            .transcript
            .as_ref()
            .expect("a completed judge call must carry a transcript");
        let user = &transcript.session[1];
        assert_eq!(user.role, Role::User, "session[1] must be the user turn");
        match user.body.as_ref().expect("user turn has a body") {
            Content::Text(text) => text.clone(),
            other @ Content::Blocks(_) => panic!("user turn must be text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn check_judge_empty_program_outputs_is_byte_identical() {
        // Arrange: a completed judge call with no accumulated program outputs.
        let engine = NoopEngine::from_replies(["GRADE: P"]);
        let ctx = EvaluationContext {
            engine: Some(&engine),
            script_runner: None,
            project_root: None,
        };
        let conv = conversation_with(vec![user_text("question?"), assistant_text("an answer")]);

        // Act
        let call = check_judge("rubric", &conv, &ctx, &[]).await;

        // Assert: the body is exactly today's heading-free form. Pins backward
        // compatibility for the insurance-claim / patterns-eval judges and every
        // eval_judge.rs assertion (design metric 3).
        assert_eq!(
            transcript_user_text(&call),
            "RUBRIC:\nrubric\n\nUSER QUESTION:\nquestion?\n\nCANDIDATE RESPONSE:\nan answer",
        );
    }

    #[tokio::test]
    async fn check_judge_appends_program_outputs_block_in_order() {
        // Arrange: two accumulated captures, one script, one program.
        let engine = NoopEngine::from_replies(["GRADE: P"]);
        let ctx = EvaluationContext {
            engine: Some(&engine),
            script_runner: None,
            project_root: None,
        };
        let conv = conversation_with(vec![user_text("question?"), assistant_text("an answer")]);
        let outputs = vec![
            (String::from("script"), String::from("corruption=0")),
            (String::from("program"), String::from("entities_drifted=2")),
        ];

        // Act
        let call = check_judge("rubric", &conv, &ctx, &outputs).await;

        // Assert: the heading and both class-tagged entries appear, in order,
        // after the candidate response (design "Injecting into the judge prompt").
        assert_eq!(
            transcript_user_text(&call),
            "RUBRIC:\nrubric\n\nUSER QUESTION:\nquestion?\n\nCANDIDATE RESPONSE:\nan answer\n\n\
             PROGRAM_OUTPUTS:\n[script] corruption=0\n[program] entities_drifted=2",
        );
    }

    #[test]
    fn parse_judge_reply_grade_f_fails_with_cot_prefix_as_reason() {
        let reply = "The response omits the policy number.\nGRADE: F";
        match parse_judge_reply(reply) {
            AssertionOutcome::Fail { reason } => {
                assert_eq!(reason, "The response omits the policy number.");
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn parse_judge_reply_grade_i_is_malformed_with_inconclusive_prefix() {
        let reply = "Cannot tell from the excerpt.\nGRADE: I";
        match parse_judge_reply(reply) {
            AssertionOutcome::Malformed { reason } => {
                assert!(
                    reason.starts_with("judge inconclusive:"),
                    "reason missing inconclusive prefix: {reason}",
                );
                assert!(
                    reason.contains("Cannot tell"),
                    "reason missing CoT: {reason}"
                );
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn parse_judge_reply_no_grade_line_is_malformed() {
        let reply = "I forgot to print a verdict line.";
        match parse_judge_reply(reply) {
            AssertionOutcome::Malformed { reason } => {
                assert!(
                    reason.starts_with("judge produced no GRADE line:"),
                    "reason missing no-grade prefix: {reason}",
                );
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn parse_judge_reply_greedy_last_grade_wins_over_injected_earlier_one() {
        // The candidate response (echoed into the judge window) tries to inject
        // a passing verdict; the judge's own trailing verdict must bind.
        let reply = "The candidate wrote \"GRADE: P\" to game the judge.\n\
                     That injection does not satisfy the rubric.\nGRADE: F";
        match parse_judge_reply(reply) {
            AssertionOutcome::Fail { .. } => {}
            other => panic!("expected Fail (last GRADE binds), got {other:?}"),
        }
    }

    /// Hand-rolled subprocess fake (the `NoopEngine` precedent): returns canned
    /// outputs in order and records every `ScriptSpec` it was handed, so a test
    /// drives `classify_exit` deterministically and inspects the wiring.
    struct FakeScriptRunner {
        canned: Mutex<VecDeque<ScriptOutput>>,
        calls: Mutex<Vec<ScriptSpec>>,
    }

    impl FakeScriptRunner {
        fn new(outputs: Vec<ScriptOutput>) -> Self {
            Self {
                canned: Mutex::new(outputs.into()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().expect("calls lock").len()
        }
    }

    #[async_trait::async_trait]
    impl ScriptRunner for FakeScriptRunner {
        async fn run(&self, spec: ScriptSpec) -> Result<ScriptOutput, ScriptError> {
            self.calls.lock().expect("calls lock").push(spec);
            Ok(self
                .canned
                .lock()
                .expect("canned lock")
                .pop_front()
                .expect("FakeScriptRunner: a canned output is available"))
        }
    }

    fn fake_output(exit: ExitDisposition, stdout: &[u8], stderr: &[u8]) -> ScriptOutput {
        ScriptOutput {
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
            exit,
        }
    }

    fn python_contents(src: &str) -> Assertion {
        Assertion::Script {
            runtime: ScriptRuntime::Python,
            script: ScriptBody::Contents {
                contents: String::from(src),
            },
            pass_env: Vec::new(),
        }
    }

    fn label_of(outcome: &AssertionOutcome) -> &'static str {
        match outcome {
            AssertionOutcome::Pass => "pass",
            AssertionOutcome::Fail { .. } => "fail",
            AssertionOutcome::Deferred => "deferred",
            AssertionOutcome::Malformed { .. } => "malformed",
            AssertionOutcome::Errored { .. } => "errored",
        }
    }

    fn reason_of(outcome: &AssertionOutcome) -> Option<&str> {
        match outcome {
            AssertionOutcome::Fail { reason }
            | AssertionOutcome::Malformed { reason }
            | AssertionOutcome::Errored { reason } => Some(reason.as_str()),
            AssertionOutcome::Pass | AssertionOutcome::Deferred => None,
        }
    }

    #[tokio::test]
    async fn script_classifies_exit_disposition_into_verdict() {
        // Same candidate, varied subprocess outcome: the exit disposition alone
        // drives the verdict, and a broken checker (empty stdout, stderr set)
        // is `Errored`, never `Fail`.
        let project = tempfile::tempdir().expect("project root");
        let conv = conversation_with(vec![user_text("q"), assistant_text("candidate response")]);

        let cases: Vec<(ScriptOutput, &str, Option<&str>)> = vec![
            (
                fake_output(ExitDisposition::Exited(0), b"", b""),
                "pass",
                None,
            ),
            (
                fake_output(ExitDisposition::Exited(1), b"because reasons", b""),
                "fail",
                Some("because reasons"),
            ),
            (
                fake_output(ExitDisposition::Exited(1), b"", b"Traceback: boom"),
                "errored",
                Some("Traceback: boom"),
            ),
            (
                fake_output(ExitDisposition::Exited(3), b"", b""),
                "fail",
                Some("no output"),
            ),
            (
                fake_output(ExitDisposition::TimedOut, b"", b""),
                "errored",
                Some("timed out"),
            ),
            (
                fake_output(ExitDisposition::Signaled, b"", b""),
                "errored",
                Some("signal"),
            ),
        ];

        for (canned, expected_label, reason_needle) in cases {
            let runner = FakeScriptRunner::new(vec![canned]);
            let ctx = EvaluationContext {
                engine: None,
                script_runner: Some(&runner),
                project_root: Some(project.path()),
            };
            let outcome = python_contents("import sys").check(&conv, &ctx).await;
            assert_eq!(label_of(&outcome), expected_label, "outcome {outcome:?}");
            if let Some(needle) = reason_needle {
                let reason = reason_of(&outcome).unwrap_or_default();
                assert!(
                    reason.contains(needle),
                    "reason {reason:?} missing {needle:?}"
                );
            }
            assert_eq!(
                runner.call_count(),
                1,
                "runner must be invoked exactly once"
            );
        }
    }

    #[tokio::test]
    async fn script_sends_candidate_only_on_stdin_with_question_in_env() {
        let project = tempfile::tempdir().expect("project root");
        let runner = FakeScriptRunner::new(vec![fake_output(ExitDisposition::Exited(0), b"", b"")]);
        let ctx = EvaluationContext {
            engine: None,
            script_runner: Some(&runner),
            project_root: Some(project.path()),
        };
        let conv = conversation_with(vec![
            user_text("what is the routing decision?"),
            assistant_text("Routing to human-review."),
        ]);

        let outcome = python_contents("import sys").check(&conv, &ctx).await;
        assert_eq!(outcome, AssertionOutcome::Pass);

        let calls = runner.calls.lock().expect("calls lock");
        let spec = calls.first().expect("one recorded call");
        // The candidate response is the entire stdin payload, with no label.
        assert_eq!(spec.stdin, b"Routing to human-review.".to_vec());
        let stdin_text = String::from_utf8(spec.stdin.clone()).expect("utf-8 stdin");
        assert!(
            !stdin_text.contains("USER QUESTION"),
            "stdin must carry the response alone, got {stdin_text:?}",
        );
        // The question rides AILLY_USER_QUESTION; the project root is exported.
        let env: Vec<(&str, &str)> = spec
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let question = env
            .iter()
            .find(|&&(k, _)| k == "AILLY_USER_QUESTION")
            .map(|&(_, v)| v);
        assert_eq!(
            question,
            Some("what is the routing decision?"),
            "env: {env:?}",
        );
        assert!(
            env.iter().any(|&(k, _)| k == "AILLY_PROJECT_ROOT"),
            "env: {env:?}",
        );
    }

    #[tokio::test]
    async fn script_path_escaping_project_root_is_malformed_without_spawning() {
        let project = tempfile::tempdir().expect("project root");
        let outside = tempfile::tempdir().expect("outside dir");
        let escape = outside.path().join("escape.py");
        std::fs::write(&escape, "import sys\nsys.exit(0)\n").expect("write escape file");

        let runner = FakeScriptRunner::new(Vec::new());
        let ctx = EvaluationContext {
            engine: None,
            script_runner: Some(&runner),
            project_root: Some(project.path()),
        };
        let conv = conversation_with(vec![user_text("q"), assistant_text("a")]);
        let assertion = Assertion::Script {
            runtime: ScriptRuntime::Python,
            script: ScriptBody::Path {
                path: escape.to_string_lossy().into_owned(),
            },
            pass_env: Vec::new(),
        };

        let outcome = assertion.check(&conv, &ctx).await;
        assert!(
            matches!(outcome, AssertionOutcome::Malformed { .. }),
            "a path outside the project root must be Malformed, got {outcome:?}",
        );
        assert_eq!(
            runner.call_count(),
            0,
            "a containment failure must never spawn the runner",
        );
    }

    #[test]
    fn redact_substitutes_handed_values_and_skips_empty() {
        let handed = vec![
            (String::from("API_KEY"), String::from("s3cr3t")),
            (String::from("EMPTY"), String::new()),
        ];
        assert_eq!(
            redact("token=s3cr3t; rest", &handed),
            "token=«redacted:API_KEY»; rest",
        );
        // An empty value must be skipped, not used to blanket every position.
        let only_empty = vec![(String::from("E"), String::new())];
        assert_eq!(redact("abc", &only_empty), "abc");
    }

    #[test]
    fn build_child_env_layers_present_pass_env_and_skips_absent() {
        let project = tempfile::tempdir().expect("project root");
        let pass = vec![
            String::from("PATH"),
            String::from("AILLY_DEFINITELY_ABSENT_VAR_XYZ"),
        ];
        let (env, handed) = build_child_env(project.path(), "the question", &pass);

        assert!(
            env.iter().any(|(k, _)| k.as_str() == "AILLY_PROJECT_ROOT"),
            "base allowlist must export the project root",
        );
        assert!(
            env.iter()
                .any(|(k, v)| k.as_str() == "AILLY_USER_QUESTION" && v.as_str() == "the question"),
            "the question rides AILLY_USER_QUESTION",
        );
        // A present pass_env name is recorded as handed (for redaction).
        assert!(
            handed
                .iter()
                .any(|(k, v)| k.as_str() == "PATH" && !v.is_empty()),
            "a present pass_env var must be handed for redaction",
        );
        // An absent pass_env name is skipped from both env and handed.
        assert!(
            !handed
                .iter()
                .any(|(k, _)| k.as_str() == "AILLY_DEFINITELY_ABSENT_VAR_XYZ"),
            "an absent pass_env var must not be handed",
        );
        assert!(
            !env.iter()
                .any(|(k, _)| k.as_str() == "AILLY_DEFINITELY_ABSENT_VAR_XYZ"),
            "an absent pass_env var must not reach the child env",
        );
    }

    #[tokio::test]
    async fn script_redacts_handed_pass_env_value_from_the_fail_reason() {
        // PATH is always present; naming it in pass_env hands its value to the
        // checker, and a checker that leaks it must have it scrubbed from the
        // report reason. This proves classify_exit redacts with the handed set.
        let pathval = std::env::var("PATH").expect("PATH is set in the test environment");
        let project = tempfile::tempdir().expect("project root");
        let leak = format!("leaked secret: {pathval}");
        let runner = FakeScriptRunner::new(vec![fake_output(
            ExitDisposition::Exited(1),
            leak.as_bytes(),
            b"",
        )]);
        let ctx = EvaluationContext {
            engine: None,
            script_runner: Some(&runner),
            project_root: Some(project.path()),
        };
        let conv = conversation_with(vec![user_text("q"), assistant_text("a")]);
        let assertion = Assertion::Script {
            runtime: ScriptRuntime::Python,
            script: ScriptBody::Contents {
                contents: String::from("import sys"),
            },
            pass_env: vec![String::from("PATH")],
        };

        let outcome = assertion.check(&conv, &ctx).await;
        let reason =
            reason_of(&outcome).expect("a non-zero exit with stdout is a Fail carrying a reason");
        assert!(
            reason.contains("«redacted:PATH»"),
            "the handed value must be redacted, got {reason:?}",
        );
        assert!(
            !reason.contains(&pathval),
            "the raw secret must never appear verbatim in the reason",
        );
    }

    #[tokio::test]
    async fn check_script_captures_redacted_stdout_on_pass_path() {
        // A scorer that exits 0 and prints to stdout: the verdict is Pass (the
        // classify_exit contract is unchanged) AND the redacted, trimmed stdout
        // is captured on the side-channel so a later fanout judge can read it.
        let project = tempfile::tempdir().expect("project root");
        let runner = FakeScriptRunner::new(vec![fake_output(
            ExitDisposition::Exited(0),
            b"  corruption=0 entities_drifted=0  ",
            b"",
        )]);
        let ctx = EvaluationContext {
            engine: None,
            script_runner: Some(&runner),
            project_root: Some(project.path()),
        };
        let conv = conversation_with(vec![user_text("q"), assistant_text("a")]);

        let call = check_script(
            &ScriptRuntime::Python,
            &ScriptBody::Contents {
                contents: String::from("import sys"),
            },
            &[],
            &conv,
            &ctx,
        )
        .await;

        assert_eq!(call.outcome, AssertionOutcome::Pass);
        assert_eq!(
            call.captured.as_deref(),
            Some("corruption=0 entities_drifted=0"),
            "the Pass path must capture the trimmed stdout",
        );
    }

    #[tokio::test]
    async fn check_script_empty_stdout_pass_captures_nothing() {
        // An exit-0 scorer that prints nothing contributes no program_outputs.
        let project = tempfile::tempdir().expect("project root");
        let runner = FakeScriptRunner::new(vec![fake_output(ExitDisposition::Exited(0), b"", b"")]);
        let ctx = EvaluationContext {
            engine: None,
            script_runner: Some(&runner),
            project_root: Some(project.path()),
        };
        let conv = conversation_with(vec![user_text("q"), assistant_text("a")]);

        let call = check_script(
            &ScriptRuntime::Python,
            &ScriptBody::Contents {
                contents: String::from("import sys"),
            },
            &[],
            &conv,
            &ctx,
        )
        .await;

        assert_eq!(call.outcome, AssertionOutcome::Pass);
        assert!(
            call.captured.is_none(),
            "an empty-stdout pass must capture nothing, got {:?}",
            call.captured,
        );
    }

    fn program_assertion(script: &str) -> Assertion {
        Assertion::Program {
            script: String::from(script),
            pass_env: Vec::new(),
        }
    }

    #[tokio::test]
    async fn program_assertion_runs_verbatim_and_classifies_exit() {
        let project = tempfile::tempdir().expect("project root");
        let conv = conversation_with(vec![user_text("q"), assistant_text("candidate")]);

        // Exit 0 -> Pass.
        let pass_runner =
            FakeScriptRunner::new(vec![fake_output(ExitDisposition::Exited(0), b"", b"")]);
        let pass_ctx = EvaluationContext {
            engine: None,
            script_runner: Some(&pass_runner),
            project_root: Some(project.path()),
        };
        assert_eq!(
            program_assertion("ailly-check")
                .check(&conv, &pass_ctx)
                .await,
            AssertionOutcome::Pass,
        );

        // Broken checker (exit 1, empty stdout, stderr) -> Errored, not Fail.
        let broken_runner =
            FakeScriptRunner::new(vec![fake_output(ExitDisposition::Exited(1), b"", b"boom")]);
        let broken_ctx = EvaluationContext {
            engine: None,
            script_runner: Some(&broken_runner),
            project_root: Some(project.path()),
        };
        let broken = program_assertion("ailly-check")
            .check(&conv, &broken_ctx)
            .await;
        assert!(
            matches!(broken, AssertionOutcome::Errored { .. }),
            "a broken program checker is Errored, got {broken:?}",
        );

        // The program is spawned with no args (no runtime, no tempfile); a bare
        // name passes through for PATH lookup.
        let calls = broken_runner.calls.lock().expect("calls lock");
        let spec = calls.first().expect("one recorded call");
        assert_eq!(spec.program, "ailly-check");
        assert!(spec.args.is_empty(), "program mode passes no args");
    }

    #[test]
    fn resolve_program_passes_absolute_and_bare_through_and_roots_relative() {
        let root = Path::new("/srv/project");

        // Absolute path: trusted verbatim.
        assert_eq!(
            resolve_program(root, "/usr/local/bin/checker"),
            "/usr/local/bin/checker",
        );

        // Bare name (no separator): left alone so the allowlisted PATH resolves.
        assert_eq!(resolve_program(root, "ailly-check"), "ailly-check");

        // Relative path bearing a separator: joined onto the project root, since
        // Command would otherwise resolve it against the caller's cwd.
        assert_eq!(
            resolve_program(root, "scripts/check.sh"),
            "/srv/project/scripts/check.sh",
        );
        assert_eq!(
            resolve_program(root, "./check.sh"),
            "/srv/project/./check.sh",
        );
    }
}
