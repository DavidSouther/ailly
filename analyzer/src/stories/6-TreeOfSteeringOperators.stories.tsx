/**
 * (c) Copyright 2026 Nominal Inc. All rights reserved.
 */

import { BadgeColor } from "../ui/colors";
import type { Meta } from "@storybook/react";
import {
  CheckCircle2Icon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleIcon,
  FilePenLineIcon,
  GitForkIcon,
  GitMergeIcon,
  Loader2Icon,
  SearchIcon,
  SquareTerminalIcon,
  TextIcon,
  TriangleAlertIcon,
  UsersIcon,
} from "lucide-react";
import React, { useState } from "react";

import { Badge } from "../ui/badges/badge";
import type { IconType } from "../ui/icons";
import { PanelAccordion, PanelAccordionItem } from "../ui/PanelAccordion";
import { BodyText, MutedText, StrongText } from "../ui/typography";
import { cn } from "../ui/utils";

/**
 * A sketch of the "Tree of Steering Operators" pattern from the Graph of
 * Actions design doc's "Where next?" section: the flat Orchestrator TODO
 * List becomes a genuine tree (most tasks are serial parent -> child
 * chains, with one deliberate parallel branch point modeling Multi-sample
 * branching's map/reduce shape), every node is tagged with one of the
 * seven steering operators from the "LLM Manifold" position paper's Table
 * 1, and expanding a node reveals both that operator's Move / Signal /
 * Referent-check triplet and a compact per-node tool-call summary.
 *
 * The paper's diagnostic point (Section 4-5): a referent check of "None"
 * -- or one that reduces to "depends on the selector" -- is a specific
 * warning sign, not an empty field. Prompting, Additional reasoning
 * tokens, and Multi-sample branching are flagged here for exactly that
 * reason; Execution feedback, Retrieval expansion, and ReAct-style
 * interaction are not, because each has a referent outside the trajectory
 * that can disagree with it.
 *
 * Continues the "migrate the legacy auth service" scenario used by the
 * sibling sketches (LinearChat, ToolCallsArtifacts, Subagents,
 * OrchestratorTodoList, ToolCallSummary) for continuity.
 */
const meta: Meta = {
  title: "Examples/Agent Harness Sketches/6. Tree of Steering Operators",
};

export default meta;

type TaskStatus = "done" | "active" | "pending";

const STATUS_LABEL: Record<TaskStatus, string> = {
  done: "Done",
  active: "In progress",
  pending: "Not started",
};

function StatusMark({ status }: { status: TaskStatus }) {
  if (status === "done") {
    return (
      <CheckCircle2Icon className="h-4 w-4 shrink-0 text-status-success" />
    );
  }
  if (status === "active") {
    return (
      <Loader2Icon className="h-4 w-4 shrink-0 animate-spin text-foreground-muted" />
    );
  }
  return <CircleIcon className="h-4 w-4 shrink-0 text-foreground-muted" />;
}

/**
 * The seven steering operators from Table 1 of "Position: Agentic LLM
 * Workflows as Trajectory-Steering on Manifolds in Document Spaces" --
 * each characterized by its move (what tokens enter the trajectory), its
 * signal (what the harness consults to produce them), and its referent
 * check (what outside the document can disagree with it).
 */
type OperatorKey =
  | "prompting"
  | "reasoning"
  | "execution"
  | "retrieval"
  | "branching"
  | "react"
  | "tree-search";

interface OperatorMeta {
  label: string;
  move: string;
  signal: string;
  referentCheck: string;
  /**
   * The paper treats a referent check of "None" -- or one that reduces to
   * "depends on the selector" -- as a specific diagnostic finding, not an
   * omission: a technique with no referent check behaves like additional
   * reasoning tokens however it's dressed up. Flagged operators surface
   * that warning directly on the node.
   */
  flagged: boolean;
}

const OPERATORS: Record<OperatorKey, OperatorMeta> = {
  prompting: {
    label: "Prompting",
    move: "Conditions the initial trajectory",
    signal: "Prompt tokens and examples",
    referentCheck:
      "None within the generation, human or agent operator when used in a harness",
    flagged: true,
  },
  reasoning: {
    label: "Additional reasoning tokens",
    move: 'Extends length of the trajectory before generating a final "response" document',
    signal: "Intermediate generated states",
    referentCheck: "None unless another operator supplies one",
    flagged: true,
  },
  execution: {
    label: "Execution feedback",
    move: "Appends an outside verdict and regenerates",
    signal: "Compiler, test, runtime, or API output",
    referentCheck: "Direct, within the coverage of the tool",
    flagged: false,
  },
  retrieval: {
    label: "Retrieval expansion",
    move: "Generates a stand-in query or document before retrieval",
    signal: "Retrieved corpus matches",
    referentCheck: "Indirect; retrieval can return irrelevant or no evidence",
    flagged: false,
  },
  branching: {
    label: "Multi-sample branching",
    move: "Samples several trajectories and selects or aggregates",
    signal: "Votes, comparisons, or an aggregator",
    referentCheck: "Depends on the selector; branches may share biases",
    flagged: true,
  },
  react: {
    label: "ReAct-style interaction",
    move: "Alternates reasoning, actions, and observations",
    signal: "Observation after each action",
    referentCheck: "Direct when the observation exposes the relevant state",
    flagged: false,
  },
  "tree-search": {
    label: "Tree-search agents",
    move: "Retain and revisit branches under a search policy",
    signal: "Value estimates, reflections, and environment feedback",
    referentCheck: "Depends on the evaluator and environment",
    flagged: false,
  },
};

