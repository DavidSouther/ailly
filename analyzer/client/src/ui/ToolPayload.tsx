import { type SourceValue, isRecorded } from "../tauri";
import { ClampedText } from "./CapturedOutput";

/**
 * Why a payload is missing is evidence too, so each way of missing reads
 * differently from the others and from a call that recorded nothing extra.
 */
const UNAVAILABLE: Record<Exclude<SourceValue<string>, { Recorded: string }>, string> = {
  Absent: "Parameters not recorded",
  Unsupported: "Parameters recorded in a form this view cannot show",
  Malformed: "Parameters recorded but unreadable",
};

/**
 * The parameters a call recorded beyond the rows already shown for it, as
 * `toolPayloadText` decoded them. `null` is the common case for a call whose
 * whole payload is its command or path, and renders nothing at all.
 */
export function ToolPayload({ payload }: { payload: SourceValue<string> | null }) {
  if (payload === null) {
    return null;
  }
  return (
    <div className="flex w-full min-w-0 flex-col gap-1">
      <span className="eyebrow-sm text-foreground-muted">Parameters</span>
      {isRecorded(payload) ? (
        <ClampedText text={payload.Recorded} />
      ) : (
        <p className="text-foreground-muted text-xs italic">{UNAVAILABLE[payload]}</p>
      )}
    </div>
  );
}
