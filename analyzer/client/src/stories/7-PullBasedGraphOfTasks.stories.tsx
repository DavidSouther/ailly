/**
 * (c) Copyright 2026 Nominal Inc. All rights reserved.
 */

import type { Meta } from "@storybook/react";
import {
  CheckCircle2Icon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleDotIcon,
  CornerDownRightIcon,
  CornerUpLeftIcon,
  GitMergeIcon,
  HourglassIcon,
  Loader2Icon,
  OctagonAlertIcon,
  TargetIcon,
  WrenchIcon,
} from "lucide-react";
import React, { useCallback, useRef, useState } from "react";
import { BadgeColor } from "../ui/colors";
import { useResizeObserverEffect } from "../ui/hooks/useResizeObserverEffect";

import { Badge } from "../ui/badges/badge";
import type { IconType } from "../ui/icons";
import { BodyText, MutedText, StrongText } from "../ui/typography";
import { cn } from "../ui/utils";

/**
 * A sketch of the "Pull-Based Graph of Tasks" pattern from the Graph of
 * Actions design doc's "Where next?" section, refined through direct
 * back-and-forth into a specific model:
 *
 * - The graph is a DAG with exactly one sink: the goal. A node's outgoing
 *   edge points toward whatever it supports; the goal has no outgoing edge.
 * - A node is born with a prompt and one outgoing edge to its requester,
 *   and no inputs. An agent claims it and runs a normal forward pass.
 * - Mid-pass, when it needs something, it searches existing nodes for one
 *   that already answers it (or something substantially similar). Found ->
 *   wire a new incoming edge to that node and keep going. Not found ->
 *   spawn a new node and pause.
 * - Tool calls (reads, greps, etc.) are not a separate input type -- they
 *   are ordinary nodes, just rendered smaller and muted here (de-emphasized
 *   peer nodes, not collapsed away).
 * - States: Ready (all inputs, including zero, have output -- this also
 *   covers a brand-new, never-claimed node, which is why there's no
 *   separate "Requested" state), Running (an agent is actively working
 *   it), Waiting (at least one input isn't Finished yet), Finished (has
 *   output for whatever's waiting on it).
 * - The DAG guard: a proposed reuse edge that would close a cycle back to
 *   an ancestor is rejected; the agent falls back to spawning a fresh node.
 *
 * Two variants dramatize the two outcomes of the discovery mechanism on the
 * same "migrate the legacy auth service" scenario used by the sibling
 * sketches: `Default` shows a successful reuse, `CycleRejection` shows a
 * proposed reuse that gets rejected.
 */
const meta: Meta = {
  title: "Examples/Agent Harness Sketches/7. Pull-Based Graph of Tasks",
};

export default meta;

type NodeState = "ready" | "running" | "waiting" | "finished";
type NodeKind = "task" | "tool";

interface GraphNode {
  id: string;
  label: string;
  kind: NodeKind;
  state: NodeState;
  /** Ids of nodes this node needs output from -- its incoming edges. */
  needs: string[];
  /** Subset of `needs` to draw as a highlighted (newly reused) edge. */
  highlightNeeds?: string[];
  /** Id of a node this one considered reusing, rejected because it would close a cycle. */
  rejectedEdge?: string;
  note?: { tone: "info" | "warning"; text: string };
}

interface StateMeta {
  label: string;
  icon: IconType;
  badgeColor: BadgeColor;
  iconColor?: string;
}

const STATE_META: Record<NodeState, StateMeta> = {
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

/**
 * Rank = longest path from this node to the goal, following "feeds" edges
 * (the reverse of `needs`). The goal is rank 0; everything else is ranked
 * strictly higher than every node that depends on it, so a node always
 * lays out further from the goal than its consumers -- purely derived from
 * the data, never hand-assigned.
 */
function computeRanks(nodes: GraphNode[], goalId: string): Map<string, number> {
  const feeds = new Map<string, string[]>();
  for (const node of nodes) {
    for (const needId of node.needs) {
      feeds.set(needId, [...(feeds.get(needId) ?? []), node.id]);
    }
  }

  const rank = new Map<string, number>([[goalId, 0]]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const node of nodes) {
      if (node.id === goalId || rank.has(node.id)) {
        continue;
      }
      const consumers = feeds.get(node.id) ?? [];
      const consumerRanks = consumers.map((id) => rank.get(id));
      if (consumers.length === 0 || consumerRanks.some((r) => r === undefined)) {
        continue;
      }
      rank.set(node.id, 1 + Math.max(...(consumerRanks as number[])));
      changed = true;
    }
  }
  return rank;
}

