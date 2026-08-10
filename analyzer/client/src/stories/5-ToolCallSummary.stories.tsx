/**
 * (c) Copyright 2026 Nominal Inc. All rights reserved.
 */

import type { Meta } from "@storybook/react";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  FilePenLineIcon,
  FilePlusIcon,
  FileTextIcon,
  GlobeIcon,
  LibraryIcon,
  ListChecksIcon,
  MessageCircleQuestionIcon,
  SparklesIcon,
  SquareTerminalIcon,
  TextIcon,
  UsersIcon,
} from "lucide-react";
import type React from "react";
import { useState } from "react";
import { BadgeColor } from "../ui/colors";

import { PanelAccordion, PanelAccordionItem } from "../ui/PanelAccordion";
import { Badge } from "../ui/badges/badge";
import type { IconType } from "../ui/icons";
import { BodyText, MutedText, StrongText } from "../ui/typography";

/**
 * A static sketch of the "post-completion Tool Call summary & drill-down"
 * pattern from the Graph of Actions design doc's third reasoning point: once
 * an orchestrator and its subagents finish a session, a rolled-up view shows
 * which tools ran, how often, and which files/skills/APIs they touched, with
 * a way to drill into individual calls to see exactly where a fact entered
 * the context window.
 *
 * This is deliberately a different view from the Orchestrator TODO List
 * sketch: that one is the in-progress, per-task surface shown *during* a
 * session; this one is the after-the-fact audit surface shown *after* one,
 * organized by tool and by file rather than by task. Both use the same
 * "migrate the legacy auth service" scenario for continuity.
 */
const meta: Meta = {
  title: "Examples/Agent Harness Sketches/5. Tool Call Summary",
};

export default meta;

type ToolName =
  | "Bash"
  | "Edit"
  | "Read"
  | "Write"
  | "Agent"
  | "TodoWrite"
  | "Skill"
  | "AskUserQuestion"
  | "WebFetch";

type ToolCategory = "exec" | "edit" | "read" | "other";

const CATEGORY_ORDER: ToolCategory[] = ["exec", "edit", "read", "other"];

const CATEGORY_LABEL: Record<ToolCategory, string> = {
  exec: "Exec / shell",
  edit: "Edit / write",
  read: "Read",
  other: "Other",
};

const CATEGORY_COLOR: Record<ToolCategory, BadgeColor> = {
  exec: BadgeColor.AMBER,
  edit: BadgeColor.SKY,
  read: BadgeColor.MINT,
  other: BadgeColor.PLUM,
};

const TOOL_META: Record<ToolName, { icon: IconType; category: ToolCategory }> = {
  Bash: { icon: SquareTerminalIcon, category: "exec" },
  Edit: { icon: FilePenLineIcon, category: "edit" },
  Write: { icon: FilePlusIcon, category: "edit" },
  Read: { icon: TextIcon, category: "read" },
  Agent: { icon: UsersIcon, category: "other" },
  TodoWrite: { icon: ListChecksIcon, category: "other" },
  Skill: { icon: SparklesIcon, category: "other" },
  AskUserQuestion: { icon: MessageCircleQuestionIcon, category: "other" },
  WebFetch: { icon: GlobeIcon, category: "other" },
};

interface ToolStat {
  name: ToolName;
  count: number;
}

interface FileTouch {
  path: string;
  touches: number;
  tools: ToolName[];
}

interface SubagentTypeStat {
  type: string;
  count: number;
}

interface SessionStats {
  scenario: string;
  totalCalls: number;
  totalDistinctFiles: number;
  durationLabel: string;
  subagentSpawns: number;
  subagentTypes: SubagentTypeStat[];
  tools: ToolStat[];
  /** Top files by touch count -- may be fewer than totalDistinctFiles. */
  files: FileTouch[];
}

