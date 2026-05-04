# TASK-NOTES: Upstream `cache_control` exposure to `rig-core`

## Context

Ailly relies on Anthropic's prefix cache to make Skill content economical at scale. Skill bodies are stable across turns and ideal candidates for `cache_control: ephemeral` breakpoints. As of `rig-core = "0.36.0"` (`Cargo.toml:18`), the `AgentBuilder` and `CompletionRequest` paths hard-code `cache_control: None` on every `SystemContent::Text` they emit, so the public Rig API cannot place a cache breakpoint on a per-block basis.

The 2026-05-03-B slice resolves this for Ailly by bypassing `AgentBuilder` in the Anthropic adapter and constructing `CompletionRequest` directly, threading `cache_control: ephemeral` onto the Skill blocks chosen by the budget heuristic. That bypass is mechanical because `CompletionRequest` is a public struct, but it is the only place in the engine layer that does not go through `AgentBuilder`. Removing it later requires the same field to be reachable through the supported builder API.

References:
- Design rationale: `docs/developer/2026-05-03-B-knowledge-skills/design.md`, "Patch or fork Rig to expose `cache_control`" alternative, and Deferred Decision 12.
- Underlying research: `docs/research/2026-05-03-A-skills-in-completions/` documents how Rig 0.36 routes system content through `split_system_messages_from_history` and where the `None` is wired in.
- In-tree consumer (once the bypass lands): `src/engine/rig_engine.rs`.

## Goal

Submit an upstream patch to `rig-core` that exposes `cache_control` on `SystemContent::Text` through the `AgentBuilder` API, so that a future Rig release lets Ailly drop the in-tree Anthropic bypass and route every provider through the same builder path.

## What to investigate

1. **Upstream surface.** Identify where in `rig-core` the `SystemContent::Text { cache_control: None }` is constructed for both the `req.preamble` path and the `history_system` path. Confirm whether a single change point exists or whether the two paths require parallel edits.
2. **Builder shape.** Decide how the field should be exposed. Candidates: a typed `preamble_with_cache(blocks)` builder method that takes structured entries, an opt-in `with_cache_control(...)` decorator on the existing `preamble(...)` string entry, or a richer `SystemBlock` input type. Match Rig's existing builder ergonomics rather than introducing a new pattern.
3. **Provider isolation.** `cache_control` is Anthropic-specific. Confirm the patch does not leak the field into provider-neutral structs in a way that requires non-Anthropic adapters to know about it. The OpenAI and Bedrock adapters currently drop the flag silently and should continue to.
4. **Maintainer signal.** Open a discussion or draft PR upstream before committing to a shape; if maintainers are cool to the idea, this task pivots to a maintained fork pin and the bypass stays.

## Acceptance

- A merged `rig-core` release exposes per-block `cache_control` through the public builder.
- Ailly bumps `rig-core` to that release.
- `src/engine/rig_engine.rs` no longer constructs `CompletionRequest` directly; the Anthropic adapter goes through `AgentBuilder` like the OpenAI and Bedrock adapters.
- The two acceptance tests carried over from the 2026-05-03-B slice still pass: `anthropic_request_carries_cache_control_ephemeral_on_skill_blocks` and `anthropic_request_carries_cache_control_on_at_most_four_blocks`.

## Dependencies

- Blocked behind the sibling task that lands the in-tree Anthropic bypass for `PreambleBlock::Skill`. There is nothing to upstream until the in-tree shape is settled and the desired API surface is concrete.
- Blocked behind upstream maintainer review cadence; this task is parallel and non-blocking for any Ailly slice.

## Out of scope

- Other Rig API gaps (tool-result routing, streaming shape, etc.).
- Forking `rig-core` permanently. If upstream rejects the change, reopen as a separate fork-and-pin task with its own maintenance plan.
