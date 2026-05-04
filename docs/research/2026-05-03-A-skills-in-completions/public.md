# Public: where Skill files are typically included in Completion conversations

## Findings

Anthropic exposes two surfaces, and Skills enter the wire format differently in each.

### Standard Messages API (`/v1/messages`)

There is no native `skills` field on the request body. The Messages API never sees the word "skill". Application-side orchestrators integrate Skill content through one of three patterns ([Agent Skills Overview](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview)):

1. The skill's tier-1 content, the YAML frontmatter `name` and `description`, sits in the `system` parameter for every turn. Roughly 100 tokens per skill.
2. The full `SKILL.md` body is injected into `system` or appended as a `messages` content block when the orchestrator decides the skill is relevant.
3. Skill files are exposed as readable resources, fetched by the model via bash or text-editor tool calls during the loop.

The orchestrator carries the responsibility. The API itself is unaware.

### Managed Agents API (`/v1/agents`, `/v1/sessions`)

Skills are first-class. They are declared once on agent creation under `skills: [{type, skill_id, version}]` ([Managed Agents Skills](https://platform.claude.com/docs/en/managed-agents/skills.md)), persisted on the agent object, and inherited by every session that references the agent. The session container mounts the skill bundle on the filesystem; the agent reads it via filesystem access. Sessions never echo `skills` in `events.send` payloads. The beta header is `managed-agents-2026-04-01`.

### Progressive disclosure

Three tiers ([anthropics/skills](https://github.com/anthropics/skills)):

- Tier 1, always loaded. YAML frontmatter in the system prompt.
- Tier 2, loaded on demand. The model issues a tool call, typically `bash` reading `SKILL.md`, when it judges the skill relevant. The body enters context only at that point.
- Tier 3, deeper resources. Bundled assets (FORMS.md, helper scripts, schemas) are read on further demand. Executable scripts run via bash; only stdout enters context, not the script source.

The "load when relevant" step is a tool call the model emits, not a side channel injected by the API.

## Sources

- [Agent Skills Overview](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview) — anatomy and progressive-disclosure model
- [Managed Agents Skills](https://platform.claude.com/docs/en/managed-agents/skills.md) — `skills` array on agent create
- [anthropics/skills](https://github.com/anthropics/skills) — reference bundles, three-tier loading