const TYPICAL_SESSION: SessionStats = {
  scenario:
    "Migrate the legacy auth service to the new session store, update the tests, and refresh the runbook.",
  totalCalls: 62,
  totalDistinctFiles: 11,
  durationLabel: "18m 42s",
  subagentSpawns: 3,
  subagentTypes: [
    { type: "general-purpose", count: 2 },
    { type: "code-reviewer", count: 1 },
  ],
  tools: [
    { name: "Bash", count: 31 },
    { name: "Edit", count: 11 },
    { name: "Read", count: 9 },
    { name: "Write", count: 4 },
    { name: "Agent", count: 3 },
    { name: "TodoWrite", count: 1 },
    { name: "Skill", count: 1 },
    { name: "AskUserQuestion", count: 1 },
    { name: "WebFetch", count: 1 },
  ],
  files: [
    {
      path: "packages/auth/src/session.ts",
      touches: 9,
      tools: ["Read", "Edit"],
    },
    {
      path: "packages/auth/src/session.test.ts",
      touches: 6,
      tools: ["Write", "Bash"],
    },
    {
      path: "apps/scout/client/hooks/useSession.ts",
      touches: 5,
      tools: ["Read", "Edit"],
    },
    {
      path: "apps/scout/client/hooks/useSession.test.ts",
      touches: 4,
      tools: ["Write", "Bash"],
    },
    {
      path: "apps/scout/client/providers/AuthProvider.tsx",
      touches: 4,
      tools: ["Read", "Edit"],
    },
    {
      path: "apps/scout/server/middleware/session.ts",
      touches: 4,
      tools: ["Read", "Edit"],
    },
    {
      path: "packages/trpc/src/routers/auth.ts",
      touches: 3,
      tools: ["Read", "Edit"],
    },
    { path: "packages/trpc/src/context.ts", touches: 3, tools: ["Read"] },
    { path: "docs/auth/runbook.md", touches: 3, tools: ["Read", "Write"] },
    { path: ".github/workflows/ci.yml", touches: 1, tools: ["Read"] },
    { path: "scripts/migrate-sessions.sh", touches: 1, tools: ["Write"] },
  ],
};

const LARGE_SESSION: SessionStats = {
  scenario:
    "The same auth migration, run long: a broader call-site sweep, a full test-suite rewrite, and a runbook overhaul, with a code-reviewer and a test-runner subagent added to the mix.",
  totalCalls: 268,
  totalDistinctFiles: 47,
  durationLabel: "1h 47m",
  subagentSpawns: 9,
  subagentTypes: [
    { type: "general-purpose", count: 7 },
    { type: "code-reviewer", count: 1 },
    { type: "test-runner", count: 1 },
  ],
  tools: [
    { name: "Bash", count: 135 },
    { name: "Edit", count: 47 },
    { name: "Read", count: 38 },
    { name: "Write", count: 17 },
    { name: "Agent", count: 9 },
    { name: "TodoWrite", count: 7 },
    { name: "WebFetch", count: 6 },
    { name: "Skill", count: 5 },
    { name: "AskUserQuestion", count: 4 },
  ],
  files: [
    {
      path: "packages/auth/src/session.ts",
      touches: 22,
      tools: ["Read", "Edit"],
    },
    {
      path: "packages/auth/src/session.test.ts",
      touches: 14,
      tools: ["Write", "Bash"],
    },
    {
      path: "apps/scout/client/hooks/useSession.ts",
      touches: 13,
      tools: ["Read", "Edit"],
    },
    {
      path: "apps/scout/client/hooks/useSession.test.ts",
      touches: 11,
      tools: ["Write", "Bash"],
    },
    {
      path: "apps/scout/client/providers/AuthProvider.tsx",
      touches: 10,
      tools: ["Read", "Edit"],
    },
    {
      path: "apps/scout/server/middleware/session.ts",
      touches: 9,
      tools: ["Read", "Edit"],
    },
    {
      path: "packages/trpc/src/routers/auth.ts",
      touches: 8,
      tools: ["Read", "Edit"],
    },
    { path: "packages/trpc/src/context.ts", touches: 7, tools: ["Read"] },
    { path: "docs/auth/runbook.md", touches: 7, tools: ["Read", "Write"] },
    { path: "packages/auth/src/index.ts", touches: 5, tools: ["Read", "Edit"] },
    { path: ".github/workflows/ci.yml", touches: 3, tools: ["Read"] },
    {
      path: "scripts/migrate-sessions.sh",
      touches: 3,
      tools: ["Write", "Bash"],
    },
  ],
};

