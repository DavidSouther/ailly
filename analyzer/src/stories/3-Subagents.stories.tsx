/**
 * (c) Copyright 2026 Nominal Inc. All rights reserved.
 */

import { BadgeColor } from "../ui/colors";
import type { Meta } from "@storybook/react";
import {
  BotIcon,
  CheckIcon,
  FilePenLineIcon,
  Loader2Icon,
  SearchIcon,
  SquareTerminalIcon,
  TextIcon,
} from "lucide-react";
import type { ComponentType } from "react";
import React from "react";

import { Badge } from "../ui/badges/badge";
import { AvatarFallback, AvatarRoot } from "../ui/images";
import { PanelAccordion, PanelAccordionItem } from "../ui/PanelAccordion";

/**
 * A static sketch of the "subagents" era of agent harness UI (~2026): a
 * single orchestrator fans work out to background subagents, but the
 * transcript still renders everything as one linear stream. Tool calls and
 * their results are interleaved by the order they happened to arrive, not
 * grouped by which subagent produced them, and the only clue to attribution
 * is a small, easy-to-miss initial next to each block.
 *
 * This is intentionally hard to follow — that confusion is the point being
 * illustrated, not a bug in the sketch.
 */
const meta: Meta = {
  title: "Examples/Agent Harness Sketches/3. Subagents",
};

export default meta;

type AgentKey = "a1" | "a2" | "a3";

const AGENTS: Record<AgentKey, { initials: string }> = {
  a1: { initials: "1" },
  a2: { initials: "2" },
  a3: { initials: "3" },
};

type ToolName = "Grep" | "Read" | "Edit" | "Bash";

const TOOL_ICONS: Record<ToolName, ComponentType<{ className?: string }>> = {
  Grep: SearchIcon,
  Read: TextIcon,
  Edit: FilePenLineIcon,
  Bash: SquareTerminalIcon,
};

interface ToolItem {
  kind: "tool";
  time: string;
  agent: AgentKey;
  tool: ToolName;
  summary: string;
  status: "done" | "running";
  detail: string;
}

interface MessageItem {
  kind: "user" | "orchestrator";
  time: string;
  text: string;
}

type TranscriptItem = ToolItem | MessageItem;

const TRANSCRIPT: TranscriptItem[] = [
  {
    kind: "user",
    time: "10:41:02",
    text: "Migrate the legacy auth service to the new session store. Update the tests and the runbook while you're at it.",
  },
  {
    kind: "orchestrator",
    time: "10:41:04",
    text: "Spawning 3 subagents in parallel: locate call sites, migrate + test the session module, and update the runbook.",
  },
  {
    kind: "tool",
    time: "10:41:09",
    agent: "a1",
    tool: "Grep",
    summary: 'rg -l "LegacySession" apps/scout',
    status: "done",
    detail:
      "apps/scout/client/hooks/useSession.ts\napps/scout/client/providers/AuthProvider.tsx\napps/scout/server/middleware/session.ts\n...11 more files",
  },
  {
    kind: "tool",
    time: "10:41:10",
    agent: "a2",
    tool: "Read",
    summary: "packages/auth/src/session.ts",
    status: "done",
    detail:
      "export class LegacySession {\n  constructor(private readonly token: string) {}\n  // ...42 lines\n}",
  },
  {
    kind: "tool",
    time: "10:41:11",
    agent: "a3",
    tool: "Read",
    summary: "docs/auth/runbook.md",
    status: "done",
    detail:
      "# Auth Runbook\n\n## Rotating session secrets\n1. ...\n\n## Debugging a stuck login\n1. ...",
  },
  {
    kind: "tool",
    time: "10:41:15",
    agent: "a1",
    tool: "Read",
    summary: "apps/scout/client/hooks/useSession.ts",
    status: "done",
    detail:
      "const session = new LegacySession(token);\nreturn { session, isExpired: session.isExpired() };",
  },
  {
    kind: "tool",
    time: "10:41:22",
    agent: "a2",
    tool: "Edit",
    summary: "packages/auth/src/session.ts",
    status: "done",
    detail:
      "- export class LegacySession {\n+ export class SessionStore {\n+   static fromToken(token: string): SessionStore { ... }",
  },
  {
    kind: "tool",
    time: "10:41:26",
    agent: "a3",
    tool: "Edit",
    summary: "docs/auth/runbook.md",
    status: "running",
    detail: 'Drafting a new "Session store migration" section...',
  },
  {
    kind: "tool",
    time: "10:41:31",
    agent: "a1",
    tool: "Grep",
    summary: 'rg -l "LegacySession" packages/trpc',
    status: "done",
    detail: "packages/trpc/src/routers/auth.ts\npackages/trpc/src/context.ts",
  },
  {
    kind: "tool",
    time: "10:41:40",
    agent: "a2",
    tool: "Bash",
    summary: "pnpm vitest packages/auth --run",
    status: "done",
    detail: "Test Files  6 passed (6)\n     Tests  42 passed (42)",
  },
  {
    kind: "tool",
    time: "10:41:44",
    agent: "a1",
    tool: "Read",
    summary: "packages/trpc/src/routers/auth.ts",
    status: "done",
    detail:
      "import { LegacySession } from '@nominal-io/auth';\n// 2 more call sites below",
  },
  {
    kind: "orchestrator",
    time: "10:41:52",
    text: "All three tasks report complete.",
  },
];

