# Plan: Verify `ailly-skill-eval` Falsification Gate (Rust API only)

**Feature test:** `tests/falsification_gate.rs` ::
`falsification_gate_matches_the_documented_improved_and_regressed_formula`
(committed RED in this worktree; confirmed failing with exactly three
`E0599: no method named 'passes_falsification_gate' found for struct
'ComparisonTotals'` errors, no other errors).

**User story:** As a developer about to build a new `ailly-skill-eval` suite,
I need `improved > 0 && regressed == 0` — the falsification gate that
`SKILL.md` and `references/method.md` §6 document — to be one named, tested
Rust function I can call, instead of a boolean I re-derive by hand from a
`ComparisonTotals`.

**Scope cut (per design.md and the coordinating brief):** this plan builds
*only* `ComparisonTotals::passes_falsification_gate` in
`src/knowledge/report.rs`. It does not touch `docs/developer/TASKS.md`, does
not rewire `e2e/patterns-eval/ci.sh`'s Python heredoc or
`tests/skill_forge_clean_comments_review.rs`, does not fix CI's
`ANTHROPIC_API_KEY` secret, and does not run the live `patterns-eval`
re-confirmation described in design.md's Specification step 2. See
"Deferred items" at the end.

## Steps checklist

- [ ] Step 0 — API surface stub (signature only, no logic)
- [ ] Step 1 — Unblock the "clears the gate" assertion (scenario 1); add `mod tests` + first unit test
- [ ] Step 2 — Unblock the "fails on regression" assertion (scenario 2); add second unit test
- [ ] Step 3 — Confirm the "fails as vacuous" assertion (scenario 3) needs no further generalization; add third unit test
- [ ] Step 4 — Doc comment, formatting, and whole-workspace lint/test confirmation

---

## Step 0 — API surface stub

Add the method's signature to `src/knowledge/report.rs`, next to the existing
`ComparisonTotals` struct definition (there is no existing `impl
ComparisonTotals` block; this creates the first one). No formula, no field
reads — a body that compiles but does not yet satisfy any assertion:

```rust
impl ComparisonTotals {
    pub fn passes_falsification_gate(&self) -> bool {
        todo!()
    }
}
```

This is enough to turn the current compile-time `E0599` (method not found)
into a runtime panic on the first assertion — i.e. it moves the failure from
"won't build" to "builds, fails deliberately," which is the correct starting
point for Step 1. No test assertion is satisfied yet.

## Step 1 — Unblock scenario 1: "clears the gate"

**Assertion unblocked:** `tests/falsification_gate.rs:106-109` —
`assert!(clears.totals.passes_falsification_gate(), "improved > 0 &&
regressed == 0 must clear the gate")`, where the fixture pair yields
`improved: 1, regressed: 0`.

**Happy-path test sketch** (already written, not new — this step targets the
existing scenario 1 block): build a baseline/invocation `EvalReport` pair via
the test's `fixture_report` helper where one assertion class flips
`fail → pass` (judge) and the others stay stable (`script` fail/fail,
`tokens` pass/pass); run `compute_comparison`; call
`.totals.passes_falsification_gate()`; expect `true`.

**Implementation outline:** replace the `todo!()` body with the simplest
predicate that satisfies this one scenario without yet accounting for
regression — i.e. a "fake it" pass keyed only on `self.improved`. This is
intentionally under-specified relative to the documented two-conjunct gate;
Step 2 supplies the second conjunct. Do not consult scenario 2 or 3's
fixtures yet when writing this step's body.

**Unit test (repo convention):** `src/knowledge/report.rs` has no
`#[cfg(test)] mod tests` block yet, unlike its sibling modules
(`src/knowledge/assertions.rs:1326`, `src/knowledge/eval.rs`). Add one in
this step with a direct, fixture-free unit test constructing
`ComparisonTotals { improved: 1, regressed: 0, ..Default::default() }` and
asserting `.passes_falsification_gate()` — this exercises the predicate
without going through `compute_comparison`/`EvalReport`, matching the "unit
tests per module plus the one feature test" convention the feature test
alone does not force.

## Step 2 — Unblock scenario 2: "fails on regression"

**Assertion unblocked:** `tests/falsification_gate.rs:130-133` —
`assert!(!regressed.totals.passes_falsification_gate(), "a single regression
must fail the gate even though improved > 0")`, where the fixture pair
yields `improved: 1, regressed: 1`.

**Happy-path test sketch** (already written — existing scenario 2 block):
build a pair where `judge` improves (`fail → pass`) *and* `tokens` regresses
(`pass → fail`) in the same comparison; run `compute_comparison`; call
`.totals.passes_falsification_gate()`; expect `false`.

