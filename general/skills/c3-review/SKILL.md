---
name: c3-review
description: Provide review and feedback based on Correctness, Conciseness, and Clarity. Given a scope of artifacts, reviews them in their entirety and provide feedback that gauges those three aspects.
---

# C3 Review

Collect evidence-grounded feedback on an artifact. C3 means **Correctness**, **Conciseness**, and **Clarity**. This skill only evaluates: it returns findings with severity and evidence. It does not edit the artifact, choose any other reviewer skills, or converge findings from other reviewers.

## C3 Rubric

- **Correctness:** Treat each load-bearing statement as a claim to verify, not a statement to trust. Trace file paths, identifiers, environment variables, API signatures, URLs, version numbers, and quoted values back to code, command output, or a cited document. Flag a claim you cannot trace this way, even if it reads as plausible — a confident tone is not evidence.
- **Conciseness:** Tighten padding without losing necessary depth. Cut clauses that only restate their subject, empty intensifiers, repeated summaries, and rhythm-only phrasing. Leave alone any material that changes meaning if removed — the target is padding, not compression or deletion.
- **Clarity:** Flag vague hedges and filler, but do not flag domain jargon with context-specific meanings. Favor complete, direct sentences. When unusual punctuation fuses independent ideas, buries the subject behind qualifiers, or disguises a list of full clauses, flag the sentence and its paragraph for restructuring. Flag the entire sentence, not just the punctuation. Swapping the em-dash for a colon leaves the same weak structure in place.

### Examples

**Correctness**
- Claim: "The config defaults to `port: 8080`." Evidence check: open the config file and read the default. If it reads `port: 3000`, flag it — do not accept the claim because it sounds specific.
- Claim: "This calls the same `validate()` used elsewhere." Evidence check: trace both call sites to confirm they resolve to the same function, not two functions that share a name.

**Conciseness**
- Before: "It is important to note that the function will, in most cases, generally return a value that represents the result." After: "The function returns the result." Flag the padding, not the sentence's existence.
- Do not flag: a precise, technical sentence that is already load-bearing and would lose meaning if shortened.

**Clarity**
- Before: "The service — which, depending on load and a few other factors, may or may not respond in time — handles requests." Flag the sentence and its paragraph for restructuring: the subject is buried and the em-dash clause hides a real conditional.
- Do not flag: "The service handles requests idempotently," even though "idempotently" is jargon — it is precise and load-bearing for this audience.

## Severity

Use exactly one of these per finding:

- **High:** The claim is false, the artifact will mislead or break its consumer, or the sentence structure obscures a critical fact. Should be independently reviewed and considered before correcting. 
- **Medium:** The claim is unverifiable from available evidence, or padding/vagueness meaningfully slows a careful reader.
- **Low:** Minor tightening or a small clarity gain; safe to batch corrections without individual scrutiny.

Severity drives the cold-challenge selection below, so pick the level the finding would still deserve after someone else double-checks it — not the level that gets it noticed fastest.

## Findings

Report only findings that the supplied artifact and its cited evidence actually support — an isolated collection lane has no way to check with the orchestrator, so when evidence is ambiguous, omit the finding rather than guess.

For each finding, return:

```
findingId: <stable slug or number, unique within this run>
severity: High | Medium | Low
category: correctness | conciseness | clarity
claim: <the exact statement in the artifact being flagged>
evidence: <file:line, command output, or cited document location>
note: <one sentence — what's wrong and, for correctness, what the evidence actually shows>
```

`findingId` must stay stable if this skill is asked to re-run against a revised artifact section — review and independent review both refer back to findings by this id, and a renumbered id looks like a new finding to them.

## Independent Review

The isolated C3 collection lane above does not run this protocol itself — it only produces the findings another agent later acts on. During full `general:review`, `general:review` or its caller invokes the protocol only if convergence selected a High-severity finding. Select exactly one deterministically: severity first, then stable-id order. If no finding is High severity, do not challenge.

The challenger reads only the finding's `findingId`, claim, and evidence, plus the artifact — it does not see other findings or the convergence discussion, so it can't anchor on them. It returns one counterclaim naming the `findingId`, evidence locations, and a falsification condition that could refute the counterclaim. It does not edit, score, or debate in rounds.

A fresh final verifier reads the original finding, counterclaim, and artifact. For a consequential challenge, evaluate both presentation orders (finding-first and counterclaim-first) to catch order-dependent judgment. Report exactly one status:

- **accepted** — the challenge refutes the finding.
- **rejected** — the finding stands.
- **unresolved** — the evidence cannot decide, or the two presentation orders disagree.
- **failed** — the challenger or verifier could not run.

Accepted findings are removed from the actionable list. Rejected findings are retained. Unresolved findings escalate to the developer if substantial, and are otherwise removed. Failed findings are also removed.

## When Not to Use

- Do not use C3 alone for an Ailly phase review; pair it with Intent review for alignment with the original request.
- Do not use it as a replacement for `general:review`, which composes C3 with applicable specialists and converges the collected feedback; running C3 alone skips specialist coverage and the verify/dedupe/rank pass that turns raw findings into an actionable list.
