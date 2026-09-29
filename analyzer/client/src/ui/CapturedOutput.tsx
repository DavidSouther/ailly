import { useState } from "react";

import { type SourceValue, isRecorded } from "../tauri";

/**
 * How many lines of a captured output render before the reader asks for the
 * rest. Results in the local corpus run to 136 KB; the point of the summary is
 * to stay scannable, so a long result is clamped rather than poured into the
 * pane.
 */
const CLAMP_LINES = 50;

/**
 * Why an output is missing is itself evidence, so each way of missing reads
 * differently: an image result is not the same fact as a call nothing ever
 * answered.
 */
const UNAVAILABLE: Record<Exclude<SourceValue<string>, { Recorded: string }>, string> = {
  Absent: "Output not recorded",
  Unsupported: "Output recorded in a form this view cannot show",
  Malformed: "Output recorded but unreadable",
};

/**
 * What a tool call returned. `null` means nothing in the session answered this
 * call at all, which differs from a result that recorded no output — and both
 * differ from an output recorded as an empty string.
 */
export function CapturedOutput({
  output,
  isError = false,
}: { output: SourceValue<string> | null; isError?: boolean }) {
  if (output === null) {
    return <p className="text-foreground-muted text-xs italic">Output not recorded</p>;
  }
  if (!isRecorded(output)) {
    return <p className="text-foreground-muted text-xs italic">{UNAVAILABLE[output]}</p>;
  }
  if (output.Recorded === "") {
    return <p className="text-foreground-muted text-xs italic">Recorded an empty output</p>;
  }

  return <ClampedText text={output.Recorded} isError={isError} />;
}

/**
 * Recorded text a session captured, bounded so one long block cannot swamp the
 * pane. Shared by every lens that shows captured bytes, so a result and a
 * call's parameters clamp at the same length and toggle the same way.
 */
export function ClampedText({ text, isError = false }: { text: string; isError?: boolean }) {
  const [showAll, setShowAll] = useState(false);
  const lines = text.split("\n");
  const clamped = !showAll && lines.length > CLAMP_LINES;
  return (
    <div className="flex w-full min-w-0 flex-col gap-1">
      <pre
        className={
          isError
            ? "w-full min-w-0 whitespace-pre-wrap break-words rounded-sm border border-status-error bg-background-active px-2 py-1.5 font-mono text-foreground-status-error text-xs"
            : "w-full min-w-0 whitespace-pre-wrap break-words rounded-sm border bg-background-active px-2 py-1.5 font-mono text-foreground text-xs"
        }
      >
        {clamped ? lines.slice(0, CLAMP_LINES).join("\n") : text}
      </pre>
      {lines.length > CLAMP_LINES ? (
        <button
          type="button"
          onClick={() => setShowAll((value) => !value)}
          className="focus-ring self-start rounded-sm text-foreground-muted text-xs underline underline-offset-2 hover:text-foreground"
        >
          {showAll ? `Show first ${CLAMP_LINES} lines` : `Show all ${lines.length} lines`}
        </button>
      ) : null}
    </div>
  );
}
