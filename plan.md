# Plan: Judge Calibration Harness

**Feature test:** `tests/judge_calibration.rs` (currently RED — confirmed by
`unresolved import ailly_two::knowledge::calibration`; see the design doc's
"Feature test" section for the full fixture and assertions).

**User story:** A developer who is not sure whether to trust a `judge`
assertion's verdicts runs the calibration harness over an `EvalReport` and a
human-labeled set and gets back an exact agreement rate plus a clear
pass/fail signal on the industry-cited ≥90% bar.

**Design reference:** `.ailly/developer/2026-07-06-A-ailly-evals/feature-e-judge-calibration/design.md`
(session copy in `domain-driven-design`), including its "Resolved by the
long-loop reviewer (2026-07-06)" block, which fixes decisions (1) report
format, (2) `labels.yaml` shape, (3) deferring the live-verification
mechanism, and (4) a typed `CalibrationError` for malformed suite shapes.

## Scope for this build

In scope: `src/knowledge/calibration.rs`'s pure `compute_calibration`
function and its supporting types, made correct against
`tests/judge_calibration.rs`, plus a typed `CalibrationError` for malformed
suite shapes (design decision 4) with its own unit test(s).

Explicitly **not** in scope for this build (per the long-loop's instruction,
recorded in design.md as decisions 1, 2, 3, 5):

- No `evals/calibration/<run-id>.json` / `.md` persistence or CLI wrapper
  (decision 1 — file-format shape is recorded in design.md for a later plan
  step; no `render_calibration_markdown` function is added in this build).
- No `e2e/judge-calibration/evals/labels.yaml` file or loader (decision 2 —
  shape is recorded; `compute_calibration` already takes an in-memory
  `BTreeMap<String, HumanVerdict>`, so no file-parsing code is needed to pass
  the feature test).
- No live-verification mechanism — no `#[ignore]`d credential-gated
  integration test, no `ci.sh` step (decision 3 — deferred to a later plan
  step once labeled examples exist).
- No curation of `e2e/judge-calibration/`'s real 20-50 human-labeled
  examples (decision 5 — explicitly deferred to a human; not a file this
  build touches).

## Steps checklist

- [ ] Step 0 — API surface stubs (`calibration.rs` types + signatures, no bodies)
- [ ] Step 1 — Reject malformed case shapes with a typed `CalibrationError`
- [ ] Step 2 — Look up each example's human label; typed error on omission
- [ ] Step 3 — Exclude errored/deferred outcomes from numerator and denominator
- [ ] Step 4 — Map judge outcome × human verdict into Agree/Disagree
- [ ] Step 5 — Compute `agreement_rate`, `meets_bar`, and assemble the final `CalibrationReport`

---

## Step 0 — API surface stubs only

No behavior. Establishes the module and its public shape so downstream steps
have a fixed contract to implement against and so the feature test's
`use` statements resolve (compilation still fails on `todo!()`/`unimplemented!()`
panics if actually run, which is expected and fine — Step 0 does not aim for
a passing test, only a compiling one).

- Add `pub mod calibration;` to `src/knowledge/mod.rs`, alongside `assertions`,
  `eval`, `report`, `script_runner`.