function SectionHeading({ children }: { children: React.ReactNode }) {
  return (
    <MutedText className="text-xs font-medium tracking-widest uppercase">{children}</MutedText>
  );
}

function StatTile({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0.5 px-3 py-2">
      <span className="text-2xl font-semibold text-foreground">{value}</span>
      <MutedText className="text-xs tracking-wide uppercase">{label}</MutedText>
    </div>
  );
}

function CategorySummary({
  tools,
  totalCalls,
}: {
  tools: ToolStat[];
  totalCalls: number;
}) {
  const totals = new Map<ToolCategory, number>();
  for (const tool of tools) {
    const category = TOOL_META[tool.name].category;
    totals.set(category, (totals.get(category) ?? 0) + tool.count);
  }

  return (
    <div className="flex flex-wrap gap-1.5">
      {CATEGORY_ORDER.map((category) => {
        const count = totals.get(category) ?? 0;
        if (count === 0) {
          return null;
        }
        const pct = Math.round((count / totalCalls) * 100);
        return (
          <Badge key={category} color={CATEGORY_COLOR[category]} textSize="sm">
            {CATEGORY_LABEL[category]} — {pct}%
          </Badge>
        );
      })}
    </div>
  );
}

function ToolBreakdownRow({
  tool,
  totalCalls,
}: {
  tool: ToolStat;
  totalCalls: number;
}) {
  const meta = TOOL_META[tool.name];
  const Icon = meta.icon;
  const pct = Math.round((tool.count / totalCalls) * 100);

  return (
    <div className="flex items-center gap-2 border-b border-border px-3 py-1.5 last:border-b-0">
      <Icon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
      <StrongText className="text-sm">{tool.name}</StrongText>
      <MutedText className="text-xs">{CATEGORY_LABEL[meta.category]}</MutedText>
      <span className="ml-auto shrink-0 text-xs text-foreground-muted">{tool.count} calls</span>
      <Badge color={CATEGORY_COLOR[meta.category]} textSize="sm" className="shrink-0">
        {pct}%
      </Badge>
    </div>
  );
}

function ToolBreakdownList({
  tools,
  totalCalls,
}: {
  tools: ToolStat[];
  totalCalls: number;
}) {
  const sorted = [...tools].sort((a, b) => b.count - a.count);
  return (
    <div className="rounded-md border">
      {sorted.map((tool) => (
        <ToolBreakdownRow key={tool.name} tool={tool} totalCalls={totalCalls} />
      ))}
    </div>
  );
}

function FileTouchRow({ file }: { file: FileTouch }) {
  const dot = file.path.lastIndexOf(".");
  const ext = dot >= 0 ? file.path.slice(dot) : "";

  return (
    <div className="flex items-center gap-2 border-b border-border px-3 py-1.5 last:border-b-0">
      <FileTextIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
      <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground">{file.path}</span>
      {ext ? (
        <Badge color={BadgeColor.METAL} outlined={true} textSize="sm" className="shrink-0">
          {ext}
        </Badge>
      ) : null}
      <div className="flex shrink-0 gap-1">
        {file.tools.map((tool) => (
          <Badge key={tool} color={BadgeColor.METAL} textSize="sm">
            {tool}
          </Badge>
        ))}
      </div>
      <span className="w-16 shrink-0 text-right text-xs text-foreground-muted">
        {file.touches} {file.touches === 1 ? "touch" : "touches"}
      </span>
    </div>
  );
}

function FilesTouchedList({
  files,
  totalDistinctFiles,
}: {
  files: FileTouch[];
  totalDistinctFiles: number;
}) {
  const hidden = totalDistinctFiles - files.length;
  return (
    <div className="rounded-md border">
      {files.map((file) => (
        <FileTouchRow key={file.path} file={file} />
      ))}
      {hidden > 0 ? (
        <div className="px-3 py-1.5 text-xs text-foreground-muted">
          +{hidden} more file{hidden === 1 ? "" : "s"} touched
        </div>
      ) : null}
    </div>
  );
}