type ToolName = "Grep" | "Read" | "Edit" | "Bash" | "Agent";
type ToolCategory = "exec" | "edit" | "read" | "other";

const TOOL_META: Record<ToolName, { icon: IconType; category: ToolCategory }> =
  {
    Grep: { icon: SearchIcon, category: "read" },
    Read: { icon: TextIcon, category: "read" },
    Edit: { icon: FilePenLineIcon, category: "edit" },
    Bash: { icon: SquareTerminalIcon, category: "exec" },
    Agent: { icon: UsersIcon, category: "other" },
  };

const CATEGORY_COLOR: Record<ToolCategory, BadgeColor> = {
  exec: BadgeColor.AMBER,
  edit: BadgeColor.SKY,
  read: BadgeColor.MINT,
  other: BadgeColor.PLUM,
};

interface ToolCallEntry {
  id: string;
  tool: ToolName;
  summary: string;
}

interface OperatorNode {
  id: string;
  label: string;
  status: TaskStatus;
  operator: OperatorKey;
  toolCalls: ToolCallEntry[];
  children?: OperatorNode[];
  /**
   * The convergence point of a parallel branch: rendered after `children`
   * as a merge back into the serial chain, not as another sibling branch.
   */
  convergesTo?: OperatorNode;
}

const REVIEW: OperatorNode = {
  id: "review",
  label: "Review the diff",
  status: "pending",
  operator: "react",
  toolCalls: [],
};

const RUNBOOK: OperatorNode = {
  id: "runbook",
  label: "Update the runbook",
  status: "pending",
  operator: "reasoning",
  toolCalls: [],
  children: [REVIEW],
};

const CANDIDATE_ADAPTER: OperatorNode = {
  id: "candidate-adapter",
  label: "Candidate: adapter shim via fromToken()",
  status: "done",
  operator: "branching",
  toolCalls: [
    {
      id: "ca-1",
      tool: "Edit",
      summary: "session.ts -- add fromToken() adapter",
    },
    {
      id: "ca-2",
      tool: "Edit",
      summary: "useSession.ts -- call through the adapter",
    },
    { id: "ca-3", tool: "Bash", summary: "pnpm vitest packages/auth --run" },
  ],
};

const CANDIDATE_RENAME: OperatorNode = {
  id: "candidate-rename",
  label: "Candidate: direct field rename across call sites",
  status: "done",
  operator: "branching",
  toolCalls: [
    { id: "cr-1", tool: "Edit", summary: "session.ts -- rename token field" },
    { id: "cr-2", tool: "Edit", summary: "useSession.ts -- follow the rename" },
    {
      id: "cr-3",
      tool: "Edit",
      summary: "server/middleware/session.ts -- follow the rename",
    },
    { id: "cr-4", tool: "Bash", summary: "pnpm vitest packages/auth --run" },
  ],
};

const CANDIDATE_WRAPPER: OperatorNode = {
  id: "candidate-wrapper",
  label: "Candidate: deprecation-warning wrapper class",
  status: "done",
  operator: "branching",
  toolCalls: [
    {
      id: "cw-1",
      tool: "Edit",
      summary: "session.ts -- wrap LegacySession, warn on use",
    },
    { id: "cw-2", tool: "Bash", summary: "pnpm vitest packages/auth --run" },
    {
      id: "cw-3",
      tool: "Bash",
      summary: "pnpm typecheck --filter=@nominal-io/auth",
    },
  ],
};

const SELECTED: OperatorNode = {
  id: "selected",
  label: "Selected: adapter shim via fromToken()",
  status: "active",
  operator: "branching",
  toolCalls: [
    {
      id: "sel-1",
      tool: "Agent",
      summary: "Compared 3 candidate branches on test pass rate and diff size",
    },
    {
      id: "sel-2",
      tool: "Bash",
      summary: "git merge candidate/adapter-shim --no-ff",
    },
    {
      id: "sel-3",
      tool: "Bash",
      summary: "pnpm vitest packages/auth --run (post-merge)",
    },
    {
      id: "sel-4",
      tool: "Edit",
      summary: "session.ts -- finalize the merged implementation",
    },
  ],
  children: [RUNBOOK],
};

