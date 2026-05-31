//! Subprocess runtime for `script` / `program` assertions. One async port
//! (`ScriptRunner`) so `check_script` is unit-testable against a fake, and one
//! Tokio production adapter. Mirrors the `EngineProvider` / `NoopEngine` split
//! in `engine.rs`.

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Default per-assertion subprocess wall-clock bound. A compile-time constant
/// (no zero-value footgun); a per-assertion override is a deferred item.
pub const SCRIPT_TIMEOUT_DEFAULT: Duration = Duration::from_secs(30);

/// Upper bound on bytes captured from each of the child's stdout / stderr
/// pipes (1 MiB). Two jobs: it bounds the runner's memory against a checker
/// that floods a pipe, and — because the drainer reads through
/// [`tokio::io::AsyncReadExt::take`] — it lets `read_to_end` terminate once the
/// cap is reached even if the pipe's write end never closes. Public so an
/// integration test can assert the captured length against it.
pub const SCRIPT_OUTPUT_CAP_BYTES: u64 = 1 << 20;

/// A checker subprocess to run. The complete spawn description: the executor
/// builds it, the runner only executes it. `env` is the COMPLETE child
/// environment — the runner calls `env_clear()` then sets exactly these pairs,
/// so environment-borne provider credentials and the project `.env` are never
/// inherited. The clear bounds what reaches the child *through the
/// environment*; it does not sandbox the filesystem, so a checker can still
/// read filesystem-rooted credentials such as `~/.aws/credentials`.
pub struct ScriptSpec {
    /// `"python3"`, `"node"`, or a `Program`-mode binary.
    pub program: String,
    /// `[resolved-script-path]` for `Script`; `[]` for `Program`.
    pub args: Vec<String>,
    /// Working directory; the project root.
    pub cwd: PathBuf,
    /// The whole child env (see `check_script` step 5).
    pub env: Vec<(String, String)>,
    /// Candidate response bytes only; the user question rides
    /// `AILLY_USER_QUESTION` out of band.
    pub stdin: Vec<u8>,
    /// Wall-clock bound; the child is killed and reaped on expiry.
    pub timeout: Duration,
}

/// Captured result of one subprocess. `stdout` / `stderr` are raw bytes so a
/// non-UTF-8 payload never demotes a verdict; the executor lowers them with
/// `from_utf8_lossy` at classification time.
pub struct ScriptOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit: ExitDisposition,
}

/// How the child finished. `Signaled` and `TimedOut` are environmental, not
/// data-driven, so the executor routes both to `Errored`.
pub enum ExitDisposition {
    Exited(i32),
    Signaled,
    TimedOut,
}

