//! Feature test for the `knowledge: eval-script` slice.
//!
//! User story: an eval-suite author writes a Python validator that reads the
//! candidate's final response on stdin and exits 0 to pass or non-zero to
//! fail, printing its reason on stdout. They wire a real script runner and a
//! project root into the `EvaluationContext` and run `ailly eval`. The two
//! `script` assertions execute as real subprocesses, and the report tallies
//! their verdicts by exit code rather than returning `Deferred`.
//!
//! This test drives the happy-path core of the contract end-to-end through a
//! real `python3`:
//!
//!   - `script_runner` and `project_root` are both present in the context, so
//!     the dispatch is no longer the blanket `Deferred` arm — a real subprocess
//!     is spawned for each `script` assertion.
//!   - Two checkers run against the *same* candidate conversation. They differ
//!     only in what they look for, so the exit code — not a hardcoded outcome —
//!     is what decides `Pass` vs `Fail`. A checker that finds `human-review`
//!     exits 0 (`Pass`); a checker that fails to find `auto-approve` prints a
//!     reason and exits 1 (`Fail`).
//!   - The candidate's final assistant text is the entire stdin payload (the
//!     user question is not on stdin), so a checker greps the response alone.
//!   - The failing checker's stdout becomes the `Fail` reason in the report,
//!     proving the subprocess actually ran and its output flowed end-to-end.
//!   - Totals show `passed: 1`, `failed: 1`, `deferred: 0`; the `script`
//!     per-class bucket shows one pass and one fail.
//!
//! Failure-mode and contract variations split across two homes. The runner's
//! own machinery — the timeout race (`TimedOut`), kill/reap of a signalled
//! child (`Signaled`), the bounded output-pipe drain when a grandchild holds a
//! pipe open, and the output-size cap — is exercised against a real
//! interpreter in `tests/script_runner.rs`. The executor's classification and
//! confinement behaviour — the broken-checker `Errored` split (non-zero exit,
//! empty stdout, stderr traceback), `ScriptBody::Path` containment (`Malformed`
//! on escape), the cleared-env confinement and per-assertion `pass_env`
//! opt-in, captured-output redaction, the Node runtime, and the fail-closed
//! `Deferred` gate — is covered by the unit tests in
//! `src/knowledge/assertions.rs` and the gate unit test in `src/cli/eval.rs`,
//! per the design's testing strategy.

use std::marker::PhantomData;
use std::path::PathBuf;

use ailly_two::content::conversation::BindingMap;
use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::Meta;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::content::evaluation::Evaluation;
use ailly_two::knowledge::assertions::EvaluationContext;
use ailly_two::knowledge::eval::EvalArgs;
use ailly_two::knowledge::eval::evaluate;
use ailly_two::knowledge::script_runner::TokioScriptRunner;

/// Two `script` cases over one candidate. The first checker passes (the
/// response routes to `human-review`); the second fails (the response carries
/// no `auto-approve` marker) and prints its reason on stdout. Inline Python via
/// a YAML literal block scalar keeps the checker readable and quote-free.
const SUITE_YAML: &str = r#"
name: regression
cases:
  - name: routing
    assertions:
      - type: script
        runtime: python
        script:
          contents: |
            import sys
            response = sys.stdin.read()
            sys.exit(0 if "human-review" in response else 1)
      - type: script
        runtime: python
        script:
          contents: |
            import sys
            response = sys.stdin.read()
            ok = "auto-approve" in response
            if not ok:
                sys.stdout.write("expected auto-approve marker; candidate routed elsewhere")
            sys.exit(0 if ok else 1)
"#;

#[tokio::test]
async fn script_assertions_run_real_subprocesses_and_tally_pass_and_fail_by_exit_code() {
    // Arrange: the suite, the candidate conversation under test, and a real
    // Tokio script runner plus a project root. The assistant turn is the
    // candidate response the checkers read on stdin; it mentions `human-review`
    // and deliberately omits `auto-approve`.
    let suite = Evaluation::from_yaml_str(SUITE_YAML).expect("regression suite parses");
    let conv_path = PathBuf::from("routing.yaml");
    let conv = conversation_with(vec![
        user_text("claim narrative: policy C-1, requested payout 12,500 USD"),
        assistant_text(
            "Per policy C-1 the requested payout exceeds the documented threshold. \
             Routing this claim to human-review.",
        ),
    ]);

    let runner = TokioScriptRunner;
    // The checkers use `ScriptBody::Contents`, but `check_script` defers unless
    // both collaborators are present, so a real project root is required.
    let project_root = tempfile::tempdir().expect("tempdir for project root");
    let ctx = EvaluationContext {
        engine: None,
        script_runner: Some(&runner),
        project_root: Some(project_root.path()),
    };

    // Act: run the orchestrator over the single candidate.
    let report = evaluate(EvalArgs {
        suite: &suite,
        conversations: &[(conv_path, conv)],
        ctx,
        suite_name: "regression",
        run_id: "2026-05-30T14-00-handler",
        judge_output_dir: None,
    })
    .await;

    // Assert: the scripts ran for real. One passed, one failed, none deferred.
    assert_eq!(
        report.totals.assertions.deferred, 0,
        "no script may defer when a runner and project root are wired; totals {:#?}",
        report.totals.assertions,
    );
    assert_eq!(
        report.totals.assertions.passed, 1,
        "the human-review checker exits 0 and must Pass; totals {:#?}",
        report.totals.assertions,
    );
    assert_eq!(
        report.totals.assertions.failed, 1,
        "the auto-approve checker exits 1 and must Fail; totals {:#?}",
        report.totals.assertions,
    );

    // Assert: the per-class `script` bucket splits the same way, proving the
    // exit code — not a fixed outcome — drove each verdict.
    let script_bucket = report
        .per_class
        .get("script")
        .copied()
        .expect("per-class rollup must include the `script` class once scripts run");
    assert_eq!(script_bucket.passed, 1, "script bucket {script_bucket:#?}");
    assert_eq!(script_bucket.failed, 1, "script bucket {script_bucket:#?}");

    // Assert: the failing checker's stdout flowed end-to-end into the report's
    // Fail reason. This is the proof the subprocess truly executed.
    let fail_reason = report
        .cases
        .iter()
        .flat_map(|case| &case.matches)
        .flat_map(|m| &m.assertions)
        .find(|a| a.outcome == "fail")
        .and_then(|a| a.reason.as_deref())
        .expect("a failing script assertion must carry a reason in the report");
    assert!(
        fail_reason.contains("expected auto-approve marker"),
        "the Fail reason must replay the checker's stdout, got {fail_reason:?}",
    );
}

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
