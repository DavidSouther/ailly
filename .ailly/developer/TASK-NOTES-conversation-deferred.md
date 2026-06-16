# TASK-NOTES: Conversation deferred decisions

Carried over from [2026-05-23-A-content-conversation/design.md](2026-05-23-A-content-conversation/design.md) (now removed). Each item is a decision to revisit when the named downstream consumer lands and forces the question. Until then the current shape in [src/content/conversation.rs](../../src/content/conversation.rs) stands.

## ImageSource typed shape

Currently `ImageSource(serde_yaml_ng::Value)` with `#[serde(transparent)]` so any payload parses without rejection. Revisit when the first e2e fixture emits an `image` content block (insurance-claim is the most likely first source). At that point, model the typed variant per Anthropic's Messages API `image` schema (base64 vs URL source forms) and migrate the field.

## Narrow tool_result.content

Currently `ToolResult.content` is the full `Content` enum (string or block array). In practice tool results are almost always plain strings. Revisit after the engine adapter and tools tasks land: if every observed tool result in `e2e/` is `Content::Text`, narrow this field to `String` and drop the enum on tool results only. Keep `Content` on assistant turns where multi-block is real.

## Close TraceEvent into a named enum

Currently `TraceEvent { name: String, attributes: BTreeMap<String, serde_yaml_ng::Value> }`. Open shape so any provider's OTEL `gen_ai.*` event deserializes. Revisit when the engine adapter (`engine/rig.rs`) settles on the concrete event names it emits. If the set is small and stable, replace `name: String` with a closed enum and keep `attributes` open.

## Promote Conversation to an aggregate root

Currently a flat record with five free-standing methods. Per `patterns:aggregate`, this becomes a candidate for aggregate-root promotion when invariants accumulate that span multiple messages (for example: "filling assistant N requires assistant N-1 to be filled," or "trace token totals must equal sum of per-message tokens"). Today no such invariant exists; the operations are independent. Revisit during the `run` command task or the multi-turn-skeletons task, whichever introduces the first cross-message invariant.

## Future Message type-states

The `Message` struct currently uses `(role: Assistant, content: None)` as the in-band representation of a blank slot. A future `patterns:type-states` pass could split `Message` into `BlankAssistant` and `FilledAssistant` variants and lift the precondition checks in `fill_blank_assistant` into the type system. Revisit only if a bug or repeated misuse signals the runtime check is insufficient.
