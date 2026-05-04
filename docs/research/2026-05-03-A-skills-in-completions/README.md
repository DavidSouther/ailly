# Skills in Completion Conversations

Research conducted 2026-05-03 on `feat_knowledge_skills` branch at commit `9db3a02`.

## Question

Where are Skill files typically included in Completion conversations, and how does Ailly assemble system content today through Rig?

## Summary

Anthropic offers two surfaces:

- **Standard Messages API** has no `skills` field. Skills enter via the `system` parameter (tier-1 frontmatter) plus model-issued tool calls that read `SKILL.md` on demand (tiers 2 and 3). Orchestration is application-side.
- **Managed Agents API** declares skills once on agent create. The container mounts the skill bundle and the agent reads it via filesystem.

In Ailly today, `.ailly.toml` `system` strings are accumulated into a `Vec<Message>` and emitted at the head of `Conversation::history_for(turn)`. They flow through `RigEngine::stream` as inline `Message::System` entries because `with_preamble()` is never called from `build_engine`.

The Rig 0.36 Anthropic provider extracts inline `Message::System` entries from history via `split_system_messages_from_history` and merges them into the Anthropic `system:` array. The system chain therefore does reach the `system:` parameter today. What is lost by leaving `with_preamble()` unwired is ordering control, not delivery.

The deeper constraint is in Rig itself: `cache_control` is hard-coded to `None` on every `SystemContent::Text` produced through `AgentBuilder`. Per-block prefix caching for skill descriptions, the standard pattern for tier-1 progressive disclosure, is unreachable through Rig's high-level API at this version.

## Files

- [public.md](public.md) — How Anthropic includes Skills in the Messages and Managed Agents APIs.
- [codebase.md](codebase.md) — How Ailly assembles system content from `.ailly.toml` and where it crosses into Rig.
- [dependencies.md](dependencies.md) — Rig `Agent`, `AgentBuilder`, and the Anthropic provider mapping at version 0.36.0.

## Decision pending

Whether to:

1. Keep using inline `Message::System` and accept no per-block cache control on skill content, or
2. Bypass `AgentBuilder` and construct `CompletionRequest` directly to set `cache_control` on individual `SystemContent::Text` blocks, or
3. Patch or fork Rig to expose `cache_control` on its system-content path.
