import { type SourceValue, type ToolCall, isRecorded } from "../tauri";

/**
 * The recorded parameters of a call that its other rows do not already show.
 *
 * Two things stand between the transcript and a readable payload. The loader
 * stores the payload as `Value::to_string()`, so an object arrives compacted
 * and a payload that was itself a JSON string arrives quoted and escaped;
 * decoding undoes that encoding without interpreting any field. And a payload
 * usually repeats the command, path, or URL already rendered beside it, so
 * those values are subtracted rather than printed twice.
 *
 * `null` means there is nothing left to show — not that the harness recorded
 * nothing, which the unrecorded `SourceValue` states report on their own.
 */
export function toolPayloadText(tool: ToolCall): SourceValue<string> | null {
  if (!isRecorded(tool.input)) {
    return tool.input === "Absent" ? null : tool.input;
  }
  const shown = new Set(
    [tool.command, tool.path, tool.url, tool.cwd].filter(isRecorded).map((value) => value.Recorded),
  );
  const raw = tool.input.Recorded;

  let decoded: unknown;
  try {
    decoded = JSON.parse(raw);
  } catch {
    // Not every payload is JSON: Codex records some as a JavaScript snippet.
    return shown.has(raw) ? null : { Recorded: raw };
  }

  if (typeof decoded === "string") {
    return shown.has(decoded) ? null : { Recorded: decoded };
  }
  if (decoded === null || typeof decoded !== "object" || Array.isArray(decoded)) {
    return shown.has(raw) ? null : { Recorded: raw };
  }

  const remaining = Object.entries(decoded).filter(
    ([, value]) => !(typeof value === "string" && shown.has(value)),
  );
  if (remaining.length === 0) {
    return null;
  }
  return { Recorded: remaining.map(payloadLine).join("\n") };
}

/**
 * One field per line, with a multi-line value starting on its own line so the
 * source it carries reads as source rather than as a run-on value.
 */
function payloadLine([key, value]: [string, unknown]): string {
  if (typeof value !== "string") {
    return `${key}: ${JSON.stringify(value)}`;
  }
  return value.includes("\n") ? `${key}:\n${value}` : `${key}: ${value}`;
}