const REUSE_NODES: GraphNode[] = [
  {
    id: "goal",
    label: "Migrate the legacy auth service",
    kind: "task",
    state: "waiting",
    needs: ["review", "runbook", "pr-description"],
  },
  {
    id: "review",
    label: "Review the diff",
    kind: "task",
    state: "waiting",
    needs: ["migrate"],
  },
  {
    id: "runbook",
    label: "Update the runbook",
    kind: "task",
    state: "running",
    needs: ["locate"],
    highlightNeeds: ["locate"],
    note: {
      tone: "info",
      text: 'Needed the current call sites of LegacySession. Found "Locate call sites of LegacySession" already answered this and reused it instead of asking again.',
    },
  },
  {
    id: "pr-description",
    label: "Draft the PR description",
    kind: "task",
    state: "ready",
    needs: [],
  },
  {
    id: "migrate",
    label: "Migrate session module & add tests",
    kind: "task",
    state: "waiting",
    needs: ["locate", "read-session"],
  },
  {
    id: "locate",
    label: "Locate call sites of LegacySession",
    kind: "task",
    state: "finished",
    needs: ["grep-scout", "grep-trpc"],
  },
  {
    id: "read-session",
    label: "Read packages/auth/src/session.ts",
    kind: "tool",
    state: "running",
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

const CYCLE_NODES: GraphNode[] = [
  {
    id: "goal",
    label: "Migrate the legacy auth service",
    kind: "task",
    state: "waiting",
    needs: ["review", "pr-description"],
  },
  {
    id: "review",
    label: "Review the diff",
    kind: "task",
    state: "waiting",
    needs: ["migrate", "runbook"],
  },
  {
    id: "pr-description",
    label: "Draft the PR description",
    kind: "task",
    state: "ready",
    needs: [],
  },
  {
    id: "migrate",
    label: "Migrate session module & add tests",
    kind: "task",
    state: "finished",
    needs: ["locate"],
  },
  {
    id: "runbook",
    label: "Update the runbook",
    kind: "task",
    state: "running",
    needs: ["summarize-diff"],
    rejectedEdge: "review",
    note: {
      tone: "warning",
      text: 'Considered reusing "Review the diff" for a plain description of the code changes -- rejected. Review the diff already depends on Update the runbook, so reusing it here would close a cycle. Spawned "Summarize the diff" instead.',
    },
  },
  {
    id: "locate",
    label: "Locate call sites of LegacySession",
    kind: "task",
    state: "finished",
    needs: ["grep-scout"],
  },
  {
    id: "summarize-diff",
    label: "Summarize the diff",
    kind: "task",
    state: "ready",
    needs: [],
  },
  {
    id: "grep-scout",
    label: 'grep -l "LegacySession" apps/scout',
    kind: "tool",
    state: "finished",
    needs: [],
  },
];

function NodeCard({
  node,
  goalId,
  byId,
  registerRef,
}: {
  node: GraphNode;
  goalId: string;
  byId: Map<string, GraphNode>;
  registerRef: (id: string, el: HTMLDivElement | null) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const meta = STATE_META[node.state];
  const isTool = node.kind === "tool";
  const isGoal = node.id === goalId;
  const feeds = [...byId.values()].filter((n) => n.needs.includes(node.id));

  return (
    <div
      ref={(el) => registerRef(node.id, el)}
      className={cn(
        "flex flex-col rounded-md border bg-background",
        isTool ? "w-52 opacity-80" : "w-64",
        isGoal && "border-2 border-foreground",
      )}
    >
      <button
        type="button"
        onClick={() => setExpanded((prev) => !prev)}
        className={cn(
          "flex items-center gap-1.5 px-2.5 text-left hover:bg-background-hover",
          isTool ? "py-1.5" : "py-2",
        )}
      >
        {isGoal ? (
          <TargetIcon className="h-3.5 w-3.5 shrink-0 text-foreground" />
        ) : isTool ? (
          <WrenchIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
        ) : null}
        <StrongText className={cn("flex-1 truncate", isTool ? "text-xs" : "text-sm")}>
          {node.label}
        </StrongText>
        {expanded ? (
          <ChevronDownIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
        ) : (
          <ChevronRightIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
        )}
      </button>
      <div className={cn("px-2.5", isTool ? "pb-1.5" : "pb-2")}>
        <Badge color={meta.badgeColor} textSize="sm" icon={meta.icon} iconColor={meta.iconColor}>
          {meta.label}
        </Badge>
      </div>
      {expanded ? (
        <div className="flex flex-col gap-2 border-t border-border bg-background-hover/40 px-2.5 py-2">
          {node.needs.length > 0 ? (
            <div>
              <MutedText className="text-xs tracking-wide uppercase">Needs</MutedText>
              <ul className="mt-1 flex flex-col gap-1">
                {node.needs.map((id) => {
                  const input = byId.get(id);
                  if (!input) {
                    return null;
                  }
                  const inputMeta = STATE_META[input.state];
                  return (
                    <li key={id} className="flex items-center gap-1.5 text-xs">
                      <CornerUpLeftIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
                      <span className="min-w-0 flex-1 truncate text-foreground">{input.label}</span>
                      <Badge
                        color={inputMeta.badgeColor}
                        textSize="sm"
                        icon={inputMeta.icon}
                        iconColor={inputMeta.iconColor}
                        className="shrink-0"
                      >
                        {inputMeta.label}
                      </Badge>
                    </li>
                  );
                })}
              </ul>
            </div>
          ) : (
            <MutedText className="text-xs">
              No inputs -- ready as soon as an agent claims it.
            </MutedText>
          )}
          {feeds.length > 0 ? (
            <div>
              <MutedText className="text-xs tracking-wide uppercase">Feeds</MutedText>
              <ul className="mt-1 flex flex-col gap-1">
                {feeds.map((consumer) => (
                  <li key={consumer.id} className="flex items-center gap-1.5 text-xs">
                    <CornerDownRightIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
                    <span className="truncate text-foreground">{consumer.label}</span>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
          {node.note ? (
            <div
              className={cn(
                "flex items-start gap-1.5 rounded-sm border p-1.5 text-xs",
                node.note.tone === "warning"
                  ? "border-status-error/40 bg-status-error/5 text-status-error"
                  : "border-border bg-background text-foreground-muted",
              )}
            >
              {node.note.tone === "warning" ? (
                <OctagonAlertIcon className="mt-0.5 h-3 w-3 shrink-0" />
              ) : (
                <GitMergeIcon className="mt-0.5 h-3 w-3 shrink-0" />
              )}
              <span>{node.note.text}</span>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

interface EdgePath {
  key: string;
  d: string;
  tone: "normal" | "highlight" | "rejected";
}

function curvePath(x1: number, y1: number, x2: number, y2: number): string {
  const midY = (y1 + y2) / 2;
  return `M ${x1},${y1} C ${x1},${midY} ${x2},${midY} ${x2},${y2}`;
}

const EDGE_MARKER = {
  normal: "graph-arrow-normal",
  highlight: "graph-arrow-highlight",
  rejected: "graph-arrow-rejected",
};

/**
 * Set inline rather than via Tailwind's `stroke-*`/`fill-*` utilities:
 * these color tokens are used elsewhere as `text-*`/`bg-*`, but the
 * `fill`/`stroke` variants weren't present in this file's generated CSS,
 * and SVG's default `stroke`/`fill` is `none` -- a missing utility here
 * fails silently as an invisible line rather than an unstyled one.
 */
const TONE_COLOR: Record<EdgePath["tone"], string> = {
  normal: "var(--color-foreground-muted)",
  highlight: "var(--color-foreground)",
  rejected: "var(--color-status-error)",
};

function GraphView({ nodes, goalId }: { nodes: GraphNode[]; goalId: string }) {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const rank = computeRanks(nodes, goalId);
  const maxRank = Math.max(...nodes.map((node) => rank.get(node.id) ?? 0));

  const rows: GraphNode[][] = [];
  for (let r = 0; r <= maxRank; r++) {
    rows.push(nodes.filter((node) => rank.get(node.id) === r));
  }

  const containerRef = useRef<HTMLDivElement>(null);
  const cardRefs = useRef(new Map<string, HTMLDivElement>());
  const [edges, setEdges] = useState<EdgePath[]>([]);

  const registerRef = useCallback((id: string, el: HTMLDivElement | null) => {
    if (el) {
      cardRefs.current.set(id, el);
    } else {
      cardRefs.current.delete(id);
    }
  }, []);

  const measure = useCallback(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const containerRect = container.getBoundingClientRect();
    const anchorTop = (id: string) => {
      const el = cardRefs.current.get(id);
      if (!el) {
        return null;
      }
      const r = el.getBoundingClientRect();
      return {
        x: r.left - containerRect.left + r.width / 2,
        y: r.top - containerRect.top,
      };
    };
    const anchorBottom = (id: string) => {
      const el = cardRefs.current.get(id);
      if (!el) {
        return null;
      }
      const r = el.getBoundingClientRect();
      return {
        x: r.left - containerRect.left + r.width / 2,
        y: r.bottom - containerRect.top,
      };
    };

    const next: EdgePath[] = [];
    for (const node of nodes) {
      // An edge goes from a supplier (further from the goal, lower on the
      // page) up into its consumer (closer to the goal, higher on the
      // page): supplier's top anchor to consumer's bottom anchor.
      for (const needId of node.needs) {
        const source = anchorTop(needId);
        const target = anchorBottom(node.id);
        if (!source || !target) {
          continue;
        }
        next.push({
          key: `${needId}->${node.id}`,
          d: curvePath(source.x, source.y, target.x, target.y),
          tone: node.highlightNeeds?.includes(needId) ? "highlight" : "normal",
        });
      }
      if (node.rejectedEdge) {
        const source = anchorTop(node.id);
        const target = anchorBottom(node.rejectedEdge);
        if (source && target) {
          next.push({
            key: `${node.id}-rejected->${node.rejectedEdge}`,
            d: curvePath(source.x, source.y, target.x, target.y),
            tone: "rejected",
          });
        }
      }
    }
    setEdges(next);
  }, [nodes]);

  useResizeObserverEffect(measure, containerRef, { measureOnMount: true });

  return (
    <div
      ref={containerRef}
      className="relative isolate flex flex-col items-center"
      style={{ gap: "3rem" }}
    >
      <svg className="pointer-events-none absolute inset-0 -z-10 h-full w-full">
        <defs>
          <marker
            id={EDGE_MARKER.normal}
            viewBox="0 0 10 10"
            refX="10"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto-start-reverse"
          >
            <path d="M0,0 L10,5 L0,10 Z" style={{ fill: TONE_COLOR.normal }} />
          </marker>
          <marker
            id={EDGE_MARKER.highlight}
            viewBox="0 0 10 10"
            refX="10"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto-start-reverse"
          >
            <path d="M0,0 L10,5 L0,10 Z" style={{ fill: TONE_COLOR.highlight }} />
          </marker>
          <marker
            id={EDGE_MARKER.rejected}
            viewBox="0 0 10 10"
            refX="10"
            refY="5"
            markerWidth="6"
            markerHeight="6"
            orient="auto-start-reverse"
          >
            <path d="M0,0 L10,5 L0,10 Z" style={{ fill: TONE_COLOR.rejected }} />
          </marker>
        </defs>
        {edges.map((edge) => (
          <path
            key={edge.key}
            d={edge.d}
            fill="none"
            style={{ stroke: TONE_COLOR[edge.tone] }}
            strokeOpacity={edge.tone === "normal" ? 0.5 : 1}
            strokeWidth={edge.tone === "normal" ? 1.5 : 2}
            strokeDasharray={edge.tone === "rejected" ? "4 4" : undefined}
            markerEnd={`url(#${EDGE_MARKER[edge.tone]})`}
          />
        ))}
      </svg>
      {rows.map((row, index) => (
        <div key={index} className="flex flex-wrap justify-center gap-3">
          {row.map((node) => (
            <NodeCard
              key={node.id}
              node={node}
              goalId={goalId}
              byId={byId}
              registerRef={registerRef}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

export function Default() {
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 bg-background p-6">
      <BodyText className="text-xs text-foreground-muted">
        A DAG with one sink: the goal, at the top. Every other node's edge points toward whatever it
        supports. Watch <StrongText className="text-xs">Update the runbook</StrongText> -- while
        running, it needed the current call sites of LegacySession, found{" "}
        <StrongText className="text-xs">Locate call sites</StrongText> already answered that, and
        wired a new edge to reuse it instead of asking again. That node now feeds two consumers.
        Tool calls (the smaller, muted cards) are ordinary nodes in the same graph, not a separate
        category.
      </BodyText>
      <GraphView nodes={REUSE_NODES} goalId="goal" />
    </div>
  );
}

export function CycleRejection() {
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-4 bg-background p-6">
      <BodyText className="text-xs text-foreground-muted">
        The same mechanism, the other outcome.{" "}
        <StrongText className="text-xs">Update the runbook</StrongText> needed a description of the
        code changes and considered reusing{" "}
        <StrongText className="text-xs">Review the diff</StrongText> -- but Review the diff already
        depends on Update the runbook, so reusing it would close a cycle back to itself. The DAG
        guard rejects the edge, and the agent falls back to spawning a fresh node,{" "}
        <StrongText className="text-xs">Summarize the diff</StrongText>, instead. Expand Update the
        runbook to see the rejection recorded.
      </BodyText>
      <GraphView nodes={CYCLE_NODES} goalId="goal" />
    </div>
  );
}