**Implementation outline:** triangulate against Step 1's under-specified body
— Step 1's predicate (keyed only on `improved`) would wrongly return `true`
here, so this step forces the second conjunct into the body: the method must
also read `self.regressed` and require it to be `0`. After this change,
re-check that Step 1's scenario is still satisfied (it is, since `regressed
== 0` holds there too). This step is where the implementation converges on
the exact documented formula, `self.improved > 0 && self.regressed == 0`.

**Unit test:** add a second case to Step 1's new `mod tests` block —
`ComparisonTotals { improved: 1, regressed: 1, ..Default::default() }`
asserting `!.passes_falsification_gate()` — pinning the same triangulation
the feature test's scenario 2 pins, directly against the struct.

## Step 3 — Confirm scenario 3: "fails as vacuous"

**Assertion unblocked:** `tests/falsification_gate.rs:157-159` —
`assert!(!vacuous.totals.passes_falsification_gate(), "improved == 0 must
fail the gate even when regressed == 0 too")`, where the fixture pair yields
`improved: 0, regressed: 0`.

**Happy-path test sketch** (already written — existing scenario 3 block):
build a pair where both arms fail the same checker (`script`
`fail`/`fail`, no class changes at all); run `compute_comparison`; call
`.totals.passes_falsification_gate()`; expect `false`.

**Implementation outline:** no further code change is expected — the
two-conjunct formula reached at the end of Step 2 already evaluates
`0 > 0 && 0 == 0` to `false`. This step is a verification checkpoint, not a
new generalization: run the full `falsification_gate` test and confirm all
three scenarios pass together in one process (the earlier steps only reason
about each scenario in isolation). If this step's assertion fails, that is a
signal Step 2's generalization was wrong (e.g. it used `>=` instead of `>`,
or dropped the `improved` conjunct entirely) and Step 2 must be revisited —
do not patch scenario 3 with a special case.

**Unit test:** add the third case to the same `mod tests` block —
`ComparisonTotals::default()` (i.e. `improved: 0, regressed: 0`) asserting
`!.passes_falsification_gate()` — completing the three-case unit-test set
that mirrors the feature test's three scenarios directly against the struct.

## Step 4 — Doc comment, formatting, and whole-workspace confirmation

**Assertion unblocked:** none new — this step closes out the feature test
as a whole and guards against regressions elsewhere in the workspace.

**Happy-path test sketch:** re-run
`cargo test --test falsification_gate --all-features` (or the project's
canonical test task) and confirm it is green; then run the full workspace
test/check/lint tasks to confirm nothing else broke.

**Implementation outline:**
1. Attach the doc comment from design.md's Specification verbatim (the
   `/// The falsification gate documented in ... SKILL.md ... and
   references/method.md §6: ...` comment) above the method, so the method's
   own doc explains *why* the formula is what it is, not just what it
   computes.
2. Run `mise run check`, `mise run test`, and `mise run lint` (this repo's
   canonical commands per `mise.toml`) from the worktree root — not ad hoc
   `cargo` invocations — and confirm all three are clean, including the
   existing `tests/report_cmd.rs` fixtures (which exercise `compute_comparison`
   but not the new method) and `tests/skill_eval_guide.rs` (unaffected, reads
   only the doc files on disk).
3. Confirm `cargo fmt` produces no diff (or run `mise run format` and check
   `git diff` is empty) so the new `impl` block matches the file's existing
   style.

---

## Deferred items (explicitly out of scope for this plan)

- **`docs/developer/TASKS.md`** — not touched. It carries unrelated
  uncommitted changes from the in-flight `docs/developer/2026-06-26-A-eval-static-doc`
  session; design.md's own Open Artifact Decision 1 says not to append to it
  until that session's edits have landed. *Needs a decision from:* the human
  coordinator (sequencing between the two in-flight sessions).
- **GitHub Actions `ANTHROPIC_API_KEY` secret (401 Unauthorized since
  2026-06-02)** — not rotated or investigated further; requires
  repo-secret-management access this plan does not exercise. *Needs a
  decision from:* the human coordinator (someone with repo secrets access).
- **`e2e/patterns-eval/ci.sh`'s Python heredoc** and
  **`tests/skill_forge_clean_comments_review.rs`'s inline gate assertions**
  — left exactly as-is; both already correctly re-derive the same formula by
  hand today, and design.md's Open Artifact Decision 3 recommends deferring
  their rewiring to the new API as a separate `TASKS.md` follow-up, not this
  feature-step. *Needs a decision from:* the human coordinator (whether/when
  to fold this refactor in).
