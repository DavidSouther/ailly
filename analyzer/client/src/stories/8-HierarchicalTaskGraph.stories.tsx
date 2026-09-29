/**
 * (c) Copyright 2026 Nominal Inc. All rights reserved.
 */

import type { Meta } from "@storybook/react";
import {
  CheckCircle2Icon,
  ChevronDownIcon as ChevronBottomIcon,
  ChevronRightIcon,
  CircleDotIcon,
  CircleSlashIcon,
  EyeIcon,
  FilePenLineIcon,
  HourglassIcon,
  LayersIcon,
  Loader2Icon,
  MinusIcon,
  PlusIcon,
  SquareTerminalIcon,
  TargetIcon,
  WrenchIcon,
} from "lucide-react";
import type React from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { BadgeColor } from "../ui/colors";
import { useResizeObserverEffect } from "../ui/hooks/useResizeObserverEffect";

import { Badge } from "../ui/badges/badge";
import type { IconType } from "../ui/icons";
import { BodyText, MutedText, StrongText } from "../ui/typography";
import { cn } from "../ui/utils";

/**
 * A sketch that takes the pull-based task DAG from sketch 7 and refines it
 * inside a hierarchy, drawn with the hulls-and-segments technique from
 * "Graph Visualization by Organizing Connections in Collapsible
 * Hierarchical Graphs" (Technical Disclosure Commons, 2996, 2020).
 *
 * The topology is unchanged from sketch 7. Tasks are still the only nodes,
 * and the only edges are task -> task "feeds" edges. What is added is a
 * second, orthogonal relation over the same nodes -- a containment tree --
 * and two derived render primitives:
 *
 * - A HULL is a box with descendants. It can be collapsed to hide its
 *   contents or expanded to show them. A hull may itself be a task (the
 *   Task Tree hierarchy below is entirely made of those), or a synthetic
 *   group (every other hierarchy below).
 * - A SEGMENT is the bundle of edges between the descendants of a pair of
 *   boxes. Every edge crossing a hull boundary is routed through a shared
 *   port on that boundary, so N edges between two hulls converge into one
 *   crossing rather than N independent lines. Collapsing a hull does not
 *   delete its edges; it merges them into the segments of its ancestors.
 *
 * Edges are drawn uniformly -- one weight, one colour, no dashes. The
 * hierarchy is what the picture is for, and a second visual channel riding
 * on the edges competes with it; the only thing a segment carries beyond
 * its route is a count of the edges inside it.
 *
 * The two things being varied are independent, and that is the point of
 * the file:
 *
 *  1. WHAT the hierarchy is -- four candidate containment trees over the
 *     same 17 tasks (`HIERARCHIES`): Task Tree, Deliverable, Path,
 *     Session. Each needs a stated tie-break rule, because a DAG does not
 *     have a tree structure lying around: a task with three consumers has
 *     to land in exactly one hull.
 *  2. HOW the hierarchy is shown. Containment draws nesting literally and
 *     SegmentMatrix is its fully-collapsed limit; both are kept. Bands and
 *     Outline are kept as REJECTED -- they are still here, and still
 *     working, because the specific way each one fails is a constraint on
 *     whatever replaces them, and that is only legible next to the thing
 *     they failed at. Each carries a banner saying why.
 *
 * Every renderer accepts every hierarchy; the stories below default to the
 * pairing that shows each one at its most legible, and each has a switcher.
 *
 * Continues the "migrate the legacy auth service" scenario used by the
 * sibling sketches (1-7) for continuity.
 */
const meta: Meta = {
  title: "Examples/Agent Harness Sketches/8. Hierarchical Task Graph",
};

export default meta;

// ---------------------------------------------------------------------------
// Topology -- carried over from sketch 7, widened so hulls have real contents.
// ---------------------------------------------------------------------------

type TaskState = "ready" | "running" | "waiting" | "finished";

/**
 * The task kinds named in the design doc: a goal is a desired state of the
 * world, an edit modifies an artifact, a run executes a computation, a
 * review produces feedback without acting on it, and a tool call is an
 * ordinary (usually small) task like any other.
 */
type TaskKind = "goal" | "edit" | "run" | "review" | "tool";

interface Task {
  id: string;
  label: string;
  kind: TaskKind;
  state: TaskState;
  /** Ids of tasks this one needs output from -- its incoming edges. */
  needs: string[];
  /**
   * Subset of `needs` wired by the discovery-and-reuse mechanism rather
   * than by spawning. Every task has at most one non-reused consumer: the
   * one that created it. That is what makes the Task Tree hierarchy free.
   */
  reused?: string[];
}

const GOAL_ID = "goal";

const TASKS: Task[] = [
  {
    id: GOAL_ID,
    label: "Migrate the legacy auth service",
    kind: "goal",
    state: "waiting",
    needs: ["review", "runbook", "pr", "rollout"],
  },
  {
    id: "review",
    label: "Review the diff",
    kind: "review",
    state: "waiting",
    needs: ["typecheck"],
  },
  {
    id: "runbook",
    label: "Update the auth runbook",
    kind: "edit",
    state: "running",
    needs: ["locate", "read-runbook"],
    reused: ["locate"],
  },
  {
    id: "pr",
    label: "Draft the PR description",
    kind: "edit",
    state: "waiting",
    needs: ["summarize"],
  },
  {
    id: "rollout",
    label: "Stage the rollout plan",
    kind: "edit",
    state: "waiting",
    needs: ["flag"],
  },
  {
    id: "typecheck",
    label: "Run typecheck & unit tests",
    kind: "run",
    state: "waiting",
    needs: ["migrate-session", "migrate-tokens"],
  },
  {
    id: "migrate-session",
    label: "Migrate session module",
    kind: "edit",
    state: "finished",
    needs: ["locate", "read-session"],
  },
  {
    id: "migrate-tokens",
    label: "Migrate token refresh path",
    kind: "edit",
    state: "running",
    needs: ["locate", "read-tokens"],
    reused: ["locate"],
  },
  {
    id: "summarize",
    label: "Summarize the diff",
    kind: "review",
    state: "waiting",
    needs: ["migrate-session", "migrate-tokens"],
    reused: ["migrate-session", "migrate-tokens"],
  },
  {
    id: "flag",
    label: "Add auth.legacy-session kill switch",
    kind: "edit",
    state: "ready",
    needs: ["read-flags"],
  },
  {
    id: "locate",
    label: "Locate call sites of LegacySession",
    kind: "run",
    state: "finished",
    needs: ["grep-scout", "grep-trpc"],
  },
  {
    id: "read-session",
    label: "Read packages/auth/src/session.ts",
    kind: "tool",
    state: "finished",
    needs: [],
  },
  {
    id: "read-tokens",
    label: "Read packages/auth/src/tokens.ts",
    kind: "tool",
    state: "finished",
    needs: [],
  },
  {
    id: "read-runbook",
    label: "Read docs/runbooks/auth.md",
    kind: "tool",
    state: "finished",
    needs: [],
  },
  {
    id: "read-flags",
    label: "Read packages/flags/src/registry.ts",
    kind: "tool",
    state: "finished",
    needs: [],
  },
  {
    id: "grep-scout",
    label: 'grep -l "LegacySession" apps/scout',
    kind: "tool",
    state: "finished",
    needs: [],
  },
  {
    id: "grep-trpc",
    label: 'grep -l "LegacySession" packages/trpc',
    kind: "tool",
    state: "finished",
    needs: [],
  },
];

const TASK_BY_ID = new Map(TASKS.map((task) => [task.id, task]));

interface Edge {
  id: string;
  from: string;
  to: string;
  /** Wired by reuse rather than by spawning -- see `Task.reused`. */
  reused: boolean;
}

const EDGES: Edge[] = TASKS.flatMap((task) =>
  task.needs.map((from) => ({
    id: `${from}->${task.id}`,
    from,
    to: task.id,
    reused: task.reused?.includes(from) ?? false,
  })),
);

/**
 * Rank = longest path from a task to the goal, following "feeds" edges.
 * The goal is rank 0; a task always ranks strictly higher than every task
 * that consumes it. Derived from the data, never hand-assigned -- same
 * definition as sketch 7, so the two sketches agree on vertical order.
 */
function computeRanks(): Map<string, number> {
  const consumers = new Map<string, string[]>();
  for (const edge of EDGES) {
    consumers.set(edge.from, [...(consumers.get(edge.from) ?? []), edge.to]);
  }
  const rank = new Map<string, number>([[GOAL_ID, 0]]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const task of TASKS) {
      if (rank.has(task.id)) {
        continue;
      }
      const consumerRanks = (consumers.get(task.id) ?? []).map((id) => rank.get(id));
      if (consumerRanks.length === 0 || consumerRanks.some((r) => r == null)) {
        continue;
      }
      rank.set(task.id, 1 + Math.max(...(consumerRanks as number[])));
      changed = true;
    }
  }
  return rank;
}

const RANK = computeRanks();
const MAX_RANK = Math.max(...TASKS.map((task) => RANK.get(task.id) ?? 0));

// ---------------------------------------------------------------------------
// Hierarchy variants -- four containment trees over the same 17 tasks.
// ---------------------------------------------------------------------------

