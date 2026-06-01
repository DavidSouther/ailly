//! Feature test for the patterns-eval invocation checkers.
//!
//! User story: a maintainer edits one of the `patterns:*` SKILL.md files and
//! wants the regression suite to notice when the edit rewords the structural
//! guidance out of the skill. The mechanism is the per-skill invocation
//! checker: a focused structural validator over TypeScript that the eval
//! runner spawns with the candidate code on stdin. The checker's job is to
//! *discriminate* — pass code that exhibits the pattern, fail code that omits
//! the structure the SKILL.md teaches.
//!
//! Each checker, run directly:
//!
//!   - exits 0 on a conforming TypeScript sample that exhibits the pattern in
//!     full, and
//!   - exits 1 on a sample that violates one of the SKILL.md "Common Mistakes"
//!     rules, printing a single-line human-readable reason to stdout and
//!     leaving stderr empty.
//!
//! The empty-stderr-on-Fail contract matters: the runner classifies a non-zero
//! exit with empty stdout and non-empty stderr as `Errored` (a broken checker),
//! never as a failed candidate. A real Fail therefore speaks only on stdout.
//!
//! This discrimination is what the README's falsification property rests on:
//! the `invocation` arm (skill loaded) passes assertions the `baseline` arm (no
//! skill) fails. A checker that cannot fail un-skilled output makes
//! `improved == 0` and the falsification claim vacuous.
//!
//! Fails until the three placeholder checkers under
//! `e2e/patterns-eval/evals/scripts/` are replaced with real structural
//! validators. The placeholders read stdin and exit 0 unconditionally, so the
//! violating samples below do not yet exit 1.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

/// The interpreter under test. `AILLY_PYTHON` overrides `python3` for CI
/// parity with the executor's `resolve_runtime_binary`.
fn python() -> String {
    std::env::var("AILLY_PYTHON").unwrap_or_else(|_| String::from("python3"))
}

fn checker_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("e2e")
        .join("patterns-eval")
        .join("evals")
        .join("scripts")
        .join(name)
}

struct CheckerOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Run a checker as the eval runner does: the candidate code on stdin, `cwd`
/// at the project root. Returns the exit code and captured streams.
fn run_checker(script: &str, candidate: &str) -> CheckerOutput {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("e2e")
        .join("patterns-eval");

    let mut child = Command::new(python())
        .arg(checker_path(script))
        .current_dir(&project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn checker");

    child
        .stdin
        .take()
        .expect("checker stdin")
        .write_all(candidate.as_bytes())
        .expect("write candidate to checker stdin");

    let output = child.wait_with_output().expect("wait for checker");
    CheckerOutput {
        code: output.status.code().expect("checker exited normally"),
        stdout: String::from_utf8(output.stdout).expect("checker stdout is utf-8"),
        stderr: String::from_utf8(output.stderr).expect("checker stderr is utf-8"),
    }
}

/// One conforming sample (must Pass) and one rule-violating sample (must Fail)
/// per checker. The violating sample is shaped like baseline-arm output: it
/// omits exactly the structure its SKILL.md teaches.
struct Discrimination {
    script: &'static str,
    conforming: &'static str,
    violating: &'static str,
}

// --- newtype ----------------------------------------------------------------
//
// Conforming: a brand intersection, the single sanctioned `as` cast confined
// to the constructor, and a named constructor returning the branded type.
// Violating: a plain `type X = string` alias with no brand — the "plain type
// alias instead of a brand" mistake.

const NEWTYPE_CONFORMING: &str = r#"
type UserId = string & { readonly __brand: "UserId" };

export function makeUserId(raw: string): UserId {
  if (raw.length === 0) {
    throw new Error("empty UserId");
  }
  return raw as UserId;
}

export function loadUser(id: UserId): void {
  console.log(id);
}
"#;

const NEWTYPE_VIOLATING: &str = r"
type UserId = string;

export function loadUser(id: UserId): void {
  console.log(id);
}
";

// --- configuring-logging -----------------------------------------------------
//
// Conforming: one bootstrap entry point that installs a registry with all four
// Format/Filter/Enrich/Export layers, `service.*` resource attributes, and a
// shutdown flush. Violating: only Format and Filter — the "three layers instead
// of five" mistake (Enrich and Export absent, no flush).

const CONFIGURING_CONFORMING: &str = r#"
import { Registry, Format, Filter, Enrich, Export } from "./logging";

export function initLogging(): void {
  const registry = new Registry()
    .with(Format.json())
    .with(Filter.fromEnv())
    .with(Enrich.resource({ "service.name": "hello", "service.version": "1.0.0" }))
    .with(Export.otlp({ endpoint: "http://collector:4317" }));
  registry.install();
  process.on("SIGTERM", () => registry.shutdown(5000));
}
"#;

const CONFIGURING_VIOLATING: &str = r#"
import { Registry, Format, Filter } from "./logging";

export function initLogging(): void {
  const registry = new Registry()
    .with(Format.json())
    .with(Filter.fromEnv());
  registry.install();
}
"#;

// --- emitting-logs -----------------------------------------------------------
//
// Conforming: a stable message body, `eventName` set, and the semantic-
// convention keys order.id / user.id / http.response.status_code attached as
// fields. Violating: the values interpolated into the message body — the
// "string interpolation in the message body" mistake.

const EMITTING_CONFORMING: &str = r#"
logger.info(
  {
    eventName: "order.placed",
    "order.id": order.id,
    "user.id": user.id,
    "http.response.status_code": res.statusCode,
  },
  "order placed",
);
"#;

const EMITTING_VIOLATING: &str = r"
logger.info(`order ${order.id} placed for ${user.id} -> ${res.statusCode}`);
";

const DISCRIMINATIONS: &[Discrimination] = &[
    Discrimination {
        script: "check_newtype.py",
        conforming: NEWTYPE_CONFORMING,
        violating: NEWTYPE_VIOLATING,
    },
    Discrimination {
        script: "check_configuring_logging.py",
        conforming: CONFIGURING_CONFORMING,
        violating: CONFIGURING_VIOLATING,
    },
    Discrimination {
        script: "check_emitting_logs.py",
        conforming: EMITTING_CONFORMING,
        violating: EMITTING_VIOLATING,
    },
];

#[test]
fn each_checker_discriminates_conforming_from_violating_typescript() {
    for case in DISCRIMINATIONS {
        let pass = run_checker(case.script, case.conforming);
        assert_eq!(
            pass.code, 0,
            "{} must exit 0 on conforming TypeScript, got {} (stdout: {:?}, stderr: {:?})",
            case.script, pass.code, pass.stdout, pass.stderr,
        );
        assert!(
            pass.stderr.is_empty(),
            "{} must leave stderr empty on a conforming sample, got {:?}",
            case.script,
            pass.stderr,
        );

        let fail = run_checker(case.script, case.violating);
        assert_eq!(
            fail.code, 1,
            "{} must exit 1 on a rule-violating sample, got {} (stdout: {:?}, stderr: {:?})",
            case.script, fail.code, fail.stdout, fail.stderr,
        );
        assert!(
            fail.stderr.is_empty(),
            "{} must leave stderr empty on a normal Fail so the runner does not \
             misclassify it as Errored, got {:?}",
            case.script,
            fail.stderr,
        );
        let reason = fail.stdout.trim_end_matches('\n');
        assert!(
            !reason.is_empty(),
            "{} must print a human-readable reason on stdout when it fails",
            case.script,
        );
        assert!(
            !reason.contains('\n'),
            "{} must print a single-line reason on Fail, got {:?}",
            case.script,
            fail.stdout,
        );
    }
}
