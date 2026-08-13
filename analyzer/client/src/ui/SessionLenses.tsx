import type { SourceValue } from "../tauri";
import { PanelTabs, PanelTabsContent, PanelTabsList, PanelTabsTrigger } from "./PanelTabs";
import { Conversation } from "./conversation/Conversation";
import { SubagentsPane } from "./subagents/SubagentsPane";
import { SummaryPane } from "./summary/SummaryPane";
import type { LoadState } from "./useSessionEvents";

const PANE_CLASS = "flex min-h-0 min-w-0 flex-1 flex-col";

/**
 * The three lenses over one session's events — Summary, Conversation, and
 * Subagents. Used for the top-level selection and for a nested child opened
 * from a spawn, so both surfaces share one interface rather than a summary
 * lookalike.
 */
export function SessionLenses({
  state,
  project,
  defaultTab = "summary",
}: {
  state: LoadState;
  project: SourceValue<string>;
  defaultTab?: "summary" | "conversation" | "subagents";
}) {
  return (
    <PanelTabs defaultValue={defaultTab} className={PANE_CLASS}>
      <PanelTabsList>
        <PanelTabsTrigger value="summary" label="Summary" />
        <PanelTabsTrigger value="conversation" label="Conversation" />
        <PanelTabsTrigger value="subagents" label="Subagents" />
      </PanelTabsList>
      <PanelTabsContent value="summary" className={PANE_CLASS}>
        <SummaryPane state={state} project={project} />
      </PanelTabsContent>
      <PanelTabsContent value="conversation" className={PANE_CLASS}>
        <Conversation state={state} project={project} />
      </PanelTabsContent>
      <PanelTabsContent value="subagents" className={PANE_CLASS}>
        <SubagentsPane state={state} project={project} />
      </PanelTabsContent>
    </PanelTabs>
  );
}