interface Box {
  id: string;
  label: string;
  parent: string | null;
  /**
   * Set when the box is a task. A task box with children is a hull that is
   * itself a node, which the technique explicitly allows; its own header is
   * an edge endpoint while its outer border is a segment boundary.
   */
  taskId?: string;
}

interface Hierarchy {
  id: string;
  name: string;
  /** What containment means in this variant. */
  blurb: string;
  /**
   * The tie-break that turns the DAG into a tree. A DAG has no hierarchy
   * lying around: `locate` has three consumers and must land in one hull.
   */
  rule: string;
  boxes: Box[];
}

/** Materializes a task as a leaf box under `parent`. */
function taskBox(taskId: string, parent: string | null): Box {
  const task = TASK_BY_ID.get(taskId);
  return { id: taskId, label: task?.label ?? taskId, parent, taskId };
}

/**
 * Variant 1: Task Tree. Parent = whoever spawned it. This hierarchy costs
 * nothing to build -- it is exactly the one outgoing edge each task was
 * born with, before any reuse edge was wired -- and every hull is a real
 * task rather than a synthetic group.
 */
const TASK_TREE_BOXES: Box[] = TASKS.map((task) => {
  const spawnedBy = TASKS.find(
    (candidate) =>
      candidate.needs.includes(task.id) && !(candidate.reused?.includes(task.id) ?? false),
  );
  return taskBox(task.id, spawnedBy?.id ?? null);
});

const DELIVERABLE_HULLS: Box[] = [
  { id: "d-code", label: "Code changes", parent: null },
  { id: "d-runbook", label: "Runbook", parent: null },
  { id: "d-pr", label: "Pull request", parent: null },
  { id: "d-rollout", label: "Rollout", parent: null },
  { id: "d-shared", label: "Shared findings", parent: null },
];

const DELIVERABLE_PLACEMENT: Record<string, string | null> = {
  goal: null,
  review: "d-code",
  typecheck: "d-code",
  "migrate-session": "d-code",
  "migrate-tokens": "d-code",
  "read-session": "d-code",
  "read-tokens": "d-code",
  runbook: "d-runbook",
  "read-runbook": "d-runbook",
  pr: "d-pr",
  summarize: "d-pr",
  rollout: "d-rollout",
  flag: "d-rollout",
  "read-flags": "d-rollout",
  locate: "d-shared",
  "grep-scout": "d-shared",
  "grep-trpc": "d-shared",
};

const PATH_HULLS: Box[] = [
  { id: "p-packages", label: "packages/", parent: null },
  { id: "p-auth", label: "auth", parent: "p-packages" },
  { id: "p-flags", label: "flags", parent: "p-packages" },
  { id: "p-trpc", label: "trpc", parent: "p-packages" },
  { id: "p-apps", label: "apps/", parent: null },
  { id: "p-scout", label: "scout", parent: "p-apps" },
  { id: "p-docs", label: "docs/", parent: null },
  { id: "p-none", label: "No artifact", parent: null },
];

const PATH_PLACEMENT: Record<string, string | null> = {
  "read-session": "p-auth",
  "migrate-session": "p-auth",
  "read-tokens": "p-auth",
  "migrate-tokens": "p-auth",
  "read-flags": "p-flags",
  flag: "p-flags",
  "grep-trpc": "p-trpc",
  "grep-scout": "p-scout",
  "read-runbook": "p-docs",
  runbook: "p-docs",
  goal: "p-none",
  review: "p-none",
  typecheck: "p-none",
  locate: "p-none",
  summarize: "p-none",
  pr: "p-none",
  rollout: "p-none",
};

const SESSION_HULLS: Box[] = [
  { id: "s-orch", label: "orchestrator", parent: null },
  { id: "s-code", label: "code agent", parent: null },
  { id: "s-search", label: "search agent", parent: null },
  { id: "s-docs", label: "docs agent", parent: null },
  { id: "s-flags", label: "flags agent", parent: null },
];

const SESSION_PLACEMENT: Record<string, string | null> = {
  goal: "s-orch",
  review: "s-orch",
  pr: "s-orch",
  rollout: "s-orch",
  typecheck: "s-code",
  "migrate-session": "s-code",
  "migrate-tokens": "s-code",
  "read-session": "s-code",
  "read-tokens": "s-code",
  locate: "s-search",
  "grep-scout": "s-search",
  "grep-trpc": "s-search",
  runbook: "s-docs",
  "read-runbook": "s-docs",
  summarize: "s-docs",
  flag: "s-flags",
  "read-flags": "s-flags",
};

function boxesFrom(hulls: Box[], placement: Record<string, string | null>) {
  return [...hulls, ...TASKS.map((task) => taskBox(task.id, placement[task.id] ?? null))];
}

const HIERARCHIES: Hierarchy[] = [
  {
    id: "task-tree",
    name: "Task Tree",
    blurb:
      "A hull is the task that spawned its contents. Nesting is the spawn tree, so the hierarchy is free -- it is the one outgoing edge every task was born with.",
    rule: "First requester wins. A task that gets reused keeps the hull of whoever spawned it; later consumers reach it through a segment.",
    boxes: TASK_TREE_BOXES,
  },
  {
    id: "deliverable",
    name: "Deliverable",
    blurb:
      "A hull is a thing the goal needs produced. Two levels, wide and shallow; the goal itself sits outside every hull as the sink.",
    rule: "A task whose consumers span more than one deliverable is promoted out into Shared findings, together with the tool calls it owns.",
    boxes: boxesFrom(DELIVERABLE_HULLS, DELIVERABLE_PLACEMENT),
  },
  {
    id: "path",
    name: "Path",
    blurb:
      "A hull is a path prefix in the repository -- what the task reads or writes. Nested three deep, and completely independent of the DAG.",
    rule: "Tasks that touch no artifact (pure reasoning and orchestration) fall into a residual hull rather than being scattered.",
    boxes: boxesFrom(PATH_HULLS, PATH_PLACEMENT),
  },
  {
    id: "session",
    name: "Session",
    blurb:
      "A hull is one long-running agent session. The work is split across a handful of agents that each stay alive across many tasks, rather than a fresh isolated subagent per task. That trades away the context-isolation benefit -- a session accumulates everything it has seen -- for a stable owner per area of the work, a warm context that does not re-derive the same background, and a segment count that reads directly as how much these agents had to tell each other.",
    rule: "A task belongs to the session that ran it. Handoffs are never hidden: work that moved between sessions must show as a segment.",
    boxes: boxesFrom(SESSION_HULLS, SESSION_PLACEMENT),
  },
];

const HIERARCHY_BY_ID = new Map(HIERARCHIES.map((h) => [h.id, h]));

// ---------------------------------------------------------------------------
// Derived hierarchy graph.
// ---------------------------------------------------------------------------

interface HierarchyGraph {
  hierarchy: Hierarchy;
  boxById: Map<string, Box>;
  /** Child ids, ordered by min descendant rank (closest to the goal first). */
  childrenOf: Map<string, string[]>;
  roots: string[];
  depthOf: Map<string, number>;
  minRankOf: Map<string, number>;
  /** [self, parent, ..., root] for every box. */
  ancestorsOf: Map<string, string[]>;
  /** Every task id under a box, including the box's own task if it has one. */
  tasksUnder: Map<string, string[]>;
  hullIds: Set<string>;
  maxDepth: number;
}

function buildGraph(hierarchy: Hierarchy): HierarchyGraph {
  const boxById = new Map(hierarchy.boxes.map((box) => [box.id, box]));
  const childrenOf = new Map<string, string[]>();
  const roots: string[] = [];
  for (const box of hierarchy.boxes) {
    if (box.parent == null) {
      roots.push(box.id);
    } else {
      childrenOf.set(box.parent, [...(childrenOf.get(box.parent) ?? []), box.id]);
    }
  }

  const ancestorsOf = new Map<string, string[]>();
  const depthOf = new Map<string, number>();
  for (const box of hierarchy.boxes) {
    const chain: string[] = [];
    let cursor: string | undefined = box.id;
    while (cursor != null) {
      chain.push(cursor);
      cursor = boxById.get(cursor)?.parent ?? undefined;
    }
    ancestorsOf.set(box.id, chain);
    depthOf.set(box.id, chain.length - 1);
  }

  const tasksUnder = new Map<string, string[]>();
  for (const box of hierarchy.boxes) {
    if (box.taskId == null) {
      continue;
    }
    for (const ancestor of ancestorsOf.get(box.id) ?? []) {
      tasksUnder.set(ancestor, [...(tasksUnder.get(ancestor) ?? []), box.taskId]);
    }
  }

  const minRankOf = new Map<string, number>();
  for (const box of hierarchy.boxes) {
    const ranks = (tasksUnder.get(box.id) ?? []).map((taskId) => RANK.get(taskId) ?? MAX_RANK);
    minRankOf.set(box.id, ranks.length > 0 ? Math.min(...ranks) : MAX_RANK);
  }

  const order = (a: string, b: string) => {
    const byRank = (minRankOf.get(a) ?? 0) - (minRankOf.get(b) ?? 0);
    return byRank !== 0
      ? byRank
      : (boxById.get(a)?.label ?? "").localeCompare(boxById.get(b)?.label ?? "");
  };
  for (const [parent, kids] of childrenOf) {
    childrenOf.set(parent, [...kids].sort(order));
  }
  roots.sort(order);

  return {
    hierarchy,
    boxById,
    childrenOf,
    roots,
    depthOf,
    minRankOf,
    ancestorsOf,
    tasksUnder,
    hullIds: new Set([...childrenOf.keys()]),
    maxDepth: Math.max(...[...depthOf.values()]),
  };
}

