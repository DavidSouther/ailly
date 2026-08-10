/**
 * (c) Copyright 2026 Nominal Inc. All rights reserved.
 */

import type { Meta } from "@storybook/react";
import {
  BrainIcon,
  CheckCircle2Icon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleIcon,
  FilePenLineIcon,
  Loader2Icon,
  SearchIcon,
  SquareTerminalIcon,
  TextIcon,
} from "lucide-react";
import type { ComponentType } from "react";
import React, { useState } from "react";

import { PanelAccordion, PanelAccordionItem } from "../ui/PanelAccordion";
import { BodyText, MutedText, StrongText } from "../ui/typography";
import { cn } from "../ui/utils";

/**
 * A refined sketch of the "Orchestrator TODO List" pattern from the Graph of
 * Actions design doc's "Where next?" section: the TODO list is the top-level
 * view, each task runs in an isolated subagent, and expanding a task reveals
 * that subagent's own interface -- thinking collapsed as a supporting
 * detail, tool calls collapsed for quick review rather than shown in flow.
 *
 * This uses the same "migrate the legacy auth service" scenario as the
 * Subagents sketch, but grouped by task instead of interleaved by
 * timestamp -- the same underlying work, reorganized to fix era 3's
 * attribution problem.
 */
const meta: Meta = {
  title: "Examples/Agent Harness Sketches/4. Orchestrator TODO List",
};

export default meta;

type TaskStatus = "done" | "active" | "pending";

type ToolName = "Grep" | "Read" | "Edit" | "Bash";

const TOOL_ICONS: Record<ToolName, ComponentType<{ className?: string }>> = {
  Grep: SearchIcon,
  Read: TextIcon,
  Edit: FilePenLineIcon,
  Bash: SquareTerminalIcon,
};

interface ToolCallEntry {
  id: string;
  tool: ToolName;
  summary: string;
  detail: string;
  status: "done" | "running";
}

interface Task {
  id: string;
  label: string;
  status: TaskStatus;
  thinking: string;
  toolCalls: ToolCallEntry[];
}

const TASKS: Task[] = [
  {
    id: "locate",
    label: "Locate call sites of LegacySession",
    status: "done",
    thinking:
      "Need to find every file referencing LegacySession before touching " +
      "the class itself, otherwise I'll break callers I didn't know " +
      "existed. Checking apps/scout first since that's where the session " +
      "hook lives, then the trpc routers that consume it server-side.",
    toolCalls: [
      {
        id: "locate-1",
        tool: "Grep",
        summary: 'rg -l "LegacySession" apps/scout',
        detail:
          "apps/scout/client/hooks/useSession.ts\n" +
          "apps/scout/client/providers/AuthProvider.tsx\n" +
          "apps/scout/server/middleware/session.ts\n" +
          "...11 more files",
        status: "done",
      },
      {
        id: "locate-2",
        tool: "Read",
        summary: "apps/scout/client/hooks/useSession.ts",
        detail:
          "const session = new LegacySession(token);\n" +
          "return { session, isExpired: session.isExpired() };",
        status: "done",
      },
      {
        id: "locate-3",
        tool: "Grep",
        summary: 'rg -l "LegacySession" packages/trpc',
        detail: "packages/trpc/src/routers/auth.ts\npackages/trpc/src/context.ts",
        status: "done",
      },
    ],
  },
  {
    id: "migrate",
    label: "Migrate session module & add tests",
    status: "active",
    thinking:
      "Renaming LegacySession to SessionStore. Keeping a static " +
      "fromToken() constructor so the call sites found in the previous " +
      "task can migrate one at a time instead of in one large edit. " +
      "Running the existing test suite before touching call sites, to " +
      "get a clean baseline.",
    toolCalls: [
      {
        id: "migrate-1",
        tool: "Read",
        summary: "packages/auth/src/session.ts",
        detail:
          "export class LegacySession {\n" +
          "  constructor(private readonly token: string) {}\n" +
          "  // ...42 lines\n" +
          "}",
        status: "done",
      },
      {
        id: "migrate-2",
        tool: "Edit",
        summary: "packages/auth/src/session.ts",
        detail:
          "- export class LegacySession {\n" +
          "+ export class SessionStore {\n" +
          "+   static fromToken(token: string): SessionStore { ... }",
        status: "done",
      },
      {
        id: "migrate-3",
        tool: "Bash",
        summary: "pnpm vitest packages/auth --run",
        detail: "Running... (started 4s ago)",
        status: "running",
      },
    ],
  },
  {
    id: "runbook",
    label: "Update the auth runbook",
    status: "pending",
    thinking: "",
    toolCalls: [],
  },
];

