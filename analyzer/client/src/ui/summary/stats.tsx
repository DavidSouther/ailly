import { ChevronDown, ChevronRight, FileText } from "lucide-react";
import { type ReactNode, useState } from "react";

import { type SourceValue, isRecorded } from "../../tauri";
import { CapturedOutput } from "../CapturedOutput";
import { ToolPayload } from "../ToolPayload";
import { Badge } from "../badges/badge";
import { BadgeColor } from "../colors";
import { shouldShowToolCwd } from "../conversation/format";
import { toolIcon } from "../toolIcons";
import type { FileTouch, SourceCall, ToolCategory, ToolFrequency } from "./rollup";

/**
 * The vocabulary every lens over a session's tool calls shares: labelled stat
 * tiles, ranked lists, and one word for absence. The Subagents tab folds a
 * child session with the same `summarizeSession` the Summary pane uses, so it
 * renders the result with these same components rather than a lookalike set.
 */

/** How many rows a ranked list shows before it caps with a visible "+N more". */
export const LIST_CAP = 10;

export const CATEGORY_LABEL: Record<ToolCategory | "unclassified", string> = {
  exec: "Exec / shell",
  edit: "Edit / write",
  read: "Read",
  other: "Other",
  unclassified: "Unclassified",
};

export const CATEGORY_COLOR: Record<ToolCategory | "unclassified", BadgeColor> = {
  exec: BadgeColor.AMBER,
  edit: BadgeColor.SKY,
  read: BadgeColor.MINT,
  other: BadgeColor.PLUM,
  unclassified: BadgeColor.METAL_DARK,
};

/** Renders a recorded value, or the explicit label the product owes the user. */
export function recordedLabel<T>(value: SourceValue<T>, render: (value: T) => string): string {
  return isRecorded(value) ? render(value.Recorded) : "Not recorded";
}

export function SectionHeading({ children }: { children: ReactNode }) {
  return <h2 className="eyebrow-sm text-foreground-muted">{children}</h2>;
}

export function StatTile({ label, value }: { label: string; value: string }) {
  return (
    <fieldset aria-label={label} className="flex flex-col gap-0.5 px-3 py-2">
      <span className="font-semibold text-2xl text-foreground">{value}</span>
      <span className="eyebrow-sm text-foreground-muted">{label}</span>
    </fieldset>
  );
}

export function MoreRow({ hidden, noun }: { hidden: number; noun: string }) {
  if (hidden <= 0) {
    return null;
  }
  return (
    <p className="text-foreground-muted text-xs">
      +{hidden} more {noun}
      {hidden === 1 ? "" : "s"} not shown
    </p>
  );
}

/**
 * Ranked tools, each expandable into the individual calls that used to live
 * under Sources — command, path, or URL, with captured output on demand.
 */
export function CallsByTool({
  tools,
  project,
}: { tools: ToolFrequency[]; project: SourceValue<string> }) {
  const shown = tools.slice(0, LIST_CAP);
  const sessionCwd = isRecorded(project) ? project.Recorded : null;
  return (
    <div className="flex flex-col gap-1.5">
      <SectionHeading>Calls by tool</SectionHeading>
      <ul aria-label="Calls by tool" className="rounded-md border">
        {shown.map((tool) => (
          <ToolCallsRow key={tool.name} tool={tool} sessionCwd={sessionCwd} />
        ))}
      </ul>
      <MoreRow hidden={tools.length - shown.length} noun="tool" />
    </div>
  );
}

function ToolCallsRow({ tool, sessionCwd }: { tool: ToolFrequency; sessionCwd: string | null }) {
  const [open, setOpen] = useState(false);
  const Icon = toolIcon(tool.name);
  return (
    <li className="border-b last:border-b-0">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-background-hover-solid"
      >
        {open ? (
          <ChevronDown size={14} className="shrink-0 text-foreground-muted" />
        ) : (
          <ChevronRight size={14} className="shrink-0 text-foreground-muted" />
        )}
        <Icon size={14} className="shrink-0 text-foreground-muted" aria-hidden="true" />
        <span className="min-w-0 truncate font-medium text-foreground-title">{tool.name}</span>
        <span className="shrink-0 text-foreground-muted text-xs">
          {CATEGORY_LABEL[tool.category]}
        </span>
        <span className="ml-auto shrink-0 text-foreground-muted text-xs">{tool.count} calls</span>
        <Badge color={CATEGORY_COLOR[tool.category]} textSize="sm" className="shrink-0">
          {tool.share}%
        </Badge>
      </button>
      {open ? (
        <ul
          aria-label={`${tool.name} calls`}
          className="flex min-w-0 flex-col gap-1.5 border-t px-3 py-2 pl-8"
        >
          {tool.calls.map((call) => (
            <li key={call.eventId} className="min-w-0">
              <CallRow call={call} sessionCwd={sessionCwd} />
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  );
}

/** One call, expanding to what it captured. */
function CallRow({ call, sessionCwd }: { call: SourceCall; sessionCwd: string | null }) {
  const [open, setOpen] = useState(false);
  const showCwd = shouldShowToolCwd({ cwd: call.cwd, path: call.path, sessionCwd });
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="focus-ring flex w-full min-w-0 items-start gap-1.5 rounded-sm text-left hover:bg-background-hover-solid"
      >
        {open ? (
          <ChevronDown size={12} className="mt-0.5 shrink-0 text-foreground-muted" />
        ) : (
          <ChevronRight size={12} className="mt-0.5 shrink-0 text-foreground-muted" />
        )}
        <span
          className={
            call.detailRecorded
              ? "min-w-0 break-all font-mono text-foreground text-xs"
              : "min-w-0 text-foreground-muted text-xs italic"
          }
        >
          {call.detail}
        </span>
      </button>
      {showCwd ? (
        <span className="pl-[18px] font-mono text-foreground-muted text-xs">
          Working directory: {call.cwd}
        </span>
      ) : null}
      {open ? (
        <div className="flex min-w-0 w-full flex-col gap-1.5 pt-1 pl-[18px]">
          <ToolPayload payload={call.payload} />
          <CapturedOutput output={call.output} isError={call.outputIsError} />
        </div>
      ) : null}
    </div>
  );
}

export function FileAccessList({ files }: { files: FileTouch[] }) {
  const shown = files.slice(0, LIST_CAP);
  return (
    <>
      <ul aria-label="Files touched" className="flex flex-col gap-1.5">
        {shown.map((file) => (
          <li key={file.path} className="flex min-w-0 items-center gap-2">
            <FileText size={14} className="shrink-0 text-foreground-muted" />
            <span
              title={file.path}
              className="truncate-start min-w-0 flex-1 font-mono text-foreground text-xs"
            >
              {file.path}
            </span>
            <div className="flex shrink-0 gap-1">
              {file.tools.map((tool) => (
                <Badge key={tool} color={BadgeColor.METAL} textSize="sm">
                  {tool}
                </Badge>
              ))}
            </div>
            <span className="shrink-0 text-foreground-muted text-xs">
              {file.touches} {file.touches === 1 ? "touch" : "touches"}
            </span>
          </li>
        ))}
      </ul>
      <MoreRow hidden={files.length - shown.length} noun="file" />
    </>
  );
}
