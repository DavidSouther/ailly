//! Real-subprocess coverage for the `TokioScriptRunner` machinery — the
//! timeout race, kill/reap, the bounded output-pipe drainers, and the
//! output-size cap. The happy path is covered end-to-end in
//! `tests/eval_script.rs`; this file drives the adapter directly with a real
//! interpreter and a SHORT timeout so the suite stays fast.
//!
//! Each test builds a minimal `ScriptSpec` (a temp `cwd`, a one-entry `PATH`
//! env, empty stdin) and asserts the observable `ExitDisposition` plus a
//! wall-clock bound proving `run()` does not hang. The interpreter is resolved
//! from `AILLY_PYTHON` (default `python3`) for CI parity, matching
//! `resolve_runtime_binary` in the executor.

use std::time::Duration;
use std::time::Instant;

use ailly_two::knowledge::script_runner::ExitDisposition;
use ailly_two::knowledge::script_runner::SCRIPT_OUTPUT_CAP_BYTES;
use ailly_two::knowledge::script_runner::ScriptRunner;
use ailly_two::knowledge::script_runner::ScriptSpec;
use ailly_two::knowledge::script_runner::TokioScriptRunner;

/// The interpreter under test. `AILLY_PYTHON` overrides `python3` for CI
/// parity with the executor's `resolve_runtime_binary`.
fn python() -> String {
    std::env::var("AILLY_PYTHON").unwrap_or_else(|_| String::from("python3"))
}

/// Build a minimal spec: run `python -c <code>` in the system temp dir with a
/// single-entry `PATH` env (so the interpreter still resolves), the given
/// stdin, and the given wall-clock timeout.
fn python_spec(code: &str, stdin: Vec<u8>, timeout: Duration) -> ScriptSpec {
    let path = std::env::var("PATH").unwrap_or_default();
    ScriptSpec {
        program: python(),
        args: vec![String::from("-c"), code.to_owned()],
        cwd: std::env::temp_dir(),
        env: vec![(String::from("PATH"), path)],
        stdin,
        timeout,
    }
}

#[tokio::test]
async fn run_returns_timed_out_and_completes_before_the_child_would_have() {
    // A checker that sleeps far longer than its timeout must be killed and
    // reaped, and `run()` must return well under the sleep duration.
    let runner = TokioScriptRunner;
    let spec = python_spec(
        "import time; time.sleep(5)",
        Vec::new(),
        Duration::from_millis(300),
    );

    let start = Instant::now();
    let output = runner.run(spec).await.expect("run completes on timeout");
    let elapsed = start.elapsed();

    assert!(
        matches!(output.exit, ExitDisposition::TimedOut),
        "a checker that outlives its timeout must classify as TimedOut, got {:?}",
        std::mem::discriminant(&output.exit),
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "run() must return well before the 5s sleep, took {elapsed:?}",
    );
}

#[tokio::test]
async fn run_returns_when_a_grandchild_holds_the_output_pipe_open() {
    // Regression for the drainer hang: the python parent spawns a grandchild
    // that inherits stdout and sleeps 30s, prints, then exits 0. `read_to_end`
    // on the stdout pipe never sees EOF (the grandchild holds the write end
    // open), so without the bounded-drain fix `run()` hangs forever. With the
    // fix it returns within roughly the drain-grace window.
    let runner = TokioScriptRunner;
    let code = "\
import subprocess, sys
subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])
print('parent done')
sys.exit(0)
";
    let spec = python_spec(code, Vec::new(), Duration::from_secs(10));

    let start = Instant::now();
    let output = runner
        .run(spec)
        .await
        .expect("run completes despite the grandchild holding the pipe");
    let elapsed = start.elapsed();

    assert!(
        matches!(output.exit, ExitDisposition::Exited(0)),
        "the parent exits 0 even though a grandchild holds the pipe, got {:?}",
        std::mem::discriminant(&output.exit),
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "run() must not block on the 30s grandchild, took {elapsed:?}",
    );
}

#[tokio::test]
async fn run_caps_captured_stdout_and_does_not_hang() {
    // A checker that writes ~5 MiB to stdout must have its capture bounded by
    // SCRIPT_OUTPUT_CAP_BYTES, and `run()` must terminate rather than buffer
    // unbounded.
    let runner = TokioScriptRunner;
    let code = "\
import sys
chunk = b'x' * (1024 * 1024)
for _ in range(5):
    sys.stdout.buffer.write(chunk)
sys.stdout.buffer.flush()
sys.exit(0)
";
    let spec = python_spec(code, Vec::new(), Duration::from_secs(10));

    let start = Instant::now();
    let output = runner.run(spec).await.expect("run completes under the cap");
    let elapsed = start.elapsed();

    assert!(
        output.stdout.len() as u64 <= SCRIPT_OUTPUT_CAP_BYTES,
        "captured stdout must be capped at {SCRIPT_OUTPUT_CAP_BYTES} bytes, got {}",
        output.stdout.len(),
    );
    // The child runs to a normal (code-bearing) exit rather than being timed
    // out or signalled; the cap closes the read end early, so a flooder past
    // the cap exits non-zero on a broken pipe — that is the cap working, so the
    // exact code is not pinned.
    assert!(
        matches!(output.exit, ExitDisposition::Exited(_)),
        "a flooding checker must reach a normal exit, got {:?}",
        std::mem::discriminant(&output.exit),
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "run() must not hang draining a flooded pipe, took {elapsed:?}",
    );
}

#[tokio::test]
async fn run_classifies_a_signal_killed_child_as_signaled() {
    // A checker that kills itself with SIGKILL has no exit code, so the runner
    // must classify it as Signaled rather than Exited.
    let runner = TokioScriptRunner;
    let spec = python_spec(
        "import os, signal; os.kill(os.getpid(), signal.SIGKILL)",
        Vec::new(),
        Duration::from_secs(10),
    );

    let output = runner.run(spec).await.expect("run completes on a signal");

    assert!(
        matches!(output.exit, ExitDisposition::Signaled),
        "a SIGKILL'd child must classify as Signaled, got {:?}",
        std::mem::discriminant(&output.exit),
    );
}
