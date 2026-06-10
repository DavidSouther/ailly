//! Feature test for the `skill-forge` loop, worked through one forged skill:
//! **Clean Comments Review**.
//!
//! User story: a developer hands `skill-forge` the intent for a comment-review
//! skill. `skill-forge` delegates the `SKILL.md` draft to the authoring role,
//! then scaffolds an `ailly-skill-eval` harness at `e2e/clean-comments-review/`
//! that references the authored body **in place** via a `kind: external` prefix
//! block (no vendored copy), and drives `assemble`/`run`/`eval`/`report`. The
//! forge is done when the falsification gate goes green:
//!
//!   * discovery routes the situation to `clean-comments-review` (routing
//!     metric — every discovery assertion passes, nothing fails or errors);
//!   * the invocation arm beats the baseline arm on the same `invocation` suite
//!     — `report <baseline> <invocation>` reports `improved > 0 && regressed ==
//!     0`;
//!   * the scaffold did not leak the answer — the skill identifier never
//!     appears in the baseline-shared prefix or the shared user prompt.
//!
//! This encodes the design's user story and metrics:
//! docs/developer/2026-06-07-A-skill-forge/design.md (the loop, "Metrics
//! (falsifiable)" #1/#3/#4, and the Same-repo `kind: external` reference).
//!
//! Like the other e2e feature tests, the arms are deterministic `model: noop`
//! conversations rather than a live model call: the judge defers without an
//! engine, the `check_clean_comments_review.py` checker still executes, and the
//! checker is what must discriminate the two arms. The live forge loop itself
//! is exercised by `skill-forge`'s own `ci.sh`, not here.
//!
//! Fails until `skill-forge` forges the harness: `skills/clean-comments-review/
//! SKILL.md`, the three assemblies (the invocation one carrying the `kind:
//! external` reference), `prompts/invocation/clean-comments-review.md`, the
//! `discovery`/`invocation` eval suites, and `evals/scripts/
//! check_clean_comments_review.py`.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use ailly_two::cli::eval::EvalCmdArgs;
use ailly_two::cli::eval::run as eval_run;
use ailly_two::cli::report::ReportCmdArgs;
use ailly_two::cli::report::ReportCmdOutcome;
use ailly_two::cli::report::ReportMode;
use ailly_two::cli::report::run as report_run;

/// The forged skill's identifier. The whole leak discipline (Metric #4) turns
/// on this string never reaching the baseline arm.
const SKILL_ID: &str = "clean-comments-review";

// --- Discovery synthetics ---------------------------------------------------
//
// Two situations whose right answer is "load clean-comments-review". Each
// assistant turn names the skill (the `text_contains` target) and avoids the
// nearest rival, a generic `code-review` skill (the `text_not_contains`
// target), so the routing assertion discriminates instead of merely matching.
// File stem == case name is how `eval` pairs a conversation to its case.

const CONV_DISCOVERY_VERBOSE_DOCBLOCK: &str = "\
---
model: noop
assembly: discovery
binding:
  case: verbose-docblock
---
role: user
content: \"A reviewer flagged that my DocBlocks spell out every current caller and will drift as soon as usage changes. Which skill applies?\"
---
role: assistant
content: \"Load clean-comments-review. It rewrites comments for their audience and cuts the rot-prone usage detail; this is not a code-correctness pass.\"
trace:
  span_id: span-ccr-d1
  model: noop
  tokens:
    input: 820
    output: 44
  latency_ms: 205
";

const CONV_DISCOVERY_AUDIENCE_MISMATCH: &str = "\
---
model: noop
assembly: discovery
binding:
  case: comment-audience-mismatch
---
role: user
content: \"I asked for a type to be documented and got a paragraph listing every place it is used today instead of what it conceptually is. Which skill applies?\"
---
role: assistant
content: \"Load clean-comments-review. The public DocBlock should state why the type exists for an external reader, not enumerate today's call sites.\"
trace:
  span_id: span-ccr-d2
  model: noop
  tokens:
    input: 835
    output: 52
  latency_ms: 210
";

// --- Invocation + baseline synthetics ---------------------------------------
//
// One case, `clean-comments-review`. Both arms answer the same comment-review
// task (the shared `prompts/invocation/clean-comments-review.md`); the only
// difference is the invocation arm has the forged `SKILL.md` loaded. The
// material under review is the real over-documented `className` DocBlock that
// motivated the skill.
//
// The invocation body is a critique that a sound
// `check_clean_comments_review.py` passes: it classifies DocBlock vs inline
// comment, flags the rot-prone usage detail, and recommends cutting it to
// intent — and it stays a critique, not an edit. The baseline body praises the
// over-documentation and recommends no change, which that same checker must
// fail. The script flip (fail -> pass) is the `improved` signal; the judge
// defers in both arms under `model: noop`.