const GRAPHS = new Map(HIERARCHIES.map((h) => [h.id, buildGraph(h)]));

/**
 * The box a task actually renders as: itself, unless one of its ancestors
 * is collapsed, in which case the outermost collapsed ancestor stands in
 * for it. A collapsed hull that is itself a task still renders its own
 * header, so collapsing a hull never hides the hull's own node.
 */
function renderedBoxOf(
  graph: HierarchyGraph,
  taskId: string,
  collapsed: ReadonlySet<string>,
): string {
  const chain = graph.ancestorsOf.get(taskId) ?? [taskId];
  for (let i = chain.length - 1; i >= 1; i--) {
    const ancestor = chain[i];
    if (ancestor != null && collapsed.has(ancestor)) {
      return ancestor;
    }
  }
  return taskId;
}

interface Segment {
  key: string;
  from: string;
  to: string;
  edges: Edge[];
}

interface SegmentSet {
  segments: Segment[];
  /** Edges swallowed whole by a collapsed box, keyed by that box. */
  internal: Map<string, number>;
  /** Rendered box id per task. */
  renderedBoxOf: Map<string, string>;
}

/**
 * Bundles the unchanged edge list into the segments visible at the current
 * expansion state. Collapsing never drops an edge: it either re-anchors it
 * to an ancestor or folds it into that ancestor's internal count.
 */
function computeSegments(graph: HierarchyGraph, collapsed: ReadonlySet<string>): SegmentSet {
  const rendered = new Map(
    TASKS.map((task) => [task.id, renderedBoxOf(graph, task.id, collapsed)]),
  );
  const byKey = new Map<string, Segment>();
  const internal = new Map<string, number>();
  for (const edge of EDGES) {
    const from = rendered.get(edge.from);
    const to = rendered.get(edge.to);
    if (from == null || to == null) {
      continue;
    }
    if (from === to) {
      internal.set(from, (internal.get(from) ?? 0) + 1);
      continue;
    }
    const key = `${from}->${to}`;
    const existing = byKey.get(key);
    if (existing) {
      existing.edges.push(edge);
    } else {
      byKey.set(key, { key, from, to, edges: [edge] });
    }
  }
  return {
    segments: [...byKey.values()],
    internal,
    renderedBoxOf: rendered,
  };
}

/**
 * The ancestor a task groups under when the hierarchy is cut at a fixed
 * depth. Used by the renderers that flatten to a single level (Bands,
 * SegmentMatrix); a task shallower than the cut stands for itself.
 */
function ancestorAtDepth(graph: HierarchyGraph, boxId: string, depth: number): string {
  const chain = graph.ancestorsOf.get(boxId) ?? [boxId];
  // chain[0] is the box; chain[chain.length - 1] is the root (depth 0).
  const index = chain.length - 1 - depth;
  return chain[Math.max(0, Math.min(index, chain.length - 1))] ?? boxId;
}

/**
 * The shallowest cut that separates the tasks into at least three groups.
 * The Task Tree hierarchy has a single root, so cutting at depth 0 would
 * put every task in one lane; the other three separate at depth 0.
 */
function defaultCutDepth(graph: HierarchyGraph): number {
  for (let depth = 0; depth <= graph.maxDepth; depth++) {
    const groups = new Set(TASKS.map((task) => ancestorAtDepth(graph, task.id, depth)));
    if (groups.size >= 3) {
      return depth;
    }
  }
  return 0;
}

// ---------------------------------------------------------------------------
// Shared chrome.
// ---------------------------------------------------------------------------

interface StateMeta {
  label: string;
  icon: IconType;
  badgeColor: BadgeColor;
  iconColor?: string;
}

const STATE_META: Record<TaskState, StateMeta> = {
  ready: { label: "Ready", icon: CircleDotIcon, badgeColor: BadgeColor.METAL },
  running: {
    label: "Running",
    icon: Loader2Icon,
    badgeColor: BadgeColor.SKY,
    iconColor: "animate-spin",
  },
  waiting: {
    label: "Waiting",
    icon: HourglassIcon,
    badgeColor: BadgeColor.METAL,
  },
  finished: {
    label: "Finished",
    icon: CheckCircle2Icon,
    badgeColor: BadgeColor.MINT,
  },
};

const KIND_ICON: Record<TaskKind, IconType> = {
  goal: TargetIcon,
  edit: FilePenLineIcon,
  run: SquareTerminalIcon,
  review: EyeIcon,
  tool: WrenchIcon,
};

/**
 * Every edge is drawn the same: one weight, one colour, no dashes. The
 * hierarchy is the only thing the picture encodes, so a second visual
 * channel riding on the edges (reuse vs spawn, bundle size as thickness)
 * competes with it for attention and was cut.
 *
 * Set inline rather than via Tailwind's `stroke-*` utility: this token is
 * used elsewhere as `text-*`/`bg-*`, but the `stroke` variant is not
 * reliably present in this file's generated CSS, and SVG's default
 * `stroke` is `none` -- a missing utility here fails silently as an
 * invisible line rather than an unstyled one. (Same footgun as sketch 7.)
 */
const EDGE_COLOR = "var(--color-foreground-muted)";
const EDGE_WIDTH = 1.25;

function TaskChip({
  taskId,
  compact = false,
  emphasis = false,
}: {
  taskId: string;
  compact?: boolean;
  emphasis?: boolean;
}) {
  const task = TASK_BY_ID.get(taskId);
  if (!task) {
    return null;
  }
  const state = STATE_META[task.state];
  const KindIcon = KIND_ICON[task.kind];
  return (
    <div className={cn("flex min-w-0 items-center gap-1.5", compact ? "text-[11px]" : "text-xs")}>
      <KindIcon
        className={cn(
          "shrink-0",
          compact ? "h-3 w-3" : "h-3.5 w-3.5",
          task.kind === "tool" ? "text-foreground-muted" : "text-foreground",
        )}
      />
      <StrongText
        className={cn(
          "min-w-0 flex-1 truncate",
          compact ? "text-[11px]" : "text-xs",
          task.kind === "tool" && "font-normal text-foreground-muted",
          emphasis && "text-foreground",
        )}
      >
        {task.label}
      </StrongText>
      <Badge
        color={state.badgeColor}
        textSize="sm"
        icon={state.icon}
        iconColor={state.iconColor}
        className="shrink-0"
      >
        {state.label}
      </Badge>
    </div>
  );
}

function HierarchyPicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (id: string) => void;
}) {
  const active = HIERARCHY_BY_ID.get(value);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-1">
        <MutedText className="mr-1 text-xs tracking-wide uppercase">Hierarchy</MutedText>
        {HIERARCHIES.map((hierarchy) => (
          <button
            key={hierarchy.id}
            type="button"
            onClick={() => onChange(hierarchy.id)}
            className={cn(
              "rounded-sm border px-2 py-1 text-xs",
              hierarchy.id === value
                ? "border-foreground bg-background-hover text-foreground"
                : "border-border text-foreground-muted hover:bg-background-hover",
            )}
          >
            {hierarchy.name}
          </button>
        ))}
      </div>
      {active ? (
        <div className="flex flex-col gap-1 rounded-sm border border-border bg-background-hover/40 p-2">
          <BodyText className="text-xs text-foreground-muted">{active.blurb}</BodyText>
          <BodyText className="text-xs text-foreground-muted">
            <StrongText className="text-xs">Tie-break: </StrongText>
            {active.rule}
          </BodyText>
        </div>
      ) : null}
    </div>
  );
}

function DepthStepper({
  value,
  max,
  onChange,
}: {
  value: number;
  max: number;
  onChange: (next: number) => void;
}) {
  return (
    <div className="flex items-center gap-1">
      <MutedText className="mr-1 text-xs tracking-wide uppercase">Cut depth</MutedText>
      <button
        type="button"
        onClick={() => onChange(Math.max(0, value - 1))}
        className="rounded-sm border border-border p-1 text-foreground-muted hover:bg-background-hover"
        aria-label="Shallower cut"
      >
        <MinusIcon className="h-3 w-3" />
      </button>
      <span className="w-4 text-center text-xs text-foreground">{value}</span>
      <button
        type="button"
        onClick={() => onChange(Math.min(max, value + 1))}
        className="rounded-sm border border-border p-1 text-foreground-muted hover:bg-background-hover"
        aria-label="Deeper cut"
      >
        <PlusIcon className="h-3 w-3" />
      </button>
    </div>
  );
}