function SubagentBreakdown({
  spawns,
  types,
}: {
  spawns: number;
  types: SubagentTypeStat[];
}) {
  if (spawns === 0) {
    return <MutedText className="text-xs">No subagents were spawned this session.</MutedText>;
  }

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <MutedText className="text-xs">
        {spawns} subagent {spawns === 1 ? "spawn" : "spawns"}:
      </MutedText>
      {types.map((type) => (
        <Badge key={type.type} color={BadgeColor.METAL} textSize="sm" icon={UsersIcon}>
          {type.type} × {type.count}
        </Badge>
      ))}
    </div>
  );
}

function ToolCallSummaryView({ stats }: { stats: SessionStats }) {
  return (
    <div className="flex flex-col gap-4">
      <div>
        <SectionHeading>Session</SectionHeading>
        <BodyText className="mt-1 text-sm">{stats.scenario}</BodyText>
      </div>
      <div className="grid grid-cols-2 divide-x divide-y divide-border rounded-md border sm:grid-cols-4">
        <StatTile label="Tool calls" value={stats.totalCalls.toLocaleString()} />
        <StatTile label="Files touched" value={stats.totalDistinctFiles.toLocaleString()} />
        <StatTile label="Duration" value={stats.durationLabel} />
        <StatTile label="Subagent spawns" value={stats.subagentSpawns.toLocaleString()} />
      </div>
      <div className="flex flex-col gap-1.5">
        <SectionHeading>Tool category split</SectionHeading>
        <CategorySummary tools={stats.tools} totalCalls={stats.totalCalls} />
      </div>
      <div className="flex flex-col gap-1.5">
        <SectionHeading>Calls by tool</SectionHeading>
        <ToolBreakdownList tools={stats.tools} totalCalls={stats.totalCalls} />
      </div>
      <div className="flex flex-col gap-1.5">
        <SectionHeading>Files touched</SectionHeading>
        <FilesTouchedList files={stats.files} totalDistinctFiles={stats.totalDistinctFiles} />
      </div>
      <div className="flex flex-col gap-1.5">
        <SectionHeading>Subagents</SectionHeading>
        <SubagentBreakdown spawns={stats.subagentSpawns} types={stats.subagentTypes} />
      </div>
    </div>
  );
}

export function Default() {
  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 bg-background p-6">
      <BodyText className="text-xs text-foreground-muted">
        Post-session Tool Call Summary -- after the orchestrator and its subagents finish, this
        rolls every tool call from the whole session up into one auditable view: which tools ran,
        how often, and which files they touched. This is the after-the-fact companion to the
        Orchestrator TODO List&apos;s in-progress view, not a replacement for it.
      </BodyText>
      <ToolCallSummaryView stats={TYPICAL_SESSION} />
    </div>
  );
}

export function LargeSession() {
  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-4 bg-background p-6">
      <BodyText className="text-xs text-foreground-muted">
        The same summary view under a long, tool-heavy session — 268 calls across 47 files and 9
        subagent spawns — to check that the rollup still reads cleanly at scale instead of only
        working for toy examples.
      </BodyText>
      <ToolCallSummaryView stats={LARGE_SESSION} />
    </div>
  );
}

type TargetKind = "file" | "api" | "skill" | "subagent" | "other";

const TARGET_KIND_ICON: Record<TargetKind, IconType> = {
  file: FileTextIcon,
  api: GlobeIcon,
  skill: SparklesIcon,
  subagent: UsersIcon,
  other: ListChecksIcon,
};

const TARGET_KIND_LABEL: Record<TargetKind, string> = {
  file: "File",
  api: "API call",
  skill: "Skill",
  subagent: "Subagent spawn",
  other: "Other",
};

const TARGET_KIND_COLOR: Record<TargetKind, BadgeColor> = {
  file: BadgeColor.MINT,
  api: BadgeColor.PLUM,
  skill: BadgeColor.AMBER,
  subagent: BadgeColor.METAL_DARK,
  other: BadgeColor.METAL,
};

interface CallDetail {
  id: string;
  agent: string;
  target: string;
  targetKind: TargetKind;
  detail: string;
}

