import { BarChart3, ChevronDown, ChevronRight, FileText } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";

import { type AillyEvent, type SourceValue, isRecorded } from "../../tauri";
import { CapturedOutput } from "../CapturedOutput";
import { Badge } from "../badges/badge";
import { BadgeColor } from "../colors";
import { shouldShowToolCwd } from "../conversation/format";
import { type LoadState, LoadStatus } from "../useSessionEvents";
import {
  type FileTouch,
  type SessionSummaryStats,
  type SourceCall,
  type SourceGroup,
  type ToolCategory,
  type ToolFrequency,
  summarizeSession,
} from "./rollup";

/** How many rows a ranked list shows before it caps with a visible "+N more". */
const LIST_CAP = 10;

const CATEGORY_LABEL: Record<ToolCategory | "unclassified", string> = {
  exec: "Exec / shell",
  edit: "Edit / write",
  read: "Read",
  other: "Other",
  unclassified: "Unclassified",
};

const CATEGORY_COLOR: Record<ToolCategory | "unclassified", BadgeColor> = {
  exec: BadgeColor.AMBER,
  edit: BadgeColor.SKY,
  read: BadgeColor.MINT,
  other: BadgeColor.PLUM,
  unclassified: BadgeColor.METAL_DARK,
};

/**
 * The investigation lens: one session's tool calls folded into totals, a
 * category split, per-tool and per-file rankings, and the calls through which
 * outside facts entered context. Dimensions the source never recorded are
 * labelled as such rather than rendered as zero.
 */
export function SummaryPane({
  state,
  project,
}: { state: LoadState; project: SourceValue<string> }) {
  return (
    <section
      aria-label="Session summary"
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background"
    >
      <SummaryBody state={state} project={project} />
    </section>
  );
}

function SummaryBody({ state, project }: { state: LoadState; project: SourceValue<string> }) {
  if (state.status === LoadStatus.Idle) {
    return <Placeholder title="No session selected" hint="Select a session to summarize it." />;
  }
  if (state.status === LoadStatus.Loading) {
    return <Placeholder title="Loading summary…" hint="Reading the session's events." />;
  }
  if (state.status === LoadStatus.Error) {
    return (
      <p className="mx-6 mt-4 rounded-md border border-status-error px-4 py-3 text-foreground-status-error">
        {state.message}
      </p>
    );
  }
  return <SummaryContent events={state.events} project={project} />;
}

function SummaryContent({
  events,
  project,
}: { events: AillyEvent[]; project: SourceValue<string> }) {
  const stats = useMemo(() => summarizeSession(events), [events]);

  return (
    <div className="flex flex-col gap-4 px-6 py-4">
      <div className="grid grid-cols-2 divide-x divide-y divide-border rounded-md border sm:grid-cols-4">
        <StatTile label="Tool calls" value={String(stats.toolCallCount)} />
        <StatTile label="Files touched" value={String(stats.filesTouchedCount)} />
        <StatTile label="Duration" value={recordedLabel(stats.duration, (value) => value)} />
        <StatTile label="Subagent spawns" value={recordedLabel(stats.subagentSpawnCount, String)} />
      </div>

      <fieldset
        aria-label="Working directory"
        className="flex items-baseline gap-2 rounded-md border px-3 py-2"
      >
        <span className="eyebrow-sm shrink-0 text-foreground-muted">Working directory</span>
        <span className="min-w-0 truncate font-mono text-foreground text-xs">
          {recordedLabel(project, (value) => value)}
        </span>
      </fieldset>

      {stats.toolCallCount === 0 ? (
        <p className="text-foreground-muted">This session recorded no tool calls.</p>
      ) : (
        <>
          <CategorySplit stats={stats} />
          <CallsByTool tools={stats.toolsByFrequency} />
          <Sources groups={stats.sources} project={project} />
        </>
      )}
    </div>
  );
}

function CategorySplit({ stats }: { stats: SessionSummaryStats }) {
  return (
    <Section label="Tool category split">
      <div className="flex flex-wrap gap-1.5">
        {stats.categories.map((total) => (
          <Badge key={total.category} color={CATEGORY_COLOR[total.category]} textSize="sm">
            {CATEGORY_LABEL[total.category]} — {total.share}%
          </Badge>
        ))}
        {stats.unclassified.count === 0 ? null : (
          <Badge color={CATEGORY_COLOR.unclassified} textSize="sm">
            {CATEGORY_LABEL.unclassified} — {stats.unclassified.share}%
          </Badge>
        )}
      </div>
    </Section>
  );
}