/// Structural failure of the runner itself (spawn / pipe), distinct from a
/// checker that ran and rejected the candidate. Every variant maps to
/// `Errored` at the executor, never `Fail`.
#[derive(thiserror::Error, Debug)]
pub enum ScriptError {
    #[error("failed to spawn {program:?}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write stdin: {0}")]
    Stdin(#[source] std::io::Error),
    #[error("failed to read pipe: {0}")]
    Pipe(#[source] std::io::Error),
}

/// Run a single checker subprocess to completion (or timeout). The sole seam
/// between the executor and the OS so `check_script` can be unit-tested against
/// a hand-rolled fake, mirroring the `NoopEngine` precedent.
#[async_trait]
pub trait ScriptRunner: Send + Sync {
    /// Run `spec` to completion, killing and reaping the child on timeout.
    ///
    /// On the wall-clock timeout the child is killed and reaped and the three
    /// I/O tasks are aborted, returning empty output with
    /// [`ExitDisposition::TimedOut`] — a grandchild that inherited stdout could
    /// otherwise hold the pipe open and hang the drain forever. On the
    /// normal-exit branch the output drains are bounded by a grace window for
    /// the same reason, and a captured stream is capped at
    /// [`SCRIPT_OUTPUT_CAP_BYTES`].
    ///
    /// # Errors
    ///
    /// Returns [`ScriptError`] when the child cannot be spawned, stdin cannot
    /// be written, or an output pipe cannot be read. A child that runs and
    /// exits non-zero is *not* an error — that is an [`ExitDisposition`] the
    /// executor classifies. A stdin write error surfaces only on the
    /// normal-exit branch; the timeout branch never lets it mask the
    /// disposition.
    async fn run(&self, spec: ScriptSpec) -> Result<ScriptOutput, ScriptError>;
}

/// Production adapter over `tokio::process::Command`. The sole real impl; unit
/// tests of `check_script` use a hand-rolled `FakeScriptRunner` instead,
/// mirroring `NoopEngine`. This adapter's own correctness (kill-on-drop, pipe
/// handling, timeout race) is proven by the integration suite in
/// `tests/eval_script.rs`, which spawns a real `python3` — a subprocess adapter
/// has no seam below it to mock.
pub struct TokioScriptRunner;

#[async_trait]
impl ScriptRunner for TokioScriptRunner {
    async fn run(&self, spec: ScriptSpec) -> Result<ScriptOutput, ScriptError> {
        let mut child = Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .env_clear()
            .envs(spec.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| ScriptError::Spawn {
                program: spec.program.clone(),
                source,
            })?;

        // Take every pipe handle so `child.wait()` cannot deadlock on an
        // unread stdin, and so the drainers own the read ends.
        let mut stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        // Stream stdin in its own task and drop the handle to close the pipe.
        // A checker that ignores stdin and exits closes the read end first; a
        // resulting `BrokenPipe` is the child's choice, not a runner failure.
        let stdin_bytes = spec.stdin;
        let stdin_task = tokio::spawn(async move {
            let result = match stdin.write_all(&stdin_bytes).await {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
                Err(err) => Err(err),
            };
            drop(stdin);
            result
        });
        // Drain both output pipes concurrently so a child that fills stdout
        // before reading stdin cannot deadlock against the stdin writer. Each
        // read is bounded by `SCRIPT_OUTPUT_CAP_BYTES` via `take`, so a flood
        // cannot exhaust memory and `read_to_end` terminates at the cap even if
        // the write end never closes.
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            stdout
                .take(SCRIPT_OUTPUT_CAP_BYTES)
                .read_to_end(&mut buf)
                .await
                .map(|_| buf)
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            stderr
                .take(SCRIPT_OUTPUT_CAP_BYTES)
                .read_to_end(&mut buf)
                .await
                .map(|_| buf)
        });

        // Race the child against the timeout. `child.wait()` borrows `child`
        // only for the duration of the select; the kill+reap below runs once
        // that borrow is released.
        let wait_result;
        let timed_out;
        tokio::select! {
            status = child.wait() => {
                wait_result = Some(status);
                timed_out = false;
            }
            () = tokio::time::sleep(spec.timeout) => {
                wait_result = None;
                timed_out = true;
            }
        }

        if timed_out {
            // Kill and reap the child, then abort the three I/O tasks rather
            // than awaiting them: a grandchild that inherited stdout keeps the
            // pipe's write end open, so `read_to_end` would never reach EOF and
            // `run()` would hang forever despite the wall-clock bound. The
            // output is discarded because `classify_exit` ignores stdout/stderr
            // on `TimedOut`, and abort cannot mask the disposition the way an
            // awaited stdin write error would.
            let _ = child.kill().await;
            let _ = child.wait().await;
            stdin_task.abort();
            stdout_task.abort();
            stderr_task.abort();
            return Ok(ScriptOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit: ExitDisposition::TimedOut,
            });
        }

        let status = wait_result
            .expect("wait_result is Some on the non-timeout branch")
            .map_err(ScriptError::Pipe)?;
        let exit = exit_disposition(status);

        // The child exited on its own. This is the only branch that surfaces a
        // genuine stdin write error; a grandchild may still hold a pipe open,
        // so the output drains are bounded by `DRAIN_GRACE`.
        match stdin_task.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(ScriptError::Stdin(err)),
            Err(join) => return Err(ScriptError::Stdin(io::Error::other(join))),
        }
        let stdout = join_pipe_bounded(stdout_task, DRAIN_GRACE).await?;
        let stderr = join_pipe_bounded(stderr_task, DRAIN_GRACE).await?;

        Ok(ScriptOutput {
            stdout,
            stderr,
            exit,
        })
    }
}

/// Map a finished child's status to an [`ExitDisposition`]. A `None` exit code
/// on Unix means the child was terminated by a signal.
fn exit_disposition(status: std::process::ExitStatus) -> ExitDisposition {
    match status.code() {
        Some(code) => ExitDisposition::Exited(code),
        None => ExitDisposition::Signaled,
    }
}

/// Grace window for draining the output pipes after the child exits on its own.
/// A grandchild that inherited stdout can hold the pipe's write end open past
/// the parent's exit, so the drain is bounded here rather than awaited
/// unconditionally — without the bound `read_to_end` would never reach EOF and
/// `run()` would hang. The cap is short because the parent has already exited;
/// any bytes still arriving are a leaked grandchild's, not the checker's.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Await a drained output pipe within `grace`, lowering both a task panic and a
/// read error to [`ScriptError::Pipe`]. If `grace` elapses first (a grandchild
/// is still holding the write end open), abort the drainer and return whatever
/// is safe — an empty buffer — so the caller never blocks on a leaked pipe.
async fn join_pipe_bounded(
    mut handle: tokio::task::JoinHandle<io::Result<Vec<u8>>>,
    grace: Duration,
) -> Result<Vec<u8>, ScriptError> {
    match tokio::time::timeout(grace, &mut handle).await {
        Ok(Ok(Ok(buf))) => Ok(buf),
        Ok(Ok(Err(err))) => Err(ScriptError::Pipe(err)),
        Ok(Err(join)) => Err(ScriptError::Pipe(io::Error::other(join))),
        Err(_elapsed) => {
            handle.abort();
            Ok(Vec::new())
        }
    }
}