interface ToolGroup {
  tool: ToolName;
  count: number;
  /** Whether the doc's "especially pertinent" callout applies to this tool. */
  notable: boolean;
  sample: CallDetail[];
}

type SourceKind = "shell" | "file" | "web";

interface SourceTypeGroup {
  kind: SourceKind;
  label: string;
  tool: ToolName;
  count: number;
  notable: boolean;
  sample: CallDetail[];
}

/**
 * Bash, Read, and WebFetch are all ways facts enter the context window from
 * outside the conversation -- shell output, file contents, and remote
 * responses, respectively. Grouped under one "Sources" umbrella so an
 * operator can audit provenance in one place, then drill into a specific
 * kind (and, for file reads, the exact files consulted).
 */
const SOURCE_GROUPS: SourceTypeGroup[] = [
  {
    kind: "shell",
    label: "Shell output",
    tool: "Bash",
    count: 31,
    notable: false,
    sample: [
      {
        id: "bash-1",
        agent: "Subagent 2",
        target: "pnpm vitest packages/auth --run",
        targetKind: "other",
        detail: "Test Files  6 passed (6)\n     Tests  42 passed (42)",
      },
      {
        id: "bash-2",
        agent: "Subagent 1",
        target: 'rg -l "LegacySession" apps/scout',
        targetKind: "other",
        detail: "11 matches across apps/scout/client and apps/scout/server.",
      },
      {
        id: "bash-3",
        agent: "Orchestrator",
        target: "git diff --stat",
        targetKind: "other",
        detail: "7 files changed, 96 insertions(+), 41 deletions(-)",
      },
      {
        id: "bash-4",
        agent: "Subagent 2",
        target: "pnpm typecheck --filter=@nominal-io/auth",
        targetKind: "other",
        detail: "No errors found.",
      },
    ],
  },
  {
    kind: "file",
    label: "File reads",
    tool: "Read",
    count: 9,
    notable: true,
    sample: [
      {
        id: "read-1",
        agent: "Subagent 2",
        target: "packages/auth/src/session.ts",
        targetKind: "file",
        detail:
          "Read the existing LegacySession class before renaming it — this is where the constructor signature entered context.",
      },
      {
        id: "read-2",
        agent: "Subagent 1",
        target: "apps/scout/client/hooks/useSession.ts",
        targetKind: "file",
        detail: "Read to find the exact call shape that needed to change.",
      },
      {
        id: "read-3",
        agent: "Subagent 3",
        target: "docs/auth/runbook.md",
        targetKind: "file",
        detail: "Read the existing runbook structure before drafting the new section.",
      },
      {
        id: "read-4",
        agent: "Orchestrator",
        target: "packages/trpc/src/routers/auth.ts",
        targetKind: "file",
        detail: "Read to confirm no other call sites were missed by the search.",
      },
    ],
  },
  {
    kind: "web",
    label: "Web / API",
    tool: "WebFetch",
    count: 1,
    notable: true,
    sample: [
      {
        id: "fetch-1",
        agent: "Subagent 2",
        target: "https://api.nominal.io/openapi/auth-session.json",
        targetKind: "api",
        detail:
          "Fetched the session-store OpenAPI schema to confirm the new /v2/sessions response shape — this is where the new field names entered context.",
      },
    ],
  },
];

const SOURCES_TOTAL = SOURCE_GROUPS.reduce((sum, group) => sum + group.count, 0);
const SOURCES_NOTABLE = SOURCE_GROUPS.some((group) => group.notable);