- **Live re-confirmation of the `patterns-eval` gate** (design.md
  Specification step 2: clearing stale local run debris, running
  `bash e2e/patterns-eval/ci.sh` with real credentials, and recording the
  resulting bucket totals and verdict) — not attempted in this plan or its
  implementation. It depends on unresolved Open Artifact Decisions
  (where to record the result; whether to fix CI first) that design.md
  explicitly leaves to human review. *Needs a decision from:* the human
  coordinator (Open Artifact Decisions 1, 2, and 4 in design.md).

---

## Resolved by the long-loop reviewer (2026-07-06)

**1. Plan sizing and Step 0's API surface against actual code. Decided: keep
the 5-step shape (Step 0–4), unchanged, plus the unit-test addition in item 2
below.** Re-read cold against `src/knowledge/report.rs` on disk: the
`ComparisonTotals` struct (5 `pub usize` fields, `Default`-derived, no
existing `impl` block) matches Step 0's stub exactly, and a fresh
`cargo test --test falsification_gate --all-features` run reproduces the
plan's claimed RED state verbatim — exactly three `E0599` errors at lines
107, 131, 158, no others. Five steps is within the 3–7 band. The
fake-it-then-triangulate shape of Steps 1–2 is more ceremony than a
one-line, four-source-corroborated formula strictly needs, but it is exactly
the red-green-refactor discipline this feature-step's build instructions
require ("write or adjust the relevant unit test first... implement the
minimum to pass"), so it is right-sized for the process being followed, not
oversized for the formula alone. No restructuring needed.

**2. Repo convention gap: no unit tests inside `src/knowledge/report.rs`
itself. Decided: add a `#[cfg(test)] mod tests` block to `report.rs` (folded
into Steps 1–3, one case per step) with three direct, fixture-free unit
tests against `ComparisonTotals` literals, alongside the existing
fixture-driven feature test.** The plan as drafted only exercised the new
method through `tests/falsification_gate.rs`'s `compute_comparison`-fixture
path. `src/knowledge/assertions.rs:1326` and `src/knowledge/eval.rs` both
carry `#[cfg(test)] mod tests` blocks; `report.rs` currently has none. The
build instructions for this feature-step are explicit: "Write unit tests for
new logic even where the feature test alone would not force it — this
repo's convention is unit tests per module plus the one feature test." This
is the conservative default (matching an already-established, repo-wide
pattern) rather than a new convention being invented.

**3. Design.md's five Open Artifact Decisions. Decided: all five stay
deferred to the human coordinator, exactly as the plan's own "Deferred
items" section and this feature-step's authoritative extra notes already
state — no further action taken on any of them in this plan or its build.**
Specifically: (OAD 1) where the Build-phase live-run result gets recorded —
deferred, blocked on `docs/developer/2026-06-26-A-eval-static-doc` landing
first, per the extra notes' explicit instruction not to touch
`docs/developer/TASKS.md`. (OAD 2) fixing the GitHub Actions
`ANTHROPIC_API_KEY` secret — deferred, per the extra notes' explicit
instruction not to attempt this; it needs repo-secret-management access this
session does not have. (OAD 3) wiring `passes_falsification_gate` into
`ci.sh`'s Python heredoc and `tests/skill_forge_clean_comments_review.rs` —
deferred, per the extra notes' explicit instruction to leave both call sites
exactly as-is; this also matches design.md's own recommendation ("not in
this feature-step... both call sites already work correctly today"). (OAD 4)
cadence for re-running the live confirmation — deferred; design.md itself
makes no recommendation and ties it to whoever resolves OAD 2, so it moves
in lockstep with that decision. (OAD 5) cleaning up ~600 stale local
`e2e/patterns-eval/runs`/`evals/reports` directories — deferred; this is
listed in design.md as step 1 of the Build-phase live-run procedure (OAD
Specification step 2), and the extra notes explicitly exclude running that
live re-confirmation in this feature-step, so the cleanup step it belongs to
does not apply here either. None of these five is a prerequisite for the
Rust-API-only scope this plan builds (`ComparisonTotals::passes_falsification_gate`
and its tests do not read `docs/developer/TASKS.md`, CI secrets, `ci.sh`, or
local run directories), so none of them blocks this gate. No escalation
triggered under the long-loop escalation rule (irreversible /
out-of-recorded-scope / underdetermined): each of these five is already
explicitly resolved by this feature-step's authoritative extra notes, so
none is underdetermined, and none is being decided here beyond what those
notes already state.
