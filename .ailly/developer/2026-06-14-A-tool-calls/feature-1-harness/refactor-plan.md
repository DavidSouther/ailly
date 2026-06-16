# Feature 1 Refactor Plan

Post-green cleanup of the Feature 1 surface (`knowledge/tools/`, the `Conversation::run`
loop, the assembly resolver, the rig lowering). Working dir clean, all 6 steps green.
One refactoring at a time; check + tests after each.

- [x] **Incomplete Library Class / Duplicated Code** — `src/engine/rig_engine.rs:106`
  `yaml_value_to_json` and `src/knowledge/assertions.rs:1150` `yaml_to_json` are
  byte-identical `serde_yaml_ng::Value -> serde_json::Value` round-trips. The design
  flagged this as the refactor-phase candidate. Extract one
  `crate::content::conversation::yaml_value_to_json` (content is the canonical home for
  `serde_yaml_ng::Value` and is reachable by both `engine` and `knowledge` without
  inverting layering); both call sites delegate.

- [x] **Comments (stale doc)** — `src/content/assembly.rs:218-220` the `Assembly::render`
  `# Errors` doc omits the `RenderError::ToolParse` variant the method now returns via
  `resolve_tool_defs`. Add it.

- [x] **Redundant assertion on the same path** — `src/engine/rig_engine.rs:971`
  `assert_eq!(lowered[0].parameters, yaml_value_to_json(&input_schema))` checks the
  lowering output against the very function it calls; the following `serde_json::json!`
  literal is the load-bearing assertion. Drop the tautological one.
