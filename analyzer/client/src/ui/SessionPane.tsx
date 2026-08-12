import type { SourceValue } from "../tauri";
import { PanelTabs, PanelTabsContent, PanelTabsList, PanelTabsTrigger } from "./PanelTabs";
import { Conversation } from "./conversation/Conversation";
import { SummaryPane } from "./summary/SummaryPane";
import { useSessionEvents } from "./useSessionEvents";

const PANE_CLASS = "flex min-h-0 min-w-0 flex-1 flex-col";

/**
 * The right pane's two lenses over one session. Summary opens first —
 * investigation before reading — and both tabs render from a single load of
 * the session's events.
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
    <PanelTabs defaultValue="summary" className={`${PANE_CLASS} border-l`}>
      <PanelTabsList>
        <PanelTabsTrigger value="summary" label="Summary" />
        <PanelTabsTrigger value="conversation" label="Conversation" />
      </PanelTabsList>
      <PanelTabsContent value="summary" className={PANE_CLASS}>
        <SummaryPane state={state} project={project} />
      </PanelTabsContent>
      <PanelTabsContent value="conversation" className={PANE_CLASS}>
        <Conversation state={state} project={project} />
      </PanelTabsContent>
    </PanelTabs>
  );
}