function SegmentLegend({ note }: { note?: string }) {
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-border pt-2">
      <span className="flex items-center gap-1.5 text-xs text-foreground-muted">
        <span className="inline-block h-0.5 w-6" style={{ background: EDGE_COLOR, opacity: 0.7 }} />
        one segment; the number on it is how many edges it bundles
      </span>
      {note ? <span className="text-xs text-foreground-muted">{note}</span> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Geometry.
// ---------------------------------------------------------------------------

interface Pt {
  x: number;
  y: number;
}

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

function centerOf(rect: Rect): Pt {
  return { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 };
}

/**
 * Where the ray from a box's center toward `toward` crosses that box's
 * border. This is the whole bundling mechanism: the port depends only on
 * the pair of boxes, never on the individual edge, so every edge crossing
 * the same boundary toward the same peer meets at the same point.
 */
function borderPoint(rect: Rect, toward: Pt, outset = 0): Pt {
  const c = centerOf(rect);
  const dx = toward.x - c.x;
  const dy = toward.y - c.y;
  if (dx === 0 && dy === 0) {
    return c;
  }
  const hw = rect.w / 2 + outset;
  const hh = rect.h / 2 + outset;
  const sx = dx === 0 ? Number.POSITIVE_INFINITY : Math.abs(hw / dx);
  const sy = dy === 0 ? Number.POSITIVE_INFINITY : Math.abs(hh / dy);
  const scale = Math.min(sx, sy);
  return { x: c.x + dx * scale, y: c.y + dy * scale };
}

function towards(from: Pt, toward: Pt, distance: number): Pt {
  const dx = toward.x - from.x;
  const dy = toward.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  const t = Math.min(distance, len / 2) / len;
  return { x: from.x + dx * t, y: from.y + dy * t };
}

function dedupePoints(points: Pt[]): Pt[] {
  return points.filter((point, index) => {
    const prev = points[index - 1];
    return prev == null || Math.abs(prev.x - point.x) > 0.5 || Math.abs(prev.y - point.y) > 0.5;
  });
}

/** A polyline through the ports, with the corners rounded off. */
function smoothPath(points: Pt[], radius = 14): string {
  const pts = dedupePoints(points);
  const [first] = pts;
  const last = pts.at(-1);
  if (first == null || last == null || pts.length < 2) {
    return "";
  }
  let d = `M ${first.x},${first.y}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const prev = pts[i - 1];
    const cur = pts[i];
    const next = pts[i + 1];
    if (prev == null || cur == null || next == null) {
      continue;
    }
    const a = towards(cur, prev, radius);
    const b = towards(cur, next, radius);
    d += ` L ${a.x},${a.y} Q ${cur.x},${cur.y} ${b.x},${b.y}`;
  }
  d += ` L ${last.x},${last.y}`;
  return d;
}

const MARKER_ID = "hulls-arrow";

function ArrowDefs() {
  return (
    <defs>
      <marker
        id={MARKER_ID}
        viewBox="0 0 10 10"
        refX="9"
        refY="5"
        markerWidth="5"
        markerHeight="5"
        orient="auto-start-reverse"
      >
        <path d="M0,0 L10,5 L0,10 Z" style={{ fill: EDGE_COLOR }} />
      </marker>
    </defs>
  );
}

interface DrawnSegment {
  key: string;
  d: string;
  label?: { x: number; y: number; text: string };
}

function SegmentPaths({ segments }: { segments: DrawnSegment[] }) {
  return (
    <>
      {segments.map((segment) => (
        <path
          key={segment.key}
          d={segment.d}
          fill="none"
          style={{ stroke: EDGE_COLOR }}
          strokeOpacity={0.7}
          strokeWidth={EDGE_WIDTH}
          strokeLinecap="round"
          markerEnd={`url(#${MARKER_ID})`}
        />
      ))}
      {segments.map((segment) =>
        segment.label ? (
          <text
            key={`${segment.key}-label`}
            x={segment.label.x}
            y={segment.label.y}
            textAnchor="middle"
            dominantBaseline="central"
            style={{ fill: EDGE_COLOR }}
            className="text-[9px]"
          >
            {segment.label.text}
          </text>
        ) : null,
      )}
    </>
  );
}

/** Measures a set of registered elements against a container. */
function useRectRegistry(containerRef: React.RefObject<HTMLDivElement | null>) {
  const elements = useRef(new Map<string, HTMLElement>());
  const [rects, setRects] = useState(new Map<string, Rect>());

  const register = useCallback((key: string, el: HTMLElement | null) => {
    if (el) {
      elements.current.set(key, el);
    } else {
      elements.current.delete(key);
    }
  }, []);

  const measure = useCallback(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const base = container.getBoundingClientRect();
    const next = new Map<string, Rect>();
    for (const [key, el] of elements.current) {
      const r = el.getBoundingClientRect();
      next.set(key, {
        x: r.left - base.left,
        y: r.top - base.top,
        w: r.width,
        h: r.height,
      });
    }
    setRects(next);
  }, [containerRef]);

  useResizeObserverEffect(measure, containerRef, { measureOnMount: true });

  return { register, measure, rects };
}

// ---------------------------------------------------------------------------
// Renderer 1: Containment. Hierarchy as literal nesting, Fig. 1 style.
// ---------------------------------------------------------------------------

const BOX_RECT = (id: string) => `box:${id}`;
const NODE_RECT = (id: string) => `node:${id}`;
const TASK_WIDTH = 215;
/** Narrower than a containment card: six rank columns have to fit at once. */
const BAND_WIDTH = 180;