- In new `src/knowledge/calibration.rs`, declare (signatures/derives only,
  every body `todo!()` or a bare `Default`-style zero value where a struct
  literal is unavoidable — no folding logic anywhere in this step):
  - `pub enum HumanVerdict { Pass, Fail }` — `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`
    (the `Serialize`/`Deserialize` derives are load-bearing, not decorative: `ExampleAgreement`
    below embeds a `HumanVerdict` field and itself derives `Serialize, Deserialize`, so without
    this the crate does not compile — caught reviewing Step 0 against `report.rs`'s derive
    pattern before build).
  - `pub enum Agreement { Agree, Disagree, Excluded }` — `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`
    (`PartialEq` because the feature test compares `Agreement` with `==`; `Serialize`/`Deserialize`
    for the same embedding reason as `HumanVerdict` above — `ExampleAgreement` carries an
    `agreement: Agreement` field).
  - `pub struct ExampleAgreement { pub id: String, pub human_verdict: HumanVerdict, pub judge_outcome: String, pub reason: Option<String>, pub agreement: Agreement }`
    — `Debug, Clone, Serialize, Deserialize` (mirrors `report.rs`'s
    `AssertionReport` shape and derive set).
  - `pub const JUDGE_CALIBRATION_BAR: f64 = 0.90;`
  - `pub struct CalibrationReport { pub suite: String, pub run_id: String, pub total_examples: usize, pub excluded: usize, pub considered: usize, pub agreements: usize, pub agreement_rate: f64, pub meets_bar: bool, pub examples: Vec<ExampleAgreement> }`
    — `Debug, Clone, Serialize, Deserialize` (mirrors `EvalReport`/`ComparisonReport`).
  - `pub enum CalibrationError` (`#[derive(Debug, thiserror::Error)]`, mirroring
    `EngineError`'s and `script_runner`'s error-enum convention) with variants:
    - `NoMatch { case: String }` — a case matched zero conversations.
    - `MultipleMatches { case: String, count: usize }` — a case matched more
      than one conversation (a calibration suite must be one case per
      example).
    - `MultipleAssertions { case: String, count: usize }` — a case carried
      more than one assertion (a calibration case must carry exactly one
      `judge` assertion).
    - `MissingLabel { case: String }` — a case name has no entry in the
      supplied `labels` map (discovered wiring this up in Step 2; not named
      in design.md's decision 4 text but the same typed-error convention
      applies — a silent skip here would silently exclude an example from
      both the numerator and denominator, the same false-confidence failure
      mode decision 4 already rejected for the three named shapes).
    - Each variant gets a `#[error("...")]` message naming the case and the
      specific defect, matching `EngineError`'s style of a human-readable
      message plus structured fields.
  - `pub fn compute_calibration(report: &crate::knowledge::eval::EvalReport, labels: &std::collections::BTreeMap<String, HumanVerdict>) -> Result<CalibrationReport, CalibrationError>`
    — signature only, body `todo!()`. Note this signature returns `Result`,
    not the bare `CalibrationReport` design.md's Specification originally
    sketched — the `Result` wrap is decision 4's resolution.

---

## Step 1 — Reject malformed case shapes with a typed `CalibrationError`