const CONV_INVOCATION_GOOD_CRITIQUE: &str = "\
---
model: noop
assembly: invocation
binding:
  skill: clean-comments-review
---
role: user
content: \"Review the comments in this prop and report on them.\\n\\n/** Render the PanelHeader (rename / move / delete). Requires a panel store in the surrounding PanelContextProvider - the header data is read from the store, not passed in. Omit for states with no store (notFound) or panels that render their own header (the comparison panels' load-error bodies). */\\nclassName?: string;\"
---
role: assistant
content: |
  # Comment critique

  ## Public DocBlock on `className?: string`
  This is a public DocBlock, so its audience is an external reader who will
  not read the surrounding code. It fails that audience on two counts:

  - It encodes current internal usage (the `PanelContextProvider` store, the
    `notFound` state, the comparison panels' load-error bodies). That detail
    will rot the moment the call sites change, and an external reader cannot
    act on it.
  - It documents the wrong symbol entirely: the prose describes `PanelHeader`
    rendering, not the `className` string the DocBlock sits on.

  Recommendation: cut it to intent, or remove it. A `className` passthrough
  rarely needs a DocBlock at all; one line stating why it exists is the ceiling.

  ## Inline comments
  No inline line comments are present. None is warranted here: nothing about a
  `className` prop is a surprising or atypical decision, so a future reader who
  can already read the code gains nothing from one.
trace:
  span_id: span-ccr-i1
  model: noop
  tokens:
    input: 1850
    output: 320
  latency_ms: 640
";

const CONV_BASELINE_WEAK_RESPONSE: &str = "\
---
model: noop
assembly: baseline
binding:
  skill: clean-comments-review
---
role: user
content: \"Review the comments in this prop and report on them.\\n\\n/** Render the PanelHeader (rename / move / delete). Requires a panel store in the surrounding PanelContextProvider - the header data is read from the store, not passed in. Omit for states with no store (notFound) or panels that render their own header (the comparison panels' load-error bodies). */\\nclassName?: string;\"
---
role: assistant
content: \"The comments are thorough and document exactly how the prop is used today. They look helpful and complete, so no changes are needed.\"
trace:
  span_id: span-ccr-b1
  model: noop
  tokens:
    input: 1550
    output: 70
  latency_ms: 300
";

const DISCOVERY_CASES: &[(&str, &str)] = &[
    ("verbose-docblock", CONV_DISCOVERY_VERBOSE_DOCBLOCK),
    (
        "comment-audience-mismatch",
        CONV_DISCOVERY_AUDIENCE_MISMATCH,
    ),
];

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "one end-to-end forge narrative: shape checks, no-leak grep, three eval arms, and the comparison gate read clearly as a single sequence rather than split across helpers"
)]
async fn skill_forge_forges_clean_comments_review_to_a_green_gate() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("e2e")
        .join("clean-comments-review");
    let reports_dir = project.join("evals").join("reports");

    // ---- The forge produced a correctly-shaped harness ----------------------
    // The skill body is the authoritative source in the monorepo's skills/ tree;
    // the harness references it in place, never vendors it (Same-repo reference).
    let skill_body = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("skills")
        .join(SKILL_ID)
        .join("SKILL.md");
    assert!(
        skill_body.exists(),
        "forged skill body lives at skills/{SKILL_ID}/SKILL.md as the authoritative source",
    );

    let invocation_assembly =
        fs::read_to_string(project.join("assemblies").join("invocation.yaml"))
            .expect("forged invocation assembly exists");
    assert!(
        invocation_assembly.contains("kind: external"),
        "invocation assembly references the skill body via a `kind: external` block",
    );
    assert!(
        invocation_assembly.contains(&format!("skills/{SKILL_ID}/SKILL.md")),
        "the external block points at the in-place skills/{SKILL_ID}/SKILL.md",
    );
    assert!(
        !project
            .join("context")
            .join("skills")
            .join(SKILL_ID)
            .exists(),
        "no vendored copy of the skill body lives under the project's context/skills/",
    );

    // Scaffold-without-leak (Metric #4): the skill identifier never appears in
    // the baseline-shared prefix or the user prompt both arms answer.
    for shared in [
        project.join("AGENTS.md"),
        project.join("context").join("AGENTS.md"),
        project
            .join("prompts")
            .join("invocation")
            .join(format!("{SKILL_ID}.md")),
    ] {
        let text = fs::read_to_string(&shared)
            .unwrap_or_else(|_| panic!("baseline-shared file exists: {}", shared.display()));
        assert!(
            !text.contains(SKILL_ID),
            "baseline-shared file must not leak the skill id {SKILL_ID}: {}",
            shared.display(),
        );
    }

    let tmp = tempfile::tempdir().expect("tempdir");

    // ---- Discovery arm: routing is clean ------------------------------------
    let discovery_run_id = "2026-06-09T00-00-00Z-test-ccr-discovery";
    let discovery_dir = tmp.path().join(discovery_run_id);
    fs::create_dir_all(&discovery_dir).expect("create discovery run dir");
    for (name, body) in DISCOVERY_CASES {
        fs::write(discovery_dir.join(format!("{name}.yaml")), body)
            .expect("write discovery conversation");
    }

    let discovery = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("discovery"),
        over: discovery_dir.clone(),
    })
    .await
    .expect("discovery eval runs end-to-end");

    assert_eq!(
        discovery.conversations_matched, 2,
        "both discovery situations match a discovery case",
    );
    assert_eq!(
        discovery.assertions_failed, 0,
        "every discovery routing assertion passes (routing >= 0.9)",
    );
    assert_eq!(
        discovery.assertions_malformed, 0,
        "no malformed discovery assertion"
    );
    assert_eq!(
        discovery.assertions_errored, 0,
        "no errored discovery assertion"
    );

    // ---- Invocation vs baseline arms ----------------------------------------
    let baseline_run_id = "2026-06-09T00-00-00Z-test-ccr-baseline";
    let baseline_dir = tmp.path().join(baseline_run_id);
    fs::create_dir_all(&baseline_dir).expect("create baseline run dir");
    fs::write(
        baseline_dir.join(format!("{SKILL_ID}.yaml")),
        CONV_BASELINE_WEAK_RESPONSE,
    )
    .expect("write baseline conversation");

    let invocation_run_id = "2026-06-09T00-00-00Z-test-ccr-invocation";
    let invocation_dir = tmp.path().join(invocation_run_id);
    fs::create_dir_all(&invocation_dir).expect("create invocation run dir");
    fs::write(
        invocation_dir.join(format!("{SKILL_ID}.yaml")),
        CONV_INVOCATION_GOOD_CRITIQUE,
    )
    .expect("write invocation conversation");

    // Both arms are scored against the same `invocation` suite, so the four-
    // bucket comparison is apples-to-apples.
    let baseline = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("invocation"),
        over: baseline_dir.clone(),
    })
    .await
    .expect("baseline eval runs end-to-end");
    assert_eq!(baseline.conversations_matched, 1, "baseline case matched");
    assert_eq!(
        baseline.assertions_malformed, 0,
        "no malformed baseline assertion"
    );
    assert_eq!(
        baseline.assertions_errored, 0,
        "no errored baseline assertion"
    );

    let invocation = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("invocation"),
        over: invocation_dir.clone(),
    })
    .await
    .expect("invocation eval runs end-to-end");
    assert_eq!(
        invocation.conversations_matched, 1,
        "invocation case matched"
    );
    assert_eq!(
        invocation.assertions_failed, 0,
        "the skilled arm passes its suite"
    );
    assert_eq!(
        invocation.assertions_malformed, 0,
        "no malformed invocation assertion"
    );
    assert_eq!(
        invocation.assertions_errored, 0,
        "no errored invocation assertion"
    );

    // ---- The gate: improved > 0 && regressed == 0 ---------------------------
    let comparison_json = match report_run(ReportCmdArgs {
        project: project.clone(),
        mode: ReportMode::Comparison {
            run_id_a: String::from(baseline_run_id),
            run_id_b: String::from(invocation_run_id),
        },
        label_a: Some(String::from("baseline")),
        label_b: Some(String::from("invocation")),
    })
    .await
    .expect("report comparison runs end-to-end")
    {
        ReportCmdOutcome::Comparison(outcome) => outcome.comparison_json,
        ReportCmdOutcome::Single(_) => panic!("comparison mode must yield a comparison outcome"),
    };

    let comparison: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&comparison_json).expect("read comparison report"),
    )
    .expect("comparison report is valid JSON");
    let improved = comparison["totals"]["improved"]
        .as_u64()
        .expect("comparison reports an improved count");
    let regressed = comparison["totals"]["regressed"]
        .as_u64()
        .expect("comparison reports a regressed count");
    assert!(
        improved >= 1,
        "the skilled arm improves at least one assertion over baseline (got {improved})",
    );
    assert_eq!(
        regressed, 0,
        "the skilled arm regresses nothing relative to baseline",
    );

    // Keep the e2e project tree pristine.
    cleanup_reports(
        &reports_dir,
        &[
            format!("{discovery_run_id}.json"),
            format!("{baseline_run_id}.json"),
            format!("{invocation_run_id}.json"),
            format!("{baseline_run_id}-vs-{invocation_run_id}.json"),
            format!("{baseline_run_id}-vs-{invocation_run_id}.md"),
        ],
    );
}

fn cleanup_reports(reports_dir: &Path, files: &[String]) {
    for file in files {
        let _ = fs::remove_file(reports_dir.join(file));
    }
    let _ = fs::remove_dir(reports_dir);
}
