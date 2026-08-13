import { Bot, ChevronDown, ChevronRight, MessagesSquare, Terminal, Wrench } from "lucide-react";
import { type ReactNode, useState } from "react";

import { type AillyEvent, EventKind, type SourceValue, isRecorded } from "../../tauri";
import { CapturedOutput } from "../CapturedOutput";
import { ToolPayload } from "../ToolPayload";
import { toolPayloadText } from "../payload";
import { type LoadState, LoadStatus } from "../useSessionEvents";
import {
  type DetailRow,
  eventAnchorId,
  roleLabel,
  subagentDetail,
  toolDetailRows,
  toolResultTitle,
  toolTitle,
  turnText,
} from "./format";

interface ConversationProps {
  state: LoadState;
  project?: SourceValue<string>;
}

/**
 * The conversation lens: a completed session read top-to-bottom in source
 * order. Tool calls and subagent spawns stay collapsed so the transcript is
 * scannable and expand on demand. Artifact contents are deliberately omitted;
 * this view shows turns and references, not file bodies.
 */
export function Conversation({ state, project = "Absent" }: ConversationProps) {
  return (
    <section
      aria-label="Conversation"
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background"
    >
      <ConversationBody state={state} project={project} />
    </section>
  );
}

function ConversationBody({ state, project }: { state: LoadState; project: SourceValue<string> }) {
  if (state.status === LoadStatus.Idle) {
    return (
      <Placeholder title="No session selected" hint="Select a session to read its conversation." />
    );
  }
  if (state.status === LoadStatus.Loading) {
    return <Placeholder title="Loading conversation…" hint="Reading the session's events." />;
  }
  if (state.status === LoadStatus.Error) {
    return (
      <p className="mx-6 mt-4 rounded-md border border-status-error px-4 py-3 text-foreground-status-error">
        {state.message}
      </p>
    );
  }
  if (state.events.length === 0) {
    return (
      <Placeholder
        title="No events in this session"
        hint="This session was indexed without any readable events."
      />
    );
  }
  return (
    <ol className="flex flex-col gap-3 px-6 py-4">
      {state.events.map((event) => (
        <li key={event.id} id={eventAnchorId(event.id)}>
          <EventRow event={event} project={project} />
        </li>
      ))}
    </ol>
  );
}

function EventRow({ event, project }: { event: AillyEvent; project: SourceValue<string> }) {
  switch (event.kind) {
    case EventKind.UserTurn:
    case EventKind.AssistantTurn:
      return <TurnRow event={event} />;
    case EventKind.ToolCall:
      return <ToolCallRow event={event} project={project} />;
    case EventKind.ToolResult:
      return <ToolResultRow event={event} />;
    case EventKind.SubagentSpawn:
      return (
        <Expandable icon={<Bot size={14} />} title="Subagent spawn">
          <p className="whitespace-pre-wrap text-foreground">{subagentDetail(event)}</p>
        </Expandable>
      );
    default:
      return <MetaRow event={event} />;
  }
}

function TurnRow({ event }: { event: AillyEvent }) {
  const isUser = event.kind === EventKind.UserTurn;
  return (
    <div
      className={
        isUser ? "rounded-md border bg-background-active px-4 py-3" : "rounded-md border px-4 py-3"
      }
    >
      <p className="eyebrow-sm mb-1 text-foreground-muted">{roleLabel(event)}</p>
      <p className="whitespace-pre-wrap text-foreground leading-6">{turnText(event)}</p>
    </div>
  );
}

function ToolCallRow({ event, project }: { event: AillyEvent; project: SourceValue<string> }) {
  const tool = isRecorded(event.tool_call) ? event.tool_call.Recorded : null;
  const title = tool ? toolTitle(tool) : "Tool call";
  const rows: DetailRow[] = tool ? toolDetailRows(tool, project) : [];
  const payload = tool ? toolPayloadText(tool) : null;
  return (
    <Expandable icon={<Wrench size={14} />} title={title}>
      {rows.length === 0 && payload === null ? (
        <p className="text-foreground-muted">No recorded parameters.</p>
      ) : (
        <div className="flex min-w-0 flex-col gap-2">
          <dl className="flex flex-col gap-2">
            {rows.map((row) => (
              <div key={row.label}>
                <dt className="eyebrow-sm text-foreground-muted">{row.label}</dt>
                <dd className="whitespace-pre-wrap break-words font-mono text-foreground text-xs">
                  {row.value}
                </dd>
              </div>
            ))}
          </dl>
          <ToolPayload payload={payload} />
        </div>
      )}
    </Expandable>
  );
}

function ToolResultRow({ event }: { event: AillyEvent }) {
  const result = isRecorded(event.tool_result) ? event.tool_result.Recorded : null;
  const output = result === null ? null : result.output;
  const isError = result !== null && isRecorded(result.is_error) && result.is_error.Recorded;
  return (
    <Expandable icon={<Terminal size={14} />} title={toolResultTitle(result)}>
      <CapturedOutput output={output} isError={isError} />
    </Expandable>
  );
}

function MetaRow({ event }: { event: AillyEvent }) {
  const detail = isRecorded(event.detail) ? event.detail.Recorded : null;
  return (
    <p className="px-1 text-foreground-muted text-xs">
      <span className="font-medium">{event.kind.replace(/_/g, " ")}</span>
      {detail ? ` — ${detail}` : null}
    </p>
  );
}

/** A collapsed-by-default disclosure; detail is absent from the DOM until open. */
function Expandable({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded-md border">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-background-hover-solid"
      >
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <span className="text-foreground-muted">{icon}</span>
        <span className="font-medium text-foreground-title">{title}</span>
      </button>
      {open ? <div className="border-t px-3 py-2">{children}</div> : null}
    </div>
  );
}

function Placeholder({ title, hint }: { title: string; hint: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-16 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-background-active text-foreground-muted">
        <MessagesSquare size={26} strokeWidth={1.8} />
      </div>
      <h2 className="font-semibold text-foreground-title">{title}</h2>
      <p className="max-w-md text-foreground-muted leading-6">{hint}</p>
    </div>
  );
}