const TOOL_GROUPS: ToolGroup[] = [
  {
    tool: "Edit",
    count: 11,
    notable: false,
    sample: [
      {
        id: "edit-1",
        agent: "Subagent 2",
        target: "packages/auth/src/session.ts",
        targetKind: "file",
        detail: "Renamed LegacySession to SessionStore and added a static fromToken() constructor.",
      },
      {
        id: "edit-2",
        agent: "Subagent 1",
        target: "apps/scout/client/hooks/useSession.ts",
        targetKind: "file",
        detail: "Swapped the LegacySession import for SessionStore.fromToken().",
      },
      {
        id: "edit-3",
        agent: "Subagent 2",
        target: "apps/scout/server/middleware/session.ts",
        targetKind: "file",
        detail: "Updated the constructor call and narrowed the return type.",
      },
      {
        id: "edit-4",
        agent: "Subagent 3",
        target: "docs/auth/runbook.md",
        targetKind: "file",
        detail: 'Added a "Session store migration" section.',
      },
    ],
  },
  {
    tool: "Write",
    count: 4,
    notable: false,
    sample: [
      {
        id: "write-1",
        agent: "Subagent 2",
        target: "packages/auth/src/session.test.ts",
        targetKind: "file",
        detail: "New test file covering SessionStore.fromToken() and expiry handling.",
      },
      {
        id: "write-2",
        agent: "Subagent 1",
        target: "apps/scout/client/hooks/useSession.test.ts",
        targetKind: "file",
        detail: "New test file covering the updated hook.",
      },
      {
        id: "write-3",
        agent: "Subagent 3",
        target: "scripts/migrate-sessions.sh",
        targetKind: "file",
        detail: "One-off script to re-encode sessions stored under the legacy key format.",
      },
    ],
  },
  {
    tool: "Agent",
    count: 3,
    notable: false,
    sample: [
      {
        id: "agent-1",
        agent: "Orchestrator",
        target: "Subagent 1 (general-purpose)",
        targetKind: "subagent",
        detail:
          'Task: "Locate every call site of LegacySession across apps/scout and packages/trpc."',
      },
      {
        id: "agent-2",
        agent: "Orchestrator",
        target: "Subagent 2 (general-purpose)",
        targetKind: "subagent",
        detail: 'Task: "Migrate packages/auth/src/session.ts to SessionStore and add tests."',
      },
      {
        id: "agent-3",
        agent: "Orchestrator",
        target: "Subagent 3 (code-reviewer)",
        targetKind: "subagent",
        detail: 'Task: "Review the session-store migration diff for backward-compatibility risk."',
      },
    ],
  },
  {
    tool: "TodoWrite",
    count: 1,
    notable: false,
    sample: [
      {
        id: "todo-1",
        agent: "Orchestrator",
        target: "Updated plan: 3/3 tasks complete",
        targetKind: "other",
        detail:
          "Marked all three delegated tasks done after the code-reviewer subagent reported no blocking issues.",
      },
    ],
  },
  {
    tool: "Skill",
    count: 1,
    notable: true,
    sample: [
      {
        id: "skill-1",
        agent: "Subagent 2",
        target: "developer:red-green-refactor",
        targetKind: "skill",
        detail:
          "Invoked to drive the SessionStore migration through a type-first TDD loop — this is where the test-first approach entered context.",
      },
    ],
  },
  {
    tool: "AskUserQuestion",
    count: 1,
    notable: false,
    sample: [
      {
        id: "ask-1",
        agent: "Orchestrator",
        target: "Keep LegacySession as a deprecated alias, or remove it outright?",
        targetKind: "other",
        detail: "User selected: keep it as a deprecated alias for one release.",
      },
    ],
  },
];

function CallAccordionItem({ call }: { call: CallDetail }) {
  const Icon = TARGET_KIND_ICON[call.targetKind];
  return (
    <PanelAccordionItem
      value={call.id}
      label={`${call.agent}: ${call.target}`}
      actions={
        <Badge color={TARGET_KIND_COLOR[call.targetKind]} textSize="sm" icon={Icon}>
          {TARGET_KIND_LABEL[call.targetKind]}
        </Badge>
      }
    >
      <p className="p-2 text-xs text-foreground-muted">{call.detail}</p>
    </PanelAccordionItem>
  );
}