const MIGRATE: OperatorNode = {
  id: "migrate",
  label: "Migrate session module & add tests",
  status: "done",
  operator: "execution",
  toolCalls: [
    { id: "mig-1", tool: "Read", summary: "packages/auth/src/session.ts" },
    { id: "mig-2", tool: "Read", summary: "packages/auth/src/session.test.ts" },
    {
      id: "mig-3",
      tool: "Bash",
      summary: "pnpm vitest packages/auth --run (baseline)",
    },
    {
      id: "mig-4",
      tool: "Agent",
      summary: "Spawned 3 candidate-implementation branches",
    },
  ],
  children: [CANDIDATE_ADAPTER, CANDIDATE_RENAME, CANDIDATE_WRAPPER],
  convergesTo: SELECTED,
};

const LOCATE: OperatorNode = {
  id: "locate",
  label: "Locate call sites of LegacySession",
  status: "done",
  operator: "retrieval",
  toolCalls: [
    { id: "loc-1", tool: "Grep", summary: 'rg -l "LegacySession" apps/scout' },
    {
      id: "loc-2",
      tool: "Grep",
      summary: 'rg -l "LegacySession" packages/trpc',
    },
    {
      id: "loc-3",
      tool: "Read",
      summary: "apps/scout/client/hooks/useSession.ts",
    },
  ],
  children: [MIGRATE],
};

const ROOT: OperatorNode = {
  id: "root",
  label: "Migrate the legacy auth service",
  status: "active",
  operator: "prompting",
  toolCalls: [
    {
      id: "root-1",
      tool: "Agent",
      summary:
        "Broke the request into a locate -> migrate -> runbook -> review chain",
    },
  ],
  children: [LOCATE],
};

function summarizeToolCalls(
  calls: ToolCallEntry[],
): { total: number; mostly: ToolName } | null {
  if (calls.length === 0) {
    return null;
  }
  const counts = new Map<ToolName, number>();
  for (const call of calls) {
    counts.set(call.tool, (counts.get(call.tool) ?? 0) + 1);
  }
  let mostly = calls[0]!.tool;
  let max = 0;
  for (const [tool, count] of counts) {
    if (count > max) {
      max = count;
      mostly = tool;
    }
  }
  return { total: calls.length, mostly };
}

function ToolCallSummaryRow({ toolCalls }: { toolCalls: ToolCallEntry[] }) {
  const [expanded, setExpanded] = useState(false);
  const summary = summarizeToolCalls(toolCalls);

  if (!summary) {
    return (
      <MutedText className="text-xs">
        This step hasn&apos;t made any tool calls yet.
      </MutedText>
    );
  }

  const meta = TOOL_META[summary.mostly];
  const Icon = meta.icon;

  return (
    <div className="flex flex-col gap-1.5">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className="flex items-center gap-1.5 self-start rounded px-1 py-0.5 text-xs text-foreground-muted hover:bg-background-hover hover:text-foreground"
      >
        {expanded ? (
          <ChevronDownIcon className="h-3 w-3" />
        ) : (
          <ChevronRightIcon className="h-3 w-3" />
        )}
        <span>
          {summary.total} call{summary.total === 1 ? "" : "s"}: mostly{" "}
        </span>
        <Badge color={CATEGORY_COLOR[meta.category]} textSize="sm" icon={Icon}>
          {summary.mostly}
        </Badge>
      </button>
      {expanded ? (
        <PanelAccordion className="ml-4 rounded-sm border">
          {toolCalls.map((call) => {
            const callMeta = TOOL_META[call.tool];
            const CallIcon = callMeta.icon;
            return (
              <PanelAccordionItem
                key={call.id}
                value={call.id}
                label={`${call.tool}: ${call.summary}`}
                actions={<CallIcon className="h-3 w-3 text-foreground-muted" />}
              >
                <p className="p-2 text-xs text-foreground-muted">
                  {call.summary}
                </p>
              </PanelAccordionItem>
            );
          })}
        </PanelAccordion>
      ) : null}
    </div>
  );
}

function OperatorBadge({ operator }: { operator: OperatorKey }) {
  const meta = OPERATORS[operator];
  return (
    <Badge
      color={meta.flagged ? BadgeColor.BERRY : BadgeColor.METAL}
      outlined={meta.flagged}
      textSize="sm"
      className="shrink-0"
      icon={meta.flagged ? TriangleAlertIcon : undefined}
    >
      {meta.label}
    </Badge>
  );
}