function ContainmentBox({
  graph,
  boxId,
  collapsed,
  toggle,
  internal,
  register,
}: {
  graph: HierarchyGraph;
  boxId: string;
  collapsed: ReadonlySet<string>;
  toggle: (id: string) => void;
  internal: Map<string, number>;
  register: (key: string, el: HTMLElement | null) => void;
}) {
  const box = graph.boxById.get(boxId);
  const kids = graph.childrenOf.get(boxId) ?? [];
  const isHull = kids.length > 0;
  const isOpen = isHull && !collapsed.has(boxId);
  const hiddenEdges = internal.get(boxId) ?? 0;
  const hiddenTasks = (graph.tasksUnder.get(boxId) ?? []).length;
  if (!box) {
    return null;
  }

  // A hull that is itself a task has two rects: its header is the edge
  // endpoint, its outer border is the segment boundary. For a leaf the two
  // coincide, which the routing code handles without a special case.
  //
  // Children wrap into a near-square grid so a hull with six members reads
  // as a blob rather than a column six cards tall. Tracks are `max-content`,
  // never `minmax(0, ...)`: a shrinkable track collapses the fixed-width
  // cards inside it and the hulls end up overlapping.
  const columns = kids.length > 3 ? 2 : 1;

  return (
    <div
      ref={(el) => register(BOX_RECT(boxId), el)}
      className={cn(
        "flex w-max shrink-0 flex-col rounded-lg",
        isHull ? "border border-dashed border-border bg-background-hover/30 p-2" : "",
      )}
    >
      {box.taskId ? (
        <div
          ref={(el) => register(NODE_RECT(boxId), el)}
          className={cn(
            "flex min-w-0 items-center gap-1.5 rounded-md border bg-background px-2 py-1.5",
            box.taskId === GOAL_ID ? "border-2 border-foreground" : "border-border",
            isHull && "shadow-sm",
          )}
          style={{ width: TASK_WIDTH }}
        >
          {isHull ? (
            <button
              type="button"
              onClick={() => toggle(boxId)}
              className="shrink-0 text-foreground-muted hover:text-foreground"
              aria-label={isOpen ? "Collapse hull" : "Expand hull"}
            >
              {isOpen ? (
                <ChevronBottomIcon className="h-3 w-3" />
              ) : (
                <ChevronRightIcon className="h-3 w-3" />
              )}
            </button>
          ) : null}
          <TaskChip taskId={box.taskId} />
        </div>
      ) : (
        <button
          type="button"
          ref={(el) => register(NODE_RECT(boxId), el)}
          onClick={() => toggle(boxId)}
          className="flex items-center gap-1.5 self-start rounded-sm px-1 py-0.5 text-left hover:bg-background-hover"
        >
          {isOpen ? (
            <ChevronBottomIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
          ) : (
            <ChevronRightIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
          )}
          <LayersIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
          <StrongText className="text-xs text-foreground-muted">{box.label}</StrongText>
        </button>
      )}

      {isHull && !isOpen ? (
        <MutedText className="mt-1 self-start px-1 text-[10px]">
          {hiddenTasks} task{hiddenTasks === 1 ? "" : "s"}
          {hiddenEdges > 0 ? ` · ${hiddenEdges} internal edge${hiddenEdges === 1 ? "" : "s"}` : ""}
        </MutedText>
      ) : null}

      {isOpen ? (
        <div
          className="mt-2 grid gap-x-8 gap-y-7"
          style={{ gridTemplateColumns: `repeat(${columns}, max-content)` }}
        >
          {kids.map((kid) => (
            <ContainmentBox
              key={kid}
              graph={graph}
              boxId={kid}
              collapsed={collapsed}
              toggle={toggle}
              internal={internal}
              register={register}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

/**
 * Routes one edge as the technique describes: up the source's ancestor
 * chain to the level the two sides share, once across, then down the
 * destination's chain. Every port on the way is a function of the box pair
 * alone, so the ports are shared -- that shared port is the segment.
 */
function routeSegment(
  graph: HierarchyGraph,
  rects: Map<string, Rect>,
  collapsed: ReadonlySet<string>,
  from: string,
  to: string,
): Pt[] | null {
  const rectOf = (id: string) => rects.get(BOX_RECT(id));
  /**
   * The rect an edge actually terminates on. For an expanded hull that is
   * itself a task, that is its own header -- the hull's outer border is a
   * segment boundary, not an endpoint. Everywhere else the two coincide.
   */
  const nodeOf = (id: string) => {
    const isExpandedTaskHull =
      graph.boxById.get(id)?.taskId != null && !(graph.hullIds.has(id) && collapsed.has(id));
    return isExpandedTaskHull
      ? (rects.get(NODE_RECT(id)) ?? rectOf(id))
      : (rectOf(id) ?? rects.get(NODE_RECT(id)));
  };
  const fromNode = nodeOf(from);
  const toNode = nodeOf(to);
  if (!fromNode || !toNode) {
    return null;
  }

  const upChainAll = graph.ancestorsOf.get(from) ?? [from];
  const downChainAll = graph.ancestorsOf.get(to) ?? [to];

  // Case A: one endpoint contains the other. Containment already expresses
  // the relation, so the edge only has to climb out of the boxes between
  // them and land on the enclosing hull's own header from the inside.
  const toIndexInUp = upChainAll.indexOf(to);
  if (toIndexInUp > 0) {
    return routeToAncestor(upChainAll.slice(0, toIndexInUp), fromNode, toNode, rectOf);
  }
  const fromIndexInDown = downChainAll.indexOf(from);
  if (fromIndexInDown > 0) {
    // Same shape, opposite direction: route from the inner endpoint out, then
    // reverse so the path still starts at the source and the arrowhead lands
    // on the target.
    const points = routeToAncestor(
      downChainAll.slice(0, fromIndexInDown),
      toNode,
      fromNode,
      rectOf,
    );
    return [...points].reverse();
  }

  // Case B: siblings under a shared ancestor (or under the render root).
  let upTopIndex = upChainAll.length - 1;
  let downTopIndex = downChainAll.length - 1;
  while (
    upTopIndex >= 0 &&
    downTopIndex >= 0 &&
    upChainAll[upTopIndex] === downChainAll[downTopIndex]
  ) {
    upTopIndex--;
    downTopIndex--;
  }
  const upChain = upChainAll.slice(0, upTopIndex + 1);
  const downChain = downChainAll.slice(0, downTopIndex + 1);
  const upTop = upChain[upChain.length - 1];
  const downTop = downChain[downChain.length - 1];
  if (upTop == null || downTop == null) {
    return null;
  }
  const upTopRect = rectOf(upTop);
  const downTopRect = rectOf(downTop);
  if (!upTopRect || !downTopRect) {
    return null;
  }

  const outPort = borderPoint(upTopRect, centerOf(downTopRect), 4);
  const inPort = borderPoint(downTopRect, centerOf(upTopRect), 4);

  const upPorts = portsInward(upChain, outPort, rectOf);
  const downPorts = portsInward(downChain, inPort, rectOf);
  const [firstUp] = upPorts;
  const [firstDown] = downPorts;
  if (firstUp == null || firstDown == null) {
    return null;
  }

  return [
    borderPoint(fromNode, firstUp, 2),
    ...upPorts,
    ...bowBetween(outPort, inPort),
    ...[...downPorts].reverse(),
    borderPoint(toNode, firstDown, 2),
  ];
}

/**
 * A single waypoint bowing the crossing to one consistent side. Without it
 * every segment between boxes on a shared row is collinear, so a segment
 * skipping over a neighbour is drawn straight through that neighbour, and
 * the two directions of a pair sit exactly on top of each other.
 */
function bowBetween(a: Pt, b: Pt): Pt[] {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = Math.hypot(dx, dy);
  if (len < 40) {
    return [];
  }
  const bow = Math.min(30, len * 0.14);
  return [
    {
      x: (a.x + b.x) / 2 - (dy / len) * bow,
      y: (a.y + b.y) / 2 + (dx / len) * bow,
    },
  ];
}

/**
 * Ports for a chain of boxes, computed outside in: the outermost port is
 * fixed by the crossing pair, and each inner box aims at the port its
 * parent already committed to. Matches the "A1 to A continuing to B"
 * segment chain in the disclosure.
 */
function portsInward(
  chain: string[],
  outerPort: Pt,
  rectOf: (id: string) => Rect | undefined,
): Pt[] {
  const ports: Pt[] = new Array(chain.length);
  ports[chain.length - 1] = outerPort;
  for (let i = chain.length - 2; i >= 0; i--) {
    const boxId = chain[i];
    const nextPort = ports[i + 1];
    const rect = boxId == null ? undefined : rectOf(boxId);
    if (!rect || nextPort == null) {
      ports[i] = outerPort;
      continue;
    }
    ports[i] = borderPoint(rect, nextPort, 4);
  }
  return ports;
}

/**
 * The containment case: `crossings` is the chain of boxes from the inner
 * endpoint outward, stopping just short of the hull the edge lands on. Ports
 * are still computed outside in and still keyed only by the box pair, so a
 * hull with four children all feeding it shows four lines converging on one
 * header rather than four independent routes.
 */
function routeToAncestor(
  crossings: string[],
  innerNode: Rect,
  outerNode: Rect,
  rectOf: (id: string) => Rect | undefined,
): Pt[] {
  const target = centerOf(outerNode);
  const ports: Pt[] = new Array(crossings.length);
  for (let i = crossings.length - 1; i >= 0; i--) {
    const boxId = crossings[i];
    const aim = i === crossings.length - 1 ? target : (ports[i + 1] ?? target);
    const rect = boxId == null ? undefined : rectOf(boxId);
    ports[i] = rect ? borderPoint(rect, aim, 4) : aim;
  }
  const first = ports[0] ?? target;
  const last = ports[ports.length - 1] ?? centerOf(innerNode);
  return [borderPoint(innerNode, first, 2), ...ports, borderPoint(outerNode, last, 2)];
}

function ContainmentView({
  hierarchyId,
  initiallyCollapsed,
  chrome = true,
}: {
  hierarchyId: string;
  initiallyCollapsed?: string[];
  /** Hidden in the side-by-side comparison, where the chrome would repeat. */
  chrome?: boolean;
}) {
  const graph = GRAPHS.get(hierarchyId);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set(initiallyCollapsed ?? []),
  );
  const containerRef = useRef<HTMLDivElement>(null);
  const { register, measure, rects } = useRectRegistry(containerRef);

  const toggle = useCallback((id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const segmentSet = useMemo(
    () => (graph ? computeSegments(graph, collapsed) : null),
    [graph, collapsed],
  );

  useEffect(() => {
    measure();
  }, [measure, collapsed]);

  const drawn: DrawnSegment[] = useMemo(() => {
    if (!graph || !segmentSet) {
      return [];
    }
    const out: DrawnSegment[] = [];
    for (const segment of segmentSet.segments) {
      const points = routeSegment(graph, rects, collapsed, segment.from, segment.to);
      if (!points) {
        continue;
      }
      const mid = points[Math.floor(points.length / 2)];
      out.push({
        key: segment.key,
        d: smoothPath(points),
        label:
          segment.edges.length > 1 && mid != null
            ? { x: mid.x, y: mid.y - 9, text: `${segment.edges.length}` }
            : undefined,
      });
    }
    return out;
  }, [graph, segmentSet, rects, collapsed]);

  if (!graph || !segmentSet) {
    return null;
  }

  return (
    <div className="flex flex-col gap-3">
      {chrome ? (
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => setCollapsed(new Set(graph.hullIds))}
            className="rounded-sm border border-border px-2 py-1 text-xs text-foreground-muted hover:bg-background-hover"
          >
            Collapse all hulls
          </button>
          <button
            type="button"
            onClick={() => setCollapsed(new Set())}
            className="rounded-sm border border-border px-2 py-1 text-xs text-foreground-muted hover:bg-background-hover"
          >
            Expand all hulls
          </button>
          <MutedText className="text-xs">
            {segmentSet.segments.length} visible segment
            {segmentSet.segments.length === 1 ? "" : "s"} bundling {EDGES.length} edges
          </MutedText>
        </div>
      ) : null}
      <div className="overflow-x-auto">
        <div
          ref={containerRef}
          className="relative isolate flex flex-wrap items-start gap-x-14 gap-y-12 p-4"
        >
          <svg className="pointer-events-none absolute inset-0 -z-10 h-full w-full overflow-visible">
            <ArrowDefs />
            <SegmentPaths segments={drawn} />
          </svg>
          {graph.roots.map((root) => (
            <ContainmentBox
              key={root}
              graph={graph}
              boxId={root}
              collapsed={collapsed}
              toggle={toggle}
              internal={segmentSet.internal}
              register={register}
            />
          ))}
        </div>
      </div>
      {chrome ? (
        <SegmentLegend note="Collapse a hull: its position is unchanged, only its size." />
      ) : null}
    </div>
  );
}

/**
 * The one-level-down view used to compare hierarchies: every hull closed,
 * except that a hierarchy with a single root has that root opened, since
 * otherwise the whole run is one box and there is nothing to compare.
 */
function topLevelCollapse(graph: HierarchyGraph): string[] {
  const floor = graph.roots.length === 1 ? 1 : 0;
  return [...graph.hullIds].filter((id) => (graph.depthOf.get(id) ?? 0) >= floor);
}

// ---------------------------------------------------------------------------
// Renderer 2: Bands. Hierarchy as adjacency, with the rank axis preserved.
// ---------------------------------------------------------------------------

function BandsView({ hierarchyId }: { hierarchyId: string }) {
  const graph = GRAPHS.get(hierarchyId);
  const [cutDepth, setCutDepth] = useState(() => (graph ? defaultCutDepth(graph) : 0));
  const containerRef = useRef<HTMLDivElement>(null);
  const { register, measure, rects } = useRectRegistry(containerRef);

  const lanes = useMemo(() => {
    if (!graph) {
      return [];
    }
    const laneOf = new Map<string, string>();
    for (const task of TASKS) {
      laneOf.set(task.id, ancestorAtDepth(graph, task.id, cutDepth));
    }
    const ids = [...new Set(laneOf.values())].sort(
      (a, b) => (graph.minRankOf.get(a) ?? 0) - (graph.minRankOf.get(b) ?? 0),
    );
    return ids.map((id) => ({
      id,
      label: graph.boxById.get(id)?.label ?? id,
      taskIds: TASKS.filter((task) => laneOf.get(task.id) === id)
        .map((task) => task.id)
        .sort((a, b) => (RANK.get(b) ?? 0) - (RANK.get(a) ?? 0)),
    }));
  }, [graph, cutDepth]);

  const laneOfTask = useMemo(() => {
    const map = new Map<string, string>();
    for (const lane of lanes) {
      for (const taskId of lane.taskIds) {
        map.set(taskId, lane.id);
      }
    }
    return map;
  }, [lanes]);

  useEffect(() => {
    measure();
  }, [measure, cutDepth]);

  const drawn: DrawnSegment[] = useMemo(() => {
    const bundles = new Map<string, Edge[]>();
    for (const edge of EDGES) {
      const a = laneOfTask.get(edge.from);
      const b = laneOfTask.get(edge.to);
      if (a == null || b == null) {
        continue;
      }
      const key = `${a}->${b}`;
      bundles.set(key, [...(bundles.get(key) ?? []), edge]);
    }

    const out: DrawnSegment[] = [];
    for (const [key, edges] of bundles) {
      const [laneA, laneB] = key.split("->");
      const anchors = edges
        .map((edge) => ({
          from: rects.get(NODE_RECT(edge.from)),
          to: rects.get(NODE_RECT(edge.to)),
        }))
        .filter((pair): pair is { from: Rect; to: Rect } => pair.from != null && pair.to != null);
      if (anchors.length === 0) {
        continue;
      }
      if (laneA === laneB) {
        for (const pair of anchors) {
          out.push({
            key: `${key}:${pair.from.x},${pair.from.y}`,
            d: smoothPath([
              {
                x: pair.from.x + pair.from.w,
                y: pair.from.y + pair.from.h / 2,
              },
              { x: pair.to.x, y: pair.to.y + pair.to.h / 2 },
            ]),
          });
        }
        continue;
      }

      // One bundle waypoint per lane pair: every edge between the two lanes
      // is pinched through it, which is exactly a segment drawn in a layout
      // that has no nesting to hang it on.
      const waypoint = {
        x:
          anchors.reduce((sum, pair) => sum + (pair.from.x + pair.from.w + pair.to.x) / 2, 0) /
          anchors.length,
        y:
          anchors.reduce(
            (sum, pair) => sum + (pair.from.y + pair.from.h / 2 + pair.to.y + pair.to.h / 2) / 2,
            0,
          ) / anchors.length,
      };
      for (const pair of anchors) {
        out.push({
          key: `${key}:${pair.from.x},${pair.from.y},${pair.to.y}`,
          d: smoothPath(
            [
              {
                x: pair.from.x + pair.from.w,
                y: pair.from.y + pair.from.h / 2,
              },
              waypoint,
              { x: pair.to.x, y: pair.to.y + pair.to.h / 2 },
            ],
            22,
          ),
        });
      }
      out.push({
        key: `${key}:count`,
        d: "",
        label:
          edges.length > 1
            ? { x: waypoint.x, y: waypoint.y - 8, text: `${edges.length}` }
            : undefined,
      });
    }
    return out;
  }, [laneOfTask, rects]);

  if (!graph) {
    return null;
  }

  const columns = MAX_RANK + 1;

  return (
    <div className="flex flex-col gap-3">
      <DepthStepper value={cutDepth} max={graph.maxDepth} onChange={setCutDepth} />
      <div className="overflow-x-auto">
        <div ref={containerRef} className="relative isolate min-w-max p-2">
          <svg className="pointer-events-none absolute inset-0 -z-10 h-full w-full overflow-visible">
            <ArrowDefs />
            <SegmentPaths segments={drawn} />
          </svg>
          <div className="flex flex-col gap-1">
            {lanes.map((lane, laneIndex) => (
              <div
                key={lane.id}
                className={cn(
                  "flex items-stretch gap-2 rounded-md px-2 py-2",
                  laneIndex % 2 === 0 ? "bg-background-hover/40" : "bg-background",
                )}
              >
                <div className="flex w-24 shrink-0 items-start gap-1.5 pt-1">
                  <LayersIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
                  <StrongText className="truncate text-xs text-foreground-muted">
                    {lane.label}
                  </StrongText>
                </div>
                <div
                  className="grid flex-1 gap-x-8 gap-y-2"
                  style={{
                    gridTemplateColumns: `repeat(${columns}, ${BAND_WIDTH}px)`,
                    // Without dense packing, a card in an earlier column than
                    // the one before it starts a new row, and lanes end up
                    // mostly whitespace.
                    gridAutoFlow: "row dense",
                  }}
                >
                  {lane.taskIds.map((taskId) => (
                    <div
                      key={taskId}
                      ref={(el) => register(NODE_RECT(taskId), el)}
                      className={cn(
                        "flex min-w-0 items-center rounded-md border bg-background px-2 py-1.5",
                        taskId === GOAL_ID ? "border-2 border-foreground" : "border-border",
                      )}
                      style={{
                        gridColumnStart: MAX_RANK - (RANK.get(taskId) ?? 0) + 1,
                      }}
                    >
                      <TaskChip taskId={taskId} compact={true} />
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
      <SegmentLegend note="Columns are DAG rank: information flows left to right into the goal." />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Renderer 3: Outline. Hierarchy as indentation, segments in a gutter.
// ---------------------------------------------------------------------------

/** One nesting level, in both the gutter's arc swing and the row indent. */
const OUTLINE_STEP = 22;

interface OutlineRow {
  boxId: string;
  depth: number;
  isHull: boolean;
  isCollapsed: boolean;
}

function flattenOutline(graph: HierarchyGraph, collapsed: ReadonlySet<string>): OutlineRow[] {
  const rows: OutlineRow[] = [];
  const walk = (boxId: string, depth: number) => {
    const kids = graph.childrenOf.get(boxId) ?? [];
    const isHull = kids.length > 0;
    const isCollapsed = isHull && collapsed.has(boxId);
    rows.push({ boxId, depth, isHull, isCollapsed });
    if (isHull && !isCollapsed) {
      for (const kid of kids) {
        walk(kid, depth + 1);
      }
    }
  };
  for (const root of graph.roots) {
    walk(root, 0);
  }
  return rows;
}

function OutlineView({ hierarchyId }: { hierarchyId: string }) {
  const graph = GRAPHS.get(hierarchyId);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const containerRef = useRef<HTMLDivElement>(null);
  const { register, measure, rects } = useRectRegistry(containerRef);

  useEffect(() => {
    measure();
  }, [measure, collapsed]);

  const toggle = useCallback((id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const rows = useMemo(() => (graph ? flattenOutline(graph, collapsed) : []), [graph, collapsed]);
  const segmentSet = useMemo(
    () => (graph ? computeSegments(graph, collapsed) : null),
    [graph, collapsed],
  );

  const gutterWidth = ((graph?.maxDepth ?? 0) + 2) * OUTLINE_STEP;

  const drawn: DrawnSegment[] = useMemo(() => {
    if (!graph || !segmentSet) {
      return [];
    }
    const out: DrawnSegment[] = [];
    for (const segment of segmentSet.segments) {
      const fromRect = rects.get(NODE_RECT(segment.from));
      const toRect = rects.get(NODE_RECT(segment.to));
      if (!fromRect || !toRect) {
        continue;
      }
      // Swing depth: how far out the arc bows is the depth of the hull the
      // two endpoints share. Edges inside a deep hull hug the rows; edges
      // between top-level hulls swing all the way out.
      const upChain = graph.ancestorsOf.get(segment.from) ?? [];
      const downChain = new Set(graph.ancestorsOf.get(segment.to) ?? []);
      const shared = upChain.find((id) => downChain.has(id));
      const sharedDepth = shared == null ? -1 : (graph.depthOf.get(shared) ?? 0);
      const swing = gutterWidth - (sharedDepth + 1) * OUTLINE_STEP;
      const y1 = fromRect.y + fromRect.h / 2;
      const y2 = toRect.y + toRect.h / 2;
      // Anchor on each row's own left edge so a deeply indented row is still
      // touched by its arcs, and bow from the outermost of the two.
      const x1 = fromRect.x;
      const x2 = toRect.x;
      const bowX = Math.min(x1, x2) - swing;
      out.push({
        key: segment.key,
        d: `M ${x1},${y1} C ${bowX},${y1} ${bowX},${y2} ${x2},${y2}`,
        label:
          segment.edges.length > 1
            ? {
                x: bowX + swing * 0.28,
                y: (y1 + y2) / 2,
                text: `${segment.edges.length}`,
              }
            : undefined,
      });
    }
    return out;
  }, [graph, segmentSet, rects, gutterWidth]);

  if (!graph || !segmentSet) {
    return null;
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => setCollapsed(new Set(graph.hullIds))}
          className="rounded-sm border border-border px-2 py-1 text-xs text-foreground-muted hover:bg-background-hover"
        >
          Collapse all
        </button>
        <button
          type="button"
          onClick={() => setCollapsed(new Set())}
          className="rounded-sm border border-border px-2 py-1 text-xs text-foreground-muted hover:bg-background-hover"
        >
          Expand all
        </button>
        <MutedText className="text-xs">Arc swing = depth of the hull the two ends share.</MutedText>
      </div>
      <div ref={containerRef} className="relative isolate">
        <svg className="pointer-events-none absolute inset-0 h-full w-full overflow-visible">
          <ArrowDefs />
          <SegmentPaths segments={drawn} />
        </svg>
        <div className="flex flex-col" style={{ paddingLeft: gutterWidth }}>
          {rows.map((row) => {
            const box = graph.boxById.get(row.boxId);
            if (!box) {
              return null;
            }
            return (
              <div
                key={row.boxId}
                className="flex items-center py-0.5"
                style={{ paddingLeft: row.depth * OUTLINE_STEP }}
              >
                <div
                  ref={(el) => register(NODE_RECT(row.boxId), el)}
                  className={cn(
                    "flex min-w-0 items-center gap-1.5 rounded-md border px-2 py-1",
                    box.taskId
                      ? "border-border bg-background"
                      : "border-transparent bg-transparent",
                    row.boxId === GOAL_ID && "border-2 border-foreground",
                  )}
                  style={{ width: TASK_WIDTH + 40 }}
                >
                  {row.isHull ? (
                    <button
                      type="button"
                      onClick={() => toggle(row.boxId)}
                      className="shrink-0 text-foreground-muted hover:text-foreground"
                      aria-label={row.isCollapsed ? "Expand" : "Collapse"}
                    >
                      {row.isCollapsed ? (
                        <ChevronRightIcon className="h-3 w-3" />
                      ) : (
                        <ChevronBottomIcon className="h-3 w-3" />
                      )}
                    </button>
                  ) : (
                    <span className="w-3 shrink-0" />
                  )}
                  {box.taskId ? (
                    <TaskChip taskId={box.taskId} compact={true} />
                  ) : (
                    <>
                      <LayersIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
                      <StrongText className="truncate text-[11px] text-foreground-muted">
                        {box.label}
                      </StrongText>
                    </>
                  )}
                </div>
                {row.isCollapsed ? (
                  <MutedText className="ml-2 text-[10px]">
                    {(graph.tasksUnder.get(row.boxId) ?? []).length} tasks
                    {(segmentSet.internal.get(row.boxId) ?? 0) > 0
                      ? ` · ${segmentSet.internal.get(row.boxId)} internal`
                      : ""}
                  </MutedText>
                ) : null}
              </div>
            );
          })}
        </div>
      </div>
      <SegmentLegend />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Renderer 4: SegmentMatrix. The fully-collapsed limit.
// ---------------------------------------------------------------------------

function SegmentMatrixView({ hierarchyId }: { hierarchyId: string }) {
  const graph = GRAPHS.get(hierarchyId);
  const [cutDepth, setCutDepth] = useState(() => (graph ? defaultCutDepth(graph) : 0));
  const [selected, setSelected] = useState<string | null>(null);

  const { groups, cells, maxWeight } = useMemo(() => {
    if (!graph) {
      return {
        groups: [] as string[],
        cells: new Map<string, Edge[]>(),
        maxWeight: 0,
      };
    }
    const groupOf = new Map(
      TASKS.map((task) => [task.id, ancestorAtDepth(graph, task.id, cutDepth)]),
    );
    const ids = [...new Set(groupOf.values())].sort(
      (a, b) => (graph.minRankOf.get(a) ?? 0) - (graph.minRankOf.get(b) ?? 0),
    );
    const map = new Map<string, Edge[]>();
    for (const edge of EDGES) {
      const a = groupOf.get(edge.from);
      const b = groupOf.get(edge.to);
      if (a == null || b == null) {
        continue;
      }
      const key = `${a}|${b}`;
      map.set(key, [...(map.get(key) ?? []), edge]);
    }
    return {
      groups: ids,
      cells: map,
      maxWeight: Math.max(1, ...[...map.values()].map((e) => e.length)),
    };
  }, [graph, cutDepth]);

  if (!graph) {
    return null;
  }

  const selectedEdges = selected ? (cells.get(selected) ?? []) : [];

  return (
    <div className="flex flex-col gap-3">
      <DepthStepper
        value={cutDepth}
        max={graph.maxDepth}
        onChange={(next) => {
          setCutDepth(next);
          setSelected(null);
        }}
      />
      <div className="overflow-x-auto">
        <table className="border-separate border-spacing-0.5 text-xs">
          <thead>
            <tr>
              <th className="p-1" />
              {groups.map((col) => (
                <th
                  key={col}
                  className="max-w-24 p-1 text-left align-bottom font-normal text-foreground-muted"
                >
                  <span className="block truncate text-[10px]">
                    {graph.boxById.get(col)?.label ?? col}
                  </span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {groups.map((row) => (
              <tr key={row}>
                <th className="max-w-32 p-1 text-right font-normal text-foreground-muted">
                  <span className="block truncate text-[10px]">
                    {graph.boxById.get(row)?.label ?? row}
                  </span>
                </th>
                {groups.map((col) => {
                  const key = `${row}|${col}`;
                  const edges = cells.get(key) ?? [];
                  const weight = edges.length;
                  const isDiagonal = row === col;
                  return (
                    <td key={col} className="p-0">
                      <button
                        type="button"
                        disabled={weight === 0}
                        onClick={() => setSelected(key)}
                        className={cn(
                          "flex h-8 w-full min-w-8 items-center justify-center rounded-sm border text-[11px]",
                          weight === 0 ? "border-transparent text-transparent" : "border-border",
                          selected === key && "ring-1 ring-foreground",
                          isDiagonal && weight > 0 && "border-dashed",
                        )}
                        style={
                          weight > 0
                            ? {
                                background: `color-mix(in srgb, var(--color-foreground) ${Math.round((weight / maxWeight) * 22)}%, transparent)`,
                              }
                            : undefined
                        }
                      >
                        {weight > 0 ? weight : ""}
                      </button>
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="rounded-sm border border-border bg-background-hover/40 p-2">
        {selected == null ? (
          <MutedText className="text-xs">
            Row feeds column. A solid cell is a segment between two hulls; a dashed cell on the
            diagonal is the traffic a hull keeps to itself. Pick a cell to see the edges it bundles.
          </MutedText>
        ) : (
          <div className="flex flex-col gap-1">
            <MutedText className="text-xs tracking-wide uppercase">
              {selectedEdges.length} edge
              {selectedEdges.length === 1 ? "" : "s"} in this segment
            </MutedText>
            {selectedEdges.map((edge) => (
              <div
                key={edge.id}
                className="flex items-center gap-1.5 text-xs text-foreground-muted"
              >
                <span className="truncate">{TASK_BY_ID.get(edge.from)?.label}</span>
                <ChevronRightIcon className="h-3 w-3 shrink-0" />
                <span className="truncate">{TASK_BY_ID.get(edge.to)?.label}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Stories.
// ---------------------------------------------------------------------------

function Frame({
  children,
  intro,
  hierarchyId,
  onHierarchyChange,
}: {
  children: React.ReactNode;
  intro: React.ReactNode;
  hierarchyId: string;
  onHierarchyChange: (id: string) => void;
}) {
  return (
    <div className="mx-auto flex max-w-[1500px] flex-col gap-4 bg-background p-6">
      <BodyText className="text-xs text-foreground-muted">{intro}</BodyText>
      <HierarchyPicker value={hierarchyId} onChange={onHierarchyChange} />
      {children}
    </div>
  );
}

/**
 * Hierarchy as literal nesting. This is Fig. 1 of the disclosure applied to
 * the task DAG: hulls contain their descendants, and an edge leaving a hull
 * is routed up to that hull's border, across to its peer, and back down --
 * meeting every other edge that makes the same crossing at a shared port.
 */
export function Containment() {
  const [hierarchyId, setHierarchyId] = useState("deliverable");
  return (
    <Frame
      hierarchyId={hierarchyId}
      onHierarchyChange={setHierarchyId}
      intro={
        <>
          The same 17 tasks and 20 edges as sketch 7, drawn inside a containment tree. Nothing about
          the topology changed; what changed is that an edge crossing a hull boundary now leaves
          through a port shared with every other edge making the same crossing. Collapse a hull and
          its edges do not disappear &mdash; they merge into the parent&apos;s segments, and the
          count of internal edges it swallowed is printed on the hull.{" "}
          <StrongText className="text-xs">Try Task Tree: </StrongText>because that hierarchy is the
          spawn tree, every line just runs from a task up to the header of a hull that already
          contains it &mdash; parentage restated as geometry. The only lines crossing sideways
          between hulls are the four reuse edges, which is the wiring a reader could not have
          predicted from the nesting alone.
        </>
      }
    >
      <ContainmentView key={hierarchyId} hierarchyId={hierarchyId} />
    </Frame>
  );
}

/**
 * The collapsed-first reading. Every hull starts closed, so the picture is
 * the top level only; expanding one changes that hull's size but not its
 * position, which is the property the disclosure is built around.
 */
export function CollapsedFirst() {
  const [hierarchyId, setHierarchyId] = useState("session");
  const graph = GRAPHS.get(hierarchyId);
  return (
    <Frame
      hierarchyId={hierarchyId}
      onHierarchyChange={setHierarchyId}
      intro={
        <>
          Every hull starts collapsed, so the first thing on screen is the shape of the whole run
          rather than 17 cards. Under the <StrongText className="text-xs">Session </StrongText>
          hierarchy each hull is one long-running agent, so each segment is something one agent had
          to tell another and each internal count is work an agent finished without leaving its own
          session. Expanding a hull grows it in place; the other hulls do not move, and the segments
          it was part of split into the finer segments underneath.
        </>
      }
    >
      <ContainmentView
        key={hierarchyId}
        hierarchyId={hierarchyId}
        initiallyCollapsed={graph ? topLevelCollapse(graph) : []}
      />
    </Frame>
  );
}

/**
 * The limit case. Collapse everything and the drawing degenerates into the
 * segment weights themselves -- which is the form that keeps working when
 * there are more hulls than a diagram can hold.
 */
export function SegmentMatrix() {
  const [hierarchyId, setHierarchyId] = useState("path");
  return (
    <Frame
      hierarchyId={hierarchyId}
      onHierarchyChange={setHierarchyId}
      intro={
        <>
          What is left of the picture when every hull is closed: rows feed columns, and the cell is
          the segment weight. No node positions, no routing, no crossing lines -- and it stays
          readable at a hull count where the drawn forms will not. Under{" "}
          <StrongText className="text-xs">Session </StrongText>this is a handoff ledger; under{" "}
          <StrongText className="text-xs">Path </StrongText>it is a coupling matrix over the
          repository.
        </>
      }
    >
      <SegmentMatrixView key={hierarchyId} hierarchyId={hierarchyId} />
    </Frame>
  );
}

/**
 * The four candidate hierarchies side by side, all collapsed to one level,
 * so the structures can be compared rather than the renderings.
 */
export function HierarchyVariants() {
  return (
    <div className="mx-auto flex max-w-[1500px] flex-col gap-6 bg-background p-6">
      <BodyText className="text-xs text-foreground-muted">
        One topology, four containment trees. A DAG does not come with a hierarchy, so each of these
        is a choice, and each choice needs a tie-break for tasks with more than one consumer. Read
        the segments as the cost of the choice: they are the edges the hierarchy failed to localize.
      </BodyText>
      {HIERARCHIES.map((hierarchy) => {
        const graph = GRAPHS.get(hierarchy.id);
        const cut = graph ? topLevelCollapse(graph) : [];
        const summary = graph ? computeSegments(graph, new Set(cut)) : null;
        const internalTotal = summary
          ? [...summary.internal.values()].reduce((a, b) => a + b, 0)
          : 0;
        return (
          <div key={hierarchy.id} className="flex flex-col gap-2 border-t border-border pt-4">
            <div className="flex flex-wrap items-baseline gap-2">
              <StrongText className="text-sm">{hierarchy.name}</StrongText>
              <MutedText className="text-xs">
                {graph?.hullIds.size ?? 0} hulls, depth {graph?.maxDepth ?? 0}
              </MutedText>
              <Badge color={BadgeColor.METAL} textSize="sm">
                {summary?.segments.length ?? 0} segments at this cut
              </Badge>
              <Badge color={BadgeColor.MINT} textSize="sm">
                {internalTotal} of {EDGES.length} edges localized
              </Badge>
            </div>
            <BodyText className="text-xs text-foreground-muted">{hierarchy.blurb}</BodyText>
            <BodyText className="text-xs text-foreground-muted">
              <StrongText className="text-xs">Tie-break: </StrongText>
              {hierarchy.rule}
            </BodyText>
            <ContainmentView
              key={hierarchy.id}
              hierarchyId={hierarchy.id}
              initiallyCollapsed={cut}
              chrome={false}
            />
          </div>
        );
      })}
      <SegmentLegend />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Rejected renderers. Kept, not deleted: the reason each one fails is a
// constraint on whatever replaces it, and that is only legible next to the
// thing it failed at.
// ---------------------------------------------------------------------------

function RejectedBanner({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-2 rounded-sm border border-status-error/40 bg-status-error/5 p-2">
      <CircleSlashIcon className="mt-0.5 h-3.5 w-3.5 shrink-0 text-status-error" />
      <div className="flex flex-col gap-1">
        <StrongText className="text-xs text-status-error">Rejected</StrongText>
        <BodyText className="text-xs text-foreground-muted">{children}</BodyText>
      </div>
    </div>
  );
}

/**
 * REJECTED. Hierarchy as adjacency instead of nesting, which buys back the
 * one thing containment gives up: a global rank axis.
 *
 * Rejected on two counts. The diagram's area is lanes x ranks while its
 * content stays at 17 cards, so most of it is whitespace, and the emptiness
 * grows with every hull added. And with no hull boundary to cross, a
 * "segment" here is a synthetic waypoint rather than a real port, so the
 * form cannot express nesting at all -- the cut depth flattens the tree to
 * one level and the deeper structure is simply gone.
 */
export function BandsRejected() {
  const [hierarchyId, setHierarchyId] = useState("session");
  return (
    <Frame
      hierarchyId={hierarchyId}
      onHierarchyChange={setHierarchyId}
      intro={
        <>
          Lanes are the hierarchy cut at a fixed depth; columns are DAG rank, so the flow still
          reads left to right into the goal. There is no nesting to hang a port on, so each pair of
          lanes gets one bundle waypoint and every edge between them is pinched through it. Raise
          the cut depth to split lanes into their sub-hulls.
        </>
      }
    >
      <RejectedBanner>
        The picture is mostly empty and gets emptier as hulls are added: its area is lanes &times;
        ranks while the content stays at 17 cards, because a lane only occupies the ranks it happens
        to own. Worse, there is no hull boundary to cross, so the bundle point is invented rather
        than derived &mdash; which means this form cannot draw nesting at all. The cut-depth control
        is not a feature here, it is the admission: the tree is flattened to one level and
        everything below it is discarded.
      </RejectedBanner>
      <BandsView key={hierarchyId} hierarchyId={hierarchyId} />
    </Frame>
  );
}

BandsRejected.storyName = "Bands (rejected)";

/**
 * REJECTED. Hierarchy as indentation, geometry reduced to a gutter. Fits a
 * side panel and needs no layout algorithm.
 *
 * Rejected because the gutter is one axis carrying every segment: arcs pile
 * onto each other as soon as the graph is more than trivially connected, and
 * two arcs that share endpoints are indistinguishable. It also drops the
 * DAG's direction of flow -- row order is tree order, not rank order, so a
 * reader cannot see what feeds what without tracing each arc by hand.
 */
export function OutlineRejected() {
  const [hierarchyId, setHierarchyId] = useState("path");
  return (
    <Frame
      hierarchyId={hierarchyId}
      onHierarchyChange={setHierarchyId}
      intro={
        <>
          The narrow-panel form. Indentation carries the hierarchy and the arcs carry the topology,
          with each arc bowing out in proportion to how far up the tree its two ends have to reach
          to meet. Under <StrongText className="text-xs">Path </StrongText>the hierarchy is the
          repository layout, which has nothing to do with the DAG, so the arcs are long and
          cross-cutting.
        </>
      }
    >
      <RejectedBanner>
        Every segment has to fit in one vertical gutter, so at 20 edges the arcs already overlap
        into a single smear and there is no way to follow one to its other end. The arc swing
        encodes the depth of the shared hull, which is a second thing to decode on top of that. And
        row order is tree order, not rank order, so the direction of the work &mdash; the thing
        sketch 7 made legible &mdash; is no longer visible anywhere on the page.
      </RejectedBanner>
      <OutlineView key={hierarchyId} hierarchyId={hierarchyId} />
    </Frame>
  );
}

OutlineRejected.storyName = "Outline (rejected)";
