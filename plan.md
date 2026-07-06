# Implementation Plan: CLI Case Filtering for `assemble`/`run`/`eval`

**Feature test:** `tests/cli_case_filter.rs`, `case_filter_scopes_assemble_run_and_eval_to_selected_cases`
**User story:** A developer who edited one skill in a multi-skill suite can scope `assemble`/`run`/`eval` to just that skill's case(s) with a repeatable `--case <name>` flag, instead of paying for the whole matrix on every edit-test cycle.

**Libraries & Skills — load before every build step:** `ailly_two/skills/ailly-skill-eval/SKILL.md` (read directly; not a registered plugin skill) for project-anatomy/assertion-palette context, and treat `ailly_two/DESIGN.md` as the authoritative schema reference rather than restating it.

**Open decisions this plan fixes (per design.md's "Open Artifact Decisions"):**
- Error variant: `UnknownCase { requested: Vec<String>, available: Vec<String> }` added to `AssembleError`, `RunCmdError`, `EvalCmdError` (one per command's existing enum, matching the design's suggested name).
- Message wording: `"--case {requested:?} matched nothing; available cases: {available:?}"` (both the requested and available lists appear, per the design's only hard constraint on wording).

**Steps:**
- [ ] Step 0: API surface area — `cases` field, error variants, shared predicate signatures, mechanical call-site fixes
- [ ] Step 1: Filter `assemble`'s matrix expansion
- [ ] Step 2: Filter `run`'s resolved conversation keys
- [ ] Step 3: Filter `eval`'s conversation list and suite case list together
- [ ] Step 4: Unknown-case hard error across all three commands
- [ ] Step 5: Wire `--case` through the CLI (`main.rs`)
- [ ] Step 6: `DESIGN.md` doc fix — replace the stale `--var` paragraph with `--case`

## Step 0: API surface area

New field, new error variants, and the shared predicate — signatures only, no bodies. Also folds in the mechanical, behavior-free fix the design flags separately: adding `cases` to a struct with existing literal call sites elsewhere in the tree (`src/cli/assemble.rs` unit tests, `tests/run.rs`, `tests/assemble.rs`, `tests/project_layout.rs`, `tests/eval_report_path_roundtrip.rs`, `tests/assemble_external_prefix.rs`, `tests/eval_cmd.rs`, `tests/e2e_patterns_eval.rs`, `tests/e2e_delegate_52.rs`, `tests/eval_insurance_claim.rs`, `tests/skill_forge_clean_comments_review.rs`, `src/main.rs`) breaks compilation unless each is updated. `#[derive(Default)]` on the three `Args` structs plus `..Default::default()` appended to every existing literal (mechanical, no logic change) keeps this step's diff purely structural. `main.rs`'s three call sites get `cases: vec![]` for now (Step 5 replaces the placeholder with real CLI wiring).

```rust
// src/cli/assemble.rs
#[derive(Clone, Debug, Default)]
pub struct AssembleArgs {
    pub project: PathBuf,
    pub name: String,
    pub cases: Vec<String>,
}

pub enum AssembleError {
    // ...existing variants unchanged...
    #[error("--case {requested:?} matched nothing; available cases: {available:?}")]
    UnknownCase { requested: Vec<String>, available: Vec<String> },
}

// src/cli/run.rs
#[derive(Clone, Debug, Default)]
pub struct RunArgs {
    pub project: PathBuf,
    pub target: PathBuf,
    pub cases: Vec<String>,
}

pub enum RunCmdError {
    // ...existing variants unchanged...
    #[error("--case {requested:?} matched nothing; available cases: {available:?}")]
    UnknownCase { requested: Vec<String>, available: Vec<String> },
}

// src/cli/eval.rs
#[derive(Clone, Debug, Default)]
pub struct EvalCmdArgs {
    pub project: PathBuf,
    pub suite: String,
    pub over: PathBuf,
    pub cases: Vec<String>,
}

pub enum EvalCmdError {
    // ...existing variants unchanged...
    #[error("--case {requested:?} matched nothing; available cases: {available:?}")]
    UnknownCase { requested: Vec<String>, available: Vec<String> },
}

// src/cli/mod.rs — shared filter concept, one predicate reused by all three handlers.
/// `true` when `cases` is empty (no filter) or contains `name` exactly.
pub(crate) fn case_filter_matches(name: &str, cases: &[String]) -> bool;

/// Requested values in `cases` that do not appear in `available`, in the
/// order they were requested. Empty means every requested value matched.
pub(crate) fn unmatched_cases(cases: &[String], available: &[&str]) -> Vec<String>;
```

## Step 1: Filter `assemble`'s matrix expansion

**Enables:** the feature test's Act 1 assertion (`assemble --case newtype writes exactly one conversation file`) and Act 2's regression guard (`omitting --case is unchanged: every matrix binding is still written`).

`run_with_project`'s loop over `assembly.expand_matrix()` computes each binding's would-be case name the same way `ConversationKey::from_binding` does today (`filename_for(binding)` stripped of `.yaml`) and skips staging any binding whose name doesn't pass `case_filter_matches(name, &args.cases)`. An empty `cases` list must stage every binding exactly as today (no observable change), since this is the flag's regression-safety guarantee.

**Tests**

```
test "filtered assemble writes only the requested binding's file":
  project <- write_project(3-skill matrix assembly)
  run_dir <- assemble::run(AssembleArgs { cases: ["newtype"], .. })
  assert yaml_files(run_dir) == ["newtype.yaml"]

test "empty cases is unchanged from today":
  run_dir <- assemble::run(AssembleArgs { cases: [], .. })
  assert yaml_files(run_dir) == ["logging.yaml", "newtype.yaml", "tracing.yaml"]
```

- Edge case: `--case` naming more than one binding (repeatable flag) stages all of them.
- Edge case: a two-axis matrix's binding name (dash-joined) still matches on the full joined name, not per-axis.

**Implementation Outline**

```
fn run_with_project(project, assembly_name, cases):
  for binding in assembly.expand_matrix():
    name <- derive binding's case name (filename_for(binding), strip ".yaml")
    if not case_filter_matches(name, cases): continue
    render + stage binding as today
  commit as today
```

## Step 2: Filter `run`'s resolved conversation keys

**Enables:** the feature test's Act 3 assertions (`run --case logging --case tracing processes exactly the two named cases`, `logging.yaml`/`tracing.yaml` filled, `newtype.yaml` left untouched) and Act 4 (filling the remaining case by name).

`resolve_keys` lists `ConversationKey`s for a directory target as today, then drops any key whose `name` doesn't pass `case_filter_matches`. A file target is filtered the same way: the single resolved key either passes (unchanged behavior) or — per the design — becomes the "matched nothing" case handled in Step 4, so this step's directory path and file path share one filter call rather than branching.

**Tests**

```
test "run --case filters a directory target to the named keys":
  dir <- assemble 3 conversations (logging, newtype, tracing), each with a blank assistant turn
  outcome <- run::run(RunArgs { target: dir, cases: ["logging", "tracing"], .. })
  assert outcome.conversations_processed == 2
  assert has_blank_assistant(dir, "newtype.yaml")  // untouched
  assert !has_blank_assistant(dir, "logging.yaml")
  assert !has_blank_assistant(dir, "tracing.yaml")
```

- Edge case: empty `cases` on a directory target processes every key, as today.
- Edge case: single-file target with a passing filter behaves exactly as an unfiltered file target today.

**Implementation Outline**

```
fn resolve_keys(project, repo, target, cases):
  keys <- (existing file/dir resolution logic, unchanged)
  if keys came from a directory: keys <- keys.filter(|k| case_filter_matches(k.name, cases))
  // file-target-plus-filter-miss deferred to Step 4's UnknownCase check
  return keys
```

## Step 3: Filter `eval`'s conversation list and suite case list together

**Enables:** the feature test's Act 5 assertions in full (`conversations_matched == 1`, `assertions_passed == 1`, `assertions_failed == 0`, `assertions_malformed == 0`, `report_path.exists()`). This is the step the design's "Fixed during review" note is about: filtering only the conversation list would make `logging`/`tracing`'s named suite cases synthesize a `Malformed` "no conversation found" outcome, so both lists must be filtered before `evaluate()` runs.

After listing conversation keys under `--over`, drop keys whose `name` doesn't pass `case_filter_matches`, exactly as Step 2's directory path does — before loading conversations. Separately, filter the loaded suite's `cases: Vec<Case>`: drop any case whose `name` is `Some(n)` and `!case_filter_matches(n, cases)`; leave `name: None` cases untouched (they have no per-name identity to filter on, and `evaluate()`'s malformed synthesis is already guarded by `case.name.is_some()`).

**Tests**

```
test "eval --case filters both conversations and named suite cases":
  run_dir <- 3 fully-run conversations (newtype, logging, tracing), each replying "noop-N"
  suite <- 3 cases named newtype/logging/tracing, each asserting text_contains "noop"
  outcome <- eval::run(EvalCmdArgs { suite, over: run_dir, cases: ["newtype"], .. })
  assert outcome.conversations_matched == 1
  assert outcome.assertions_passed == 1
  assert outcome.assertions_malformed == 0  // logging/tracing cases dropped, not malformed
```

- Edge case: a suite case with no `name:` (a `when:`-only or fully open case) is never dropped by `--case`, regardless of filter contents.
- Edge case: empty `cases` leaves both lists untouched — unchanged behavior.

**Implementation Outline**

```
fn eval::run(args):
  suite <- load suite as today
  keys <- list conversations under `over` as today
  keys <- keys.filter(|k| case_filter_matches(k.name, args.cases))
  filtered_suite_cases <- suite.cases.filter(|c| c.name.is_none() || case_filter_matches(c.name, args.cases))
  conversations <- load each filtered key, as today
  report <- evaluate(EvalArgs { suite: filtered_suite_cases, conversations, .. })
  write report + build outcome, as today
```

## Step 4: Unknown-case hard error across all three commands

**Enables:** no assertion in the recorded feature test (it only exercises valid names) — this implements the design's separately-stated "Failure mode" contract (a typo'd `--case` value is a hard error naming the miss and the available names, even when mixed with a real name) so the feature is complete per spec, not just per the happy-path test. Kept as its own step so Steps 1–3 stay focused on the assertions that unblock the feature test.