function OperatorDetail({ operator }: { operator: OperatorKey }) {
  const meta = OPERATORS[operator];
  return (
    <div className="flex flex-col gap-1 rounded-sm border bg-background p-2">
      <div className="flex items-center gap-1.5">
        <StrongText className="text-xs">{meta.label}</StrongText>
        {meta.flagged ? (
          <Badge
            color={BadgeColor.BERRY}
            outlined={true}
            textSize="sm"
            icon={TriangleAlertIcon}
          >
            No referent outside the trajectory
          </Badge>
        ) : null}
      </div>
      <dl className="grid grid-cols-[5.5rem_1fr] gap-x-2 gap-y-1 text-xs">
        <dt className="tracking-wide text-foreground-muted uppercase">Move</dt>
        <dd className="m-0 text-foreground">{meta.move}</dd>
        <dt className="tracking-wide text-foreground-muted uppercase">
          Signal
        </dt>
        <dd className="m-0 text-foreground">{meta.signal}</dd>
        <dt className="tracking-wide text-foreground-muted uppercase">
          Referent check
        </dt>
        <dd
          className={cn(
            "m-0",
            meta.flagged ? "font-medium text-status-error" : "text-foreground",
          )}
        >
          {meta.referentCheck}
        </dd>
      </dl>
    </div>
  );
}

const DEPTH_PADDING = [
  "pl-0",
  "pl-5",
  "pl-10",
  "pl-14",
  "pl-[4.5rem]",
  "pl-20",
];

function depthPadding(depth: number): string {
  return DEPTH_PADDING[Math.min(depth, DEPTH_PADDING.length - 1)]!;
}

function TreeNode({
  node,
  depth,
  isBranch,
}: {
  node: OperatorNode;
  depth: number;
  isBranch?: boolean;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="flex flex-col">
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className={cn(
          "flex w-full items-center gap-2 py-2 pr-3 text-left hover:bg-background-hover",
          depthPadding(depth),
        )}
      >
        {expanded ? (
          <ChevronDownIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        ) : (
          <ChevronRightIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        )}
        <StatusMark status={node.status} />
        {isBranch ? (
          <GitForkIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
        ) : null}
        <StrongText
          className={cn(
            "flex-1 text-sm",
            node.status === "done" && "text-foreground-muted line-through",
          )}
        >
          {node.label}
        </StrongText>
        <OperatorBadge operator={node.operator} />
        <MutedText className="shrink-0 text-xs">
          {STATUS_LABEL[node.status]}
        </MutedText>
      </button>
      {expanded ? (
        <div
          className={cn(
            "border-t border-border bg-background-hover/40 py-2 pr-3",
            depthPadding(depth),
          )}
        >
          <div className="flex flex-col gap-2 pl-6">
            <OperatorDetail operator={node.operator} />
            <ToolCallSummaryRow toolCalls={node.toolCalls} />
          </div>
        </div>
      ) : null}
      {node.children && node.children.length > 0 ? (
        <div>
          {node.children.map((child) => (
            <TreeNode
              key={child.id}
              node={child}
              depth={depth + 1}
              isBranch={node.children!.length > 1}
            />
          ))}
        </div>
      ) : null}
      {node.convergesTo ? (
        <>
          <div
            className={cn(
              "flex items-center gap-1.5 py-1 text-xs text-foreground-muted",
              depthPadding(depth + 1),
            )}
          >
            <GitMergeIcon className="h-3 w-3 shrink-0" />
            <span>{node.children?.length ?? 0} branches converge here</span>
          </div>
          <TreeNode node={node.convergesTo} depth={depth + 1} />
        </>
      ) : null}
    </div>
  );
}

export function Default() {
  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-3 bg-background p-6">
      <BodyText className="text-xs text-foreground-muted">
        The Orchestrator TODO List, regrown as a tree. Locate -&gt; Migrate
        -&gt; Runbook -&gt; Review form a serial chain; Migrate itself fans out
        into three candidate implementations (Multi-sample branching) that
        converge into one selected approach before the chain continues. Every
        node is tagged with a steering operator from the paper&apos;s Table 1 --
        expand a node for its Move / Signal / Referent-check triplet and a
        compact tool-call summary for that step. Operators whose referent check
        is <StrongText className="text-xs">None</StrongText> (Prompting,
        Additional reasoning tokens) or reduces to{" "}
        <StrongText className="text-xs">depends on the selector</StrongText>{" "}
        (Multi-sample branching) carry a warning badge -- that&apos;s the
        paper&apos;s specific diagnostic point, not an omission.
      </BodyText>
      <div className="rounded-md border">
        <TreeNode node={ROOT} depth={0} />
      </div>
    </div>
  );
}