**Unblocks:** a new unit test (not in `tests/judge_calibration.rs`; added in
this step per design decision 4's instruction to "add a test for it") proving
`compute_calibration` returns `Err(CalibrationError::NoMatch { .. })` for a
case matching zero conversations, `Err(CalibrationError::MultipleMatches { .. })`
for a case matching two, and `Err(CalibrationError::MultipleAssertions { .. })`
for a case carrying two assertions.

**Happy-path test sketch:** hand-build a one-case `EvalReport` (or drive
`evaluate()` with a suite/conversation set shaped to match twice) where
`cases[0].matches.len() == 2`; call `compute_calibration` with any labels map;
assert the return is `Err(CalibrationError::MultipleMatches { case, count: 2 })`
with `case` equal to the suite case's name. Repeat for zero matches and for
two assertions on one match.

**Implementation outline:** iterate `report.cases` in order; for each case,
inspect `case.matches.len()` (`0` ⇒ `NoMatch`, `> 1` ⇒ `MultipleMatches`);
for a case with exactly one match, inspect that match's `assertions.len()`
(`!= 1` ⇒ `MultipleAssertions`); return the first violation found (`?`-style
short-circuit) before any counting/aggregation begins. Cases with exactly one
match and exactly one assertion fall through to Step 2.

---

## Step 2 — Look up each example's human label; typed error on omission

**Unblocks:** a new unit test proving a case name absent from the `labels`
map yields `Err(CalibrationError::MissingLabel { case })`; and begins making
progress on the feature test's `total_examples == 10` assertion for the
fully-covered happy path (all ten `example-NN` cases have a label).

**Happy-path test sketch:** the feature test's fixture minus the
`labels.insert("example-07", ...)` line; assert `compute_calibration` returns
`Err(CalibrationError::MissingLabel { case }) if case == "example-07"`.
Positive companion: the full ten-case, ten-label fixture reaches this step
without error and (once Step 5 lands) produces `total_examples == 10`.

**Implementation outline:** after Step 1's shape validation passes for a
case, resolve the case's `name` (a calibration case is expected to carry
one, per the suite shape in design.md's "Suite/labels shape" section) and
look it up in `labels`; missing ⇒ return `Err(CalibrationError::MissingLabel)`
before touching any counters. `total_examples` is `report.cases.len()` once
every case clears Steps 1 and 2 without error.

---

## Step 3 — Exclude errored/deferred outcomes from numerator and denominator

**Unblocks:** `calibration.excluded == 0` and `calibration.considered == 10`
in the feature test (proves exclusion accounting is correct even when the
fixture happens to exclude nothing), paired with a dedicated unit test that
gives one case an `"errored"` or `"deferred"` outcome and asserts `excluded`
increments while `considered` and `agreements` do not count that example.

**Happy-path test sketch:** a small report built from three cases whose
single assertion outcomes are `"pass"`, `"deferred"`, `"fail"`, all present
in `labels`; assert `excluded == 1`, `considered == 2`, and the `deferred`
example's `ExampleAgreement.agreement == Agreement::Excluded`.

**Implementation outline:** for each validated case (Steps 1-2 passed), read
`match.assertions[0].outcome` (the `String` `"pass"|"fail"|"deferred"|"malformed"|"errored"`
per `report.rs::outcome_label`); when it is `"errored"` or `"deferred"`, push
an `ExampleAgreement` with `agreement: Agreement::Excluded` and increment a
running `excluded` counter, without touching `agreements`; otherwise fall
through to Step 4's mapping. `considered = total_examples - excluded`.

---

## Step 4 — Map judge outcome × human verdict into Agree/Disagree

**Unblocks:** `calibration.agreements == 9`, the `example-07` lookup's
`agreement == Agreement::Disagree`, and the `agree_count == 9` filter — all
three in the feature test.

**Happy-path test sketch:** the feature test's own fixture is the primary
happy path here (nine agreeing pass/fail pairs, one deliberate
`"pass"` outcome against a `HumanVerdict::Fail` label at `example-07`).
A dedicated unit test additionally covers the `"malformed"` outcome, not
exercised by the feature test's fixture: `"malformed"` against either
`HumanVerdict::Pass` or `HumanVerdict::Fail` must resolve to
`Agreement::Disagree` (per design.md's "Outcome-to-agreement mapping" — a
hedge is never treated as agreement).

**Implementation outline:** for each case reaching this step (outcome is
`"pass"`, `"fail"`, or `"malformed"`), compare `(outcome, human_verdict)`:
`"pass"` agrees with `HumanVerdict::Pass` and disagrees with `Fail`; `"fail"`
agrees with `HumanVerdict::Fail` and disagrees with `Pass`; `"malformed"`
always disagrees. Push the resulting `ExampleAgreement` (carrying `reason`
from `AssertionReport.reason`) and increment a running `agreements` counter
on every `Agree`.

---

## Step 5 — Compute `agreement_rate`, `meets_bar`, and assemble the final `CalibrationReport`

**Unblocks:** the feature test's `agreement_rate` (exact `0.9`) and
`meets_bar == true` assertions — the last two assertions the feature test
makes that no prior step satisfies. A dedicated unit test at a rate just
under the bar (e.g. `8/9 ≈ 0.888…`) asserts `meets_bar == false`, to prove
the boundary is `>=` and not merely "close to 90%" by construction of the
one fixture the feature test happens to use.

**Happy-path test sketch:** given `considered == 10`, `agreements == 9` from
Steps 3-4, assert `agreement_rate == 0.9` exactly and `meets_bar == true`.
Companion: `considered == 9`, `agreements == 8` ⇒ `meets_bar == false`.

**Implementation outline:** `agreement_rate = if considered == 0 { 0.0 } else { agreements as f64 / considered as f64 }`;
`meets_bar = considered > 0 && agreement_rate >= JUDGE_CALIBRATION_BAR`
(inclusive, per design.md's "Bar semantics"). Assemble and return
`Ok(CalibrationReport { suite: report.suite.clone(), run_id: report.run_id.clone(), total_examples, excluded, considered, agreements, agreement_rate, meets_bar, examples })`,
where `examples` is the `Vec<ExampleAgreement>` accumulated across Steps 3-4
in case order.

---

## Deferred (not this build)

- **Live verification mechanism** (design decision 3) — no `#[ignore]`d
  integration test, no `ci.sh` step. Revisit once `e2e/judge-calibration/`'s
  labeled examples exist.
- **`e2e/judge-calibration/`'s real 20-50 human-labeled examples** (design
  decision 5) — sourcing (existing suite transcripts vs. purpose-written edge
  cases vs. both) is real human-judgment work explicitly left for a human,
  not this build. No files under `e2e/judge-calibration/` are created here.
- **`evals/calibration/<run-id>.json` + `.md` persistence and any CLI
  wrapper** (design decision 1) — format is recorded in design.md; no
  `cli`-layer code or `render_calibration_markdown` function is added in
  this build.
- **`labels.yaml` file format loader** (design decision 2) — shape is
  recorded in design.md; `compute_calibration` takes its labels as an
  in-memory `BTreeMap` already, so no YAML-parsing code is needed to satisfy
  the feature test.

---

## Resolved by the long-loop reviewer (2026-07-06)

**1. Design decisions 1-4's Open Artifact Decisions. Decided: already resolved
in `design.md`'s Summary (verified, not re-litigated).** Read `design.md`
directly: all four items already carry a `Resolved by the long-loop reviewer
(2026-07-06)` block in the exact format this shape's own convention requires
(one entry per item, `**N. <title>. Decided: ...**` followed by rationale).
No open decision remains in `design.md`'s Summary; decision 5 (curating
`e2e/judge-calibration/`'s real labeled examples) is correctly left open,
explicitly deferred to a human, and is not touched by this plan or build.

**2. `HumanVerdict` and `Agreement`'s Step 0 derive lists were missing
`Serialize`/`Deserialize`. Decided: add both derives to each enum.** Caught
reviewing Step 0 against the actual crate: `ExampleAgreement` embeds a
`human_verdict: HumanVerdict` field and an `agreement: Agreement` field and
itself derives `Serialize, Deserialize` (matching `report.rs`'s
`AssertionReport` convention this plan explicitly cites) — with the enums
undecorated, the crate does not compile. Fixed directly in Step 0 above
rather than filing this as a build-time surprise, since Step 0's whole
purpose is a coherent, compiling API surface before any test is written.

**3. The feature test (`tests/judge_calibration.rs`, committed at `d08a77c`)
calls `compute_calibration(&report, &labels)` and immediately reads
`calibration.total_examples` with no `.expect`/`?` — i.e. it was authored
against the design's original bare-`CalibrationReport`-return sketch, before
decision 4 (also recorded in `design.md`, this same review date) changed the
signature to `Result<CalibrationReport, CalibrationError>`. Decided: keep the
`Result`-returning signature (decision 4's typed-error resolution, matching
`EngineError`/`ScriptError`'s established library-boundary convention, stands
on its own well-argued merits) and fix the two call sites in the feature test
to `.expect("judge-calibration fixture is well-formed and fully labeled")`
instead.** This is the conservative default, not an escalation: the
feature test's fixture is fully well-formed (ten named cases, one match and
one assertion each, all ten labeled) and only ever hits the `Ok` path
regardless of which shape wins, so the fix is syntax-only, does not change
what the test asserts or verifies, and preserves the crate-convention-backed
typed error decision 4 already made for a well-documented reason (a silent
skip or panic on a malformed suite is rejected there for the same
false-confidence failure mode this whole feature-step exists to guard
against). Reverting decision 4 instead — dropping back to a bare-return
signature — would either force `compute_calibration` to panic on a malformed
suite shape (the exact failure mode decision 4 rejects) or silently abandon
the just-recorded decision without a comparably strong reason. This is a
same-session authoring-order artifact (the test was written before decision 4
landed), not a live product/API design tradeoff, so it does not meet the
long-loop's escalation bar (not irreversible — a two-line, git-tracked test
edit; not out of scope — still the same function, same feature test's
intent; not underdetermined — decision 4's rationale plus crate convention
both point the same direction). The two lines to change are the `Act 2` call
in the happy-path test (`let calibration = compute_calibration(&report,
&labels);`) and nothing else — the assertions immediately below it are
unaffected since they operate on the unwrapped `CalibrationReport` either
way.

**Step count check.** Six steps (0 through 5) — within the 3-7 range; no
resizing needed.