function ToolGroupRow({ group }: { group: ToolGroup }) {
  const [expanded, setExpanded] = useState(false);
  const meta = TOOL_META[group.tool];
  const Icon = meta.icon;
  const hidden = group.count - group.sample.length;

  return (
    <div className="border-b border-border last:border-b-0">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-center gap-2 px-3 py-2.5 text-left hover:bg-background-hover"
      >
        <Icon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        <StrongText className="text-sm">{group.tool}</StrongText>
        {group.notable ? (
          <Badge color={BadgeColor.METAL_DARK} outlined={true} textSize="sm" className="shrink-0">
            Notable
          </Badge>
        ) : null}
        <span className="ml-auto shrink-0 text-xs text-foreground-muted">{group.count} calls</span>
        {expanded ? (
          <ChevronDownIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        ) : (
          <ChevronRightIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        )}
      </button>
      {expanded ? (
        <div className="border-t border-border bg-background-hover/40 px-3 py-2 pl-8">
          <PanelAccordion className="rounded-sm border">
            {group.sample.map((call) => (
              <CallAccordionItem key={call.id} call={call} />
            ))}
          </PanelAccordion>
          {hidden > 0 ? (
            <MutedText className="mt-1.5 text-xs">
              +{hidden} more {group.tool} call{hidden === 1 ? "" : "s"} not shown
            </MutedText>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

const SOURCE_HIDDEN_LABEL: Record<SourceKind, string> = {
  shell: "shell call",
  file: "file",
  web: "web / API call",
};

function SourceTypeRow({ group }: { group: SourceTypeGroup }) {
  const [expanded, setExpanded] = useState(false);
  const Icon = TOOL_META[group.tool].icon;
  const hidden = group.count - group.sample.length;
  const hiddenLabel = SOURCE_HIDDEN_LABEL[group.kind];

  return (
    <div className="border-b border-border last:border-b-0">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-background-hover"
      >
        <Icon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        <StrongText className="text-sm">{group.label}</StrongText>
        {group.notable ? (
          <Badge color={BadgeColor.METAL_DARK} outlined={true} textSize="sm" className="shrink-0">
            Notable
          </Badge>
        ) : null}
        <span className="ml-auto shrink-0 text-xs text-foreground-muted">{group.count} calls</span>
        {expanded ? (
          <ChevronDownIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        ) : (
          <ChevronRightIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        )}
      </button>
      {expanded ? (
        <div className="border-t border-border bg-background-hover/40 px-3 py-2 pl-8">
          <PanelAccordion className="rounded-sm border">
            {group.sample.map((call) => (
              <CallAccordionItem key={call.id} call={call} />
            ))}
          </PanelAccordion>
          {hidden > 0 ? (
            <MutedText className="mt-1.5 text-xs">
              +{hidden} more {hiddenLabel}
              {hidden === 1 ? "" : "s"} not shown
            </MutedText>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function SourcesRow() {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="border-b border-border last:border-b-0">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex w-full items-center gap-2 px-3 py-2.5 text-left hover:bg-background-hover"
      >
        <LibraryIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        <StrongText className="text-sm">Sources</StrongText>
        {SOURCES_NOTABLE ? (
          <Badge color={BadgeColor.METAL_DARK} outlined={true} textSize="sm" className="shrink-0">
            Notable
          </Badge>
        ) : null}
        <span className="ml-auto shrink-0 text-xs text-foreground-muted">
          {SOURCES_TOTAL} calls
        </span>
        {expanded ? (
          <ChevronDownIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        ) : (
          <ChevronRightIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        )}
      </button>
      {expanded ? (
        <div className="border-t border-border bg-background-hover/40 pl-4">
          <div className="rounded-sm border">
            {SOURCE_GROUPS.map((group) => (
              <SourceTypeRow key={group.kind} group={group} />
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

export function ByTool() {
  return (
    <div className="mx-auto flex max-w-xl flex-col gap-3 bg-background p-6">
      <BodyText className="text-xs text-foreground-muted">
        The same session, regrouped by tool instead of by task — the drill-down the design doc calls
        out specifically: which tools were called with what parameters. Bash, Read, and WebFetch are
        rolled into a single <StrongText className="text-xs">Sources</StrongText> group — every way
        a fact enters context from outside the conversation. Expand it for the breakdown by type,
        then expand a type (e.g. File reads) to see exactly which files were consulted.
      </BodyText>
      <div className="rounded-md border">
        <SourcesRow />
        {TOOL_GROUPS.map((group) => (
          <ToolGroupRow key={group.tool} group={group} />
        ))}
      </div>
    </div>
  );
}