After computing the filtered set in each of `assemble`/`run`/`eval`, compute `unmatched_cases(&args.cases, &available_names)` where `available_names` is every case name that existed in the target before filtering (matrix bindings for `assemble`; listed keys for `run`; listed keys for `eval` — not the suite's case names, since `--case` selects conversations, and a suite case with no matching conversation already has its own "no conversation" signal handled in Step 3). If non-empty, return `UnknownCase { requested: unmatched, available: available_names }` instead of proceeding.

**Tests**

```
test "one real name and one typo still errors on the typo":
  err <- assemble::run(AssembleArgs { cases: ["newtype", "nwetype"], .. })
  assert err is UnknownCase { requested: ["nwetype"], available: ["logging", "newtype", "tracing"] }
```

- Edge case: every requested value misses (not just one) — same variant, `requested` lists all of them.
- Edge case: `run` against a single-file target whose name doesn't pass the filter — also an `UnknownCase`, not a silent no-op, per Step 2's deferred note.

**Implementation Outline**

```
fn <command>::run(args):
  available <- names present in the target before filtering
  missing <- unmatched_cases(&args.cases, &available)
  if not missing.is_empty(): return Err(UnknownCase { requested: missing, available })
  // proceed with the Step 1/2/3 filtered path
```

## Step 5: Wire `--case` through the CLI (`main.rs`)

**Enables:** no assertion in the feature test (it constructs `Args` by hand) — this is the design's actual user-facing entry point (`ailly assemble invocation --case newtype`) and the last piece needed for the journey described in design.md to work from a terminal, replacing Step 0's `cases: vec![]` placeholders.

Add `#[arg(long = "case")] cases: Vec<String>` to `Command::Assemble`, `Command::Run`, and `Command::Eval` in `main.rs` (clap's `Vec<String>` field with a repeatable `long` arg already gives `--case a --case b` for free), and pass `cases` through to each `Args` construction in place of the Step 0 placeholder.

**Tests**

```
test "clap parses repeated --case flags into a Vec":
  cli <- Cli::parse_from(["ailly", "assemble", "invocation", "--case", "a", "--case", "b"])
  assert cli.command is Assemble { cases: ["a", "b"], .. }
```

- Edge case: no `--case` at all parses to an empty `Vec` (clap default for `Vec<T>` args).
- Edge case: `--case` combined with existing flags (e.g. `eval --over <dir> --case x`) still parses `over` correctly — order independence.

**Implementation Outline**

```
enum Command:
  Assemble { name: String, #[arg(long = "case")] cases: Vec<String> }
  Run { target: PathBuf, #[arg(long = "case")] cases: Vec<String> }
  Eval { suite: String, #[arg(long)] over: PathBuf, #[arg(long = "case")] cases: Vec<String> }

match cli.command:
  Assemble { name, cases } => assemble_run(AssembleArgs { project, name, cases })
  Run { target, cases } => run_cmd(RunArgs { project, target, cases })
  Eval { suite, over, cases } => eval_run(EvalCmdArgs { project, suite, over, cases })
```

## Step 6: `DESIGN.md` doc fix — replace the stale `--var` paragraph with `--case`

**Enables:** no assertion in the feature test — an in-scope doc fix the design names explicitly (`ailly_two/DESIGN.md`, "assembly" section, the paragraph documenting an unimplemented `--var <axis>=<value>` flag with no CLI flag, no code path, no test referencing it anywhere in `src/`).

Replace that paragraph with a short description of `--case`'s actual, now-implemented behavior: repeatable, exact-string match against the case/conversation name, shared across `assemble`/`run`/`eval`, empty-by-default (no filter), hard error on any unmatched value. Since `--case` is one mechanism documented once and used identically by all three commands, the correction belongs near the `assembly` section only if it stays assembly-scoped, or — better — as a short cross-cutting note near the top of the schema doc (above the per-schema sections) so it isn't misread as `assemble`-only. Plan defers the exact placement to the build step; content is fixed (describe `--case`, not `--var`).

**Tests**

This step has no unit test (a documentation edit); the acceptance check is a manual read: no reference to `--var` remains anywhere in `DESIGN.md`, and the replacement paragraph accurately describes the behavior Steps 1–4 implement.

- Edge case: grep `DESIGN.md` for `--var` after the edit — must return nothing.

**Implementation Outline**

```
DESIGN.md: delete the "--var <axis>=<value>" paragraph (assembly section);
add a "--case" paragraph describing repeatable exact-match filtering shared
by assemble/run/eval, empty-list-means-unfiltered, and the unmatched-value
hard error.
```

## Resolved by the long-loop reviewer (2026-07-06)

**1. Error variant names and "matched nothing" message wording. Decided:
`AssembleError::UnknownCase`, `RunCmdError::UnknownCase`,
`EvalCmdError::UnknownCase`, each `{ requested: Vec<String>, available:
Vec<String> }`, with message
`"--case {requested:?} matched nothing; available cases: {available:?}"`.**
This is the plan's own draft proposal, already conservative: it reuses the
exact variant name the design itself suggested (`AssembleError::UnknownCase`
etc.) rather than inventing a new one, follows this codebase's existing
`thiserror` struct-variant convention (see `AssembleError::Assembling`,
`RunCmdError::TargetNotFound`), and the wording satisfies the design's only
hard constraint — both the requested and available lists appear in the
message. No research turned up a competing convention elsewhere in the repo
for a "value not found among available values" error, so the design's own
proposal stands as the conservative default. Per the task's extra notes,
this was explicitly left to the builder to pick and move on.

**2. Placement of the `DESIGN.md` `--case` replacement paragraph. Decided:
replace the stale `--var` paragraph in place, in the `assembly` section
(`DESIGN.md` ~line 70), rather than relocating to a new cross-cutting note
near the top of the doc.** The plan's Step 6 explicitly deferred exact
placement to the build step while fixing the content. Editing in place is
the smaller, lower-blast-radius diff (one paragraph swapped, not a new
section plus a deletion) and keeps the correction anchored to the exact
spot a reader following the `--var` reference would already be looking; the
paragraph itself can note that `--case` is shared by `assemble`/`run`/`eval`
without needing a separate top-of-file section. Reversible and cosmetic
either way, so the smaller diff is the conservative choice.

No items required escalation: both open items were explicitly delegated to
the builder by the task's extra notes, the plan's own proposed resolutions
already match this repo's conventions, and nothing in the plan is
irreversible, out of the design's recorded scope, or underdetermined by the
repo's existing conventions.