const STATUS_LABEL: Record<TaskStatus, string> = {
  done: "Done",
  active: "In progress",
  pending: "Not started",
};

function StatusMark({ status }: { status: TaskStatus }) {
  if (status === "done") {
    return <CheckCircle2Icon className="h-4 w-4 shrink-0 text-status-success" />;
  }
  if (status === "active") {
    return <Loader2Icon className="h-4 w-4 shrink-0 animate-spin text-foreground-muted" />;
  }
  return <CircleIcon className="h-4 w-4 shrink-0 text-foreground-muted" />;
}

function ThinkingDisclosure({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="pl-1">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex items-center gap-1.5 rounded px-1 py-0.5 text-xs text-foreground-muted hover:bg-background-hover hover:text-foreground"
      >
        {expanded ? (
          <ChevronDownIcon className="h-3 w-3" />
        ) : (
          <ChevronRightIcon className="h-3 w-3" />
        )}
        <BrainIcon className="h-3 w-3" />
        <span>{expanded ? "Hide thinking" : "Show thinking"}</span>
      </button>
      {expanded ? (
        <p className="mt-1 ml-5 max-w-md text-xs text-foreground-muted italic">{text}</p>
      ) : null}
    </div>
  );
}

function ToolCallList({ toolCalls }: { toolCalls: ToolCallEntry[] }) {
  return (
    <PanelAccordion className="mt-2 rounded-sm border">
      {toolCalls.map((entry) => {
        const Icon = TOOL_ICONS[entry.tool];
        return (
          <PanelAccordionItem
            key={entry.id}
            value={entry.id}
            label={`${entry.tool}: ${entry.summary}`}
            actions={
              entry.status === "running" ? (
                <Loader2Icon className="h-3 w-3 animate-spin text-foreground-muted" />
              ) : (
                <Icon className="h-3 w-3 text-foreground-muted" />
              )
            }
          >
            <pre className="overflow-x-auto p-2 font-mono text-[11px] whitespace-pre-wrap text-foreground-muted">
              {entry.detail}
            </pre>
          </PanelAccordionItem>
        );
      })}
    </PanelAccordion>
  );
}

function SubagentInterface({ task }: { task: Task }) {
  if (task.toolCalls.length === 0) {
    return <MutedText className="pl-1 text-xs">This subagent hasn&apos;t started yet.</MutedText>;
  }

  return (
    <div className="flex flex-col gap-2">
      {task.thinking ? <ThinkingDisclosure text={task.thinking} /> : null}
      <ToolCallList toolCalls={task.toolCalls} />
    </div>
  );
}

function TodoRow({ task }: { task: Task }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="border-b border-border last:border-b-0">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-center gap-2 px-3 py-2.5 text-left hover:bg-background-hover"
      >
        <StatusMark status={task.status} />
        <StrongText
          className={cn(
            "flex-1 text-sm",
            task.status === "done" && "text-foreground-muted line-through",
          )}
        >
          {task.label}
        </StrongText>
        <MutedText className="shrink-0 text-xs">{STATUS_LABEL[task.status]}</MutedText>
        {expanded ? (
          <ChevronDownIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        ) : (
          <ChevronRightIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        )}
      </button>
      {expanded ? (
        <div className="border-t border-border bg-background-hover/40 px-3 py-2 pl-9">
          <SubagentInterface task={task} />
        </div>
      ) : null}
    </div>
  );
}

export function Default() {
  return (
    <div className="mx-auto flex max-w-xl flex-col gap-3 bg-background p-6">
      <BodyText className="text-xs text-foreground-muted">
        Refined orchestrator view -- the TODO list is the top-level surface. Expand a task to drill
        into its subagent&apos;s own interface; thinking stays collapsed as a supporting detail, and
        tool calls stay collapsed for quick review instead of scrolling past in flow.
      </BodyText>
      <div className="rounded-md border">
        {TASKS.map((task) => (
          <TodoRow key={task.id} task={task} />
        ))}
      </div>
    </div>
  );
}