function CallsByTool({ tools }: { tools: ToolFrequency[] }) {
  const shown = tools.slice(0, LIST_CAP);
  return (
    <div className="flex flex-col gap-1.5">
      <SectionHeading>Calls by tool</SectionHeading>
      <ul aria-label="Calls by tool" className="rounded-md border">
        {shown.map((tool) => (
          <li
            key={tool.name}
            className="flex items-center gap-2 border-b px-3 py-1.5 last:border-b-0"
          >
            <span className="min-w-0 truncate font-medium text-foreground-title">{tool.name}</span>
            <span className="shrink-0 text-foreground-muted text-xs">
              {CATEGORY_LABEL[tool.category]}
            </span>
            <span className="ml-auto shrink-0 text-foreground-muted text-xs">
              {tool.count} calls
            </span>
            <Badge color={CATEGORY_COLOR[tool.category]} textSize="sm" className="shrink-0">
              {tool.share}%
            </Badge>
          </li>
        ))}
      </ul>
      <MoreRow hidden={tools.length - shown.length} noun="tool" />
    </div>
  );
}

function Sources({ groups, project }: { groups: SourceGroup[]; project: SourceValue<string> }) {
  const sessionCwd = isRecorded(project) ? project.Recorded : null;
  return (
    <Section label="Sources">
      <div className="rounded-md border">
        {groups.map((group) => (
          <SourceGroupRow key={group.kind} group={group} sessionCwd={sessionCwd} />
        ))}
      </div>
    </Section>
  );
}

function SourceGroupRow({ group, sessionCwd }: { group: SourceGroup; sessionCwd: string | null }) {
  const [open, setOpen] = useState(false);
  const countLabel =
    group.kind === "file"
      ? `${group.count} ${group.count === 1 ? "file" : "files"}`
      : `${group.count} calls`;
  return (
    <fieldset aria-label={group.label} className="border-b last:border-b-0">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-background-hover-solid"
      >
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <span className="font-medium text-foreground-title">{group.label}</span>
        <span className="ml-auto shrink-0 text-foreground-muted text-xs">{countLabel}</span>
      </button>
      {open ? (
        <div className="border-t px-3 py-2 pl-8">
          {group.kind === "file" ? (
            <FileAccessList files={group.files} />
          ) : (
            <CallList group={group} sessionCwd={sessionCwd} />
          )}
        </div>
      ) : null}
    </fieldset>
  );
}

function CallList({ group, sessionCwd }: { group: SourceGroup; sessionCwd: string | null }) {
  return (
    <ul className="flex min-w-0 flex-col gap-1.5">
      {group.calls.map((call) => (
        <li key={call.eventId} className="min-w-0">
          <CallRow call={call} sessionCwd={sessionCwd} />
        </li>
      ))}
    </ul>
  );
}

/** One call, expanding to what it captured. */
function CallRow({ call, sessionCwd }: { call: SourceCall; sessionCwd: string | null }) {
  const [open, setOpen] = useState(false);
  // Shell and web rows carry a command or URL as detail, not a file path; the
  // absolute-path rule belongs to conversation / file detail. Here cwd only
  // appears when it differs from the session's.
  const showCwd = shouldShowToolCwd({ cwd: call.cwd, path: null, sessionCwd });
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
        <div className="min-w-0 w-full pt-1 pl-[18px]">
          <CapturedOutput output={call.output} isError={call.outputIsError} />
        </div>
      ) : null}
    </div>
  );
}

function FileAccessList({ files }: { files: FileTouch[] }) {
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

function Section({ label, children }: { label: string; children: ReactNode }) {
  return (
    <fieldset aria-label={label} className="flex flex-col gap-1.5">
      <SectionHeading>{label}</SectionHeading>
      {children}
    </fieldset>
  );
}

function SectionHeading({ children }: { children: ReactNode }) {
  return <h2 className="eyebrow-sm text-foreground-muted">{children}</h2>;
}

function StatTile({ label, value }: { label: string; value: string }) {
  return (
    <fieldset aria-label={label} className="flex flex-col gap-0.5 px-3 py-2">
      <span className="font-semibold text-2xl text-foreground">{value}</span>
      <span className="eyebrow-sm text-foreground-muted">{label}</span>
    </fieldset>
  );
}

function MoreRow({ hidden, noun }: { hidden: number; noun: string }) {
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

/** Renders a recorded value, or the explicit label the product owes the user. */
function recordedLabel<T>(value: SourceValue<T>, render: (value: T) => string): string {
  return isRecorded(value) ? render(value.Recorded) : "Not recorded";
}

function Placeholder({ title, hint }: { title: string; hint: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-16 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-background-active text-foreground-muted">
        <BarChart3 size={26} strokeWidth={1.8} />
      </div>
      <h2 className="font-semibold text-foreground-title">{title}</h2>
      <p className="max-w-md text-foreground-muted leading-6">{hint}</p>
    </div>
  );
}