function AgentMark({ agent }: { agent: AgentKey }) {
  return (
    <AvatarRoot size={16} border={false} className="mt-0.5 shrink-0">
      <AvatarFallback size={16} className="text-[8px]">
        {AGENTS[agent].initials}
      </AvatarFallback>
    </AvatarRoot>
  );
}

function ToolRow({ item }: { item: ToolItem }) {
  const Icon = TOOL_ICONS[item.tool];
  return (
    <div className="flex items-start gap-1.5">
      <AgentMark agent={item.agent} />
      <div className="min-w-0 flex-1">
        <PanelAccordion className="rounded-sm border">
          <PanelAccordionItem
            value={`${item.time}-${item.agent}`}
            label={`${item.tool}: ${item.summary}`}
            actions={
              item.status === "running" ? (
                <Loader2Icon className="h-3 w-3 animate-spin text-foreground-muted" />
              ) : (
                <Badge color={BadgeColor.METAL} textSize="sm" icon={CheckIcon}>
                  done
                </Badge>
              )
            }
          >
            <pre className="overflow-x-auto p-2 font-mono text-[11px] whitespace-pre-wrap text-foreground-muted">
              {item.detail}
            </pre>
          </PanelAccordionItem>
        </PanelAccordion>
      </div>
      <Icon className="mt-1 h-3 w-3 shrink-0 text-foreground-muted" />
      <span className="mt-1 shrink-0 text-[10px] text-foreground-muted/60">
        {item.time}
      </span>
    </div>
  );
}

function MessageRow({ item }: { item: MessageItem }) {
  if (item.kind === "user") {
    return (
      <div className="rounded-md bg-background-active px-3 py-2 text-sm text-foreground">
        {item.text}
      </div>
    );
  }
  return (
    <div className="flex items-start gap-1.5 text-sm text-foreground">
      <BotIcon className="mt-0.5 h-3.5 w-3.5 shrink-0 text-foreground-muted" />
      <p className="m-0">{item.text}</p>
    </div>
  );
}

export function Default() {
  return (
    <div className="flex max-w-xl flex-col gap-3 bg-background p-6">
      <div className="text-xs text-foreground-muted">
        Era 3 — subagents run concurrently, but their tool calls still render
        inline, one after another, in a single transcript. Try tracing which
        agent did what.
      </div>
      {TRANSCRIPT.map((item, index) =>
        item.kind === "tool" ? (
          <ToolRow key={`${item.time}-${item.agent}-${index}`} item={item} />
        ) : (
          <MessageRow key={`${item.time}-${item.kind}-${index}`} item={item} />
        ),
      )}
    </div>
  );
}
