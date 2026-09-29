import type { SourceValue } from "../tauri";
import { SessionLenses } from "./SessionLenses";
import { Conversation } from "./conversation/Conversation";
import { useSessionEvents } from "./useSessionEvents";

/**
 * The right pane over one selected session. Summary opens first —
 * investigation before reading — and every tab renders from a single load of
 * the session's events. Subagents is present even when a session delegated
 * nothing, so its absence is never mistaken for a missing feature.
 */
export function SessionPane({
  sessionId,
  project,
}: { sessionId: string | null; project: SourceValue<string> }) {
  const state = useSessionEvents(sessionId);

  if (sessionId === null) {
    return <Conversation state={state} project={project} />;
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col border-l">
      <SessionLenses state={state} project={project} />
    </div>
  );
}
