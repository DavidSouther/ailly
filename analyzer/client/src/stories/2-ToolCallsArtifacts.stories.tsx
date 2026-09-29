/**
 * (c) Copyright 2026 Nominal Inc. All rights reserved.
 */

import type { Meta } from "@storybook/react";
import {
  BotIcon,
  CheckIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleIcon,
  FileCodeIcon,
  PanelRightOpenIcon,
  WrenchIcon,
} from "lucide-react";
import type React from "react";
import { useState } from "react";

import { CollapsibleResizePanel } from "../ui/CollapsibleResizePanel";
import { PanelTabs, PanelTabsContent, PanelTabsList, PanelTabsTrigger } from "../ui/PanelTabs";
import { Avatar, AvatarFallback, AvatarRoot } from "../ui/images";
import { BodyText, MutedText, StrongText } from "../ui/typography";
import { cn } from "../ui/utils";

/**
 * Static sketch of "Era 2: Tool Calls & Artifacts" (circa 2024) from the
 * Graph of Actions design doc.
 *
 * The base is still the same linear chat transcript as the previous era,
 * but the LLM can now produce artifacts that live outside the conversation
 * (shown here in a side panel). Tool calls render as small, collapsed,
 * low-emphasis chips that scroll past quickly rather than first-class UI.
 * TODO / plan updates are NOT a persistent tracked widget yet -- every
 * update is its own fresh chat message dropped into the transcript, which
 * is deliberately noisy: that's the point of this era.
 */
const meta: Meta = {
  title: "Examples/Agent Harness Sketches/2. Tool Calls & Artifacts",
};

export default meta;

interface TodoEntry {
  label: string;
  done: boolean;
}

type TranscriptItem =
  | { kind: "user"; id: string; text: string }
  | { kind: "assistant"; id: string; text: string }
  | { kind: "tool-call"; id: string; label: string; detail: string }
  | { kind: "todo-update"; id: string; heading: string; items: TodoEntry[] }
  | { kind: "artifact-ref"; id: string; text: string; fileName: string };

const ARTIFACT = {
  fileName: "src/telemetry/loader.ts",
  code: `export async function* loadTelemetryChunks(
  source: ReadableStream<Uint8Array>,
  chunkSize = 64 * 1024,
): AsyncGenerator<TelemetryChunk> {
  const reader = source.getReader();
  let buffer = new Uint8Array(0);

  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer = concat(buffer, value);
    while (buffer.length >= chunkSize) {
      yield decodeChunk(buffer.subarray(0, chunkSize));
      buffer = buffer.subarray(chunkSize);
    }
  }
  if (buffer.length > 0) yield decodeChunk(buffer);
}`,
  description:
    "Replaces the old loadTelemetryFile(path) helper, which buffered the " +
    "entire file into memory before parsing. The new loadTelemetryChunks " +
    "generator streams fixed-size chunks as they arrive, so callers can " +
    "start processing before the transfer finishes. Two call sites " +
    "(ingest worker, replay panel) still need to switch to the async " +
    "iterable form.",
};

const TRANSCRIPT: TranscriptItem[] = [
  {
    kind: "user",
    id: "u1",
    text:
      "Can you refactor the telemetry loader to stream chunks instead of " +
      "reading the whole file into memory?",
  },
  {
    kind: "assistant",
    id: "a1",
    text: "Sure -- let me look at the current loader first, then sketch a " + "streaming version.",
  },
  {
    kind: "tool-call",
    id: "t1",
    label: 'Ran tool: search_codebase("loadTelemetryFile")',
    detail:
      "3 matches:\n" +
      "  apps/scout/client/telemetry/loader.ts:18\n" +
      "  apps/scout/client/telemetry/ingest-worker.ts:42\n" +
      "  apps/scout/client/telemetry/replay-panel.tsx:97",
  },
  {
    kind: "todo-update",
    id: "d1",
    heading: "Updated plan",
    items: [
      { label: "Read loader.ts", done: true },
      { label: "Draft streaming implementation", done: false },
      { label: "Update call sites", done: false },
    ],
  },
  {
    kind: "assistant",
    id: "a2",
    text:
      "Found it -- loadTelemetryFile reads the entire buffer up front. " +
      "Drafting a streaming rewrite now.",
  },
  {
    kind: "artifact-ref",
    id: "r1",
    text: "Created loader.ts -- a chunked, async-iterable rewrite of the loader.",
    fileName: ARTIFACT.fileName,
  },
  {
    kind: "tool-call",
    id: "t2",
    label: 'Ran tool: run_tests("loader.test.ts")',
    detail: "12 passed, 0 failed (312ms)",
  },
  {
    kind: "todo-update",
    id: "d2",
    heading: "Updated plan",
    items: [
      { label: "Read loader.ts", done: true },
      { label: "Draft streaming implementation", done: true },
      { label: "Update call sites", done: false },
    ],
  },
  {
    kind: "assistant",
    id: "a3",
    text:
      "Tests pass. Next I'll update the two call sites that assume the " +
      "old, buffered signature.",
  },
];

function AssistantAvatar() {
  return (
    <AvatarRoot size={28} border={true}>
      <AvatarFallback size={28}>
        <BotIcon className="h-3.5 w-3.5 text-foreground-muted" />
      </AvatarFallback>
    </AvatarRoot>
  );
}

function TurnRow({
  role = "Assistant",
  avatar,
  children,
}: {
  role?: string;
  avatar: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex gap-3 py-3">
      {avatar}
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <StrongText className="text-xs text-foreground-muted">{role}</StrongText>
        {children}
      </div>
    </div>
  );
}

function ToolCallChip({
  item,
  expanded,
  onToggle,
}: {
  item: { label: string; detail: string };
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <div className="py-1 pl-9">
      <button
        type="button"
        onClick={onToggle}
        className="flex items-center gap-1.5 rounded border border-transparent px-1.5 py-0.5 text-xs text-foreground-muted hover:border-border hover:bg-background-hover"
      >
        {expanded ? (
          <ChevronDownIcon className="h-3 w-3" />
        ) : (
          <ChevronRightIcon className="h-3 w-3" />
        )}
        <WrenchIcon className="h-3 w-3" />
        <span className="font-mono">{item.label}</span>
      </button>
      {expanded ? (
        <pre className="mt-1 ml-1 max-w-md rounded border bg-background-hover p-2 text-xs whitespace-pre-wrap text-foreground-muted">
          {item.detail}
        </pre>
      ) : null}
    </div>
  );
}

function TodoUpdateMessage({
  heading,
  items,
}: {
  heading: string;
  items: TodoEntry[];
}) {
  return (
    <TurnRow avatar={<AssistantAvatar />}>
      <div className="max-w-md rounded border bg-background-hover p-2">
        <MutedText className="text-xs font-medium tracking-wide uppercase">{heading}</MutedText>
        <ul className="mt-1 flex flex-col gap-1">
          {items.map((entry) => (
            <li key={entry.label} className="flex items-center gap-1.5 text-sm">
              {entry.done ? (
                <CheckIcon className="h-3 w-3 shrink-0 text-status-success" />
              ) : (
                <CircleIcon className="h-3 w-3 shrink-0 text-foreground-muted" />
              )}
              <span className={cn(entry.done && "text-foreground-muted line-through")}>
                {entry.label}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </TurnRow>
  );
}

function ArtifactRefCard({
  text,
  fileName,
}: {
  text: string;
  fileName: string;
}) {
  return (
    <TurnRow avatar={<AssistantAvatar />}>
      <BodyText>{text}</BodyText>
      <div className="flex max-w-md items-center gap-2 rounded border bg-background px-2 py-1.5">
        <FileCodeIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        <span className="truncate font-mono text-xs text-foreground">{fileName}</span>
        <span className="ml-auto shrink-0 text-xs text-foreground-muted">Open in panel &rarr;</span>
      </div>
    </TurnRow>
  );
}

function Transcript() {
  const [expandedToolIds, setExpandedToolIds] = useState<Set<string>>(() => new Set());

  const toggleTool = (id: string) => {
    setExpandedToolIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  return (
    <div className="flex flex-col divide-y divide-border">
      {TRANSCRIPT.map((item) => {
        switch (item.kind) {
          case "user":
            return (
              <TurnRow key={item.id} avatar={<Avatar size={28} alt="You" />}>
                <BodyText>{item.text}</BodyText>
              </TurnRow>
            );
          case "assistant":
            return (
              <TurnRow key={item.id} avatar={<AssistantAvatar />}>
                <BodyText>{item.text}</BodyText>
              </TurnRow>
            );
          case "tool-call":
            return (
              <ToolCallChip
                key={item.id}
                item={item}
                expanded={expandedToolIds.has(item.id)}
                onToggle={() => toggleTool(item.id)}
              />
            );
          case "todo-update":
            return <TodoUpdateMessage key={item.id} heading={item.heading} items={item.items} />;
          case "artifact-ref":
            return <ArtifactRefCard key={item.id} text={item.text} fileName={item.fileName} />;
          default:
            return null;
        }
      })}
    </div>
  );
}

function ArtifactPanel({ onCollapse }: { onCollapse: () => void }) {
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b px-3 py-2">
        <FileCodeIcon className="h-3.5 w-3.5 shrink-0 text-foreground-muted" />
        <span className="truncate font-mono text-xs text-foreground">{ARTIFACT.fileName}</span>
        <button
          type="button"
          onClick={onCollapse}
          aria-label="Collapse artifact panel"
          className="ml-auto rounded p-0.5 text-foreground-muted hover:bg-background-hover hover:text-foreground"
        >
          <ChevronRightIcon className="h-3.5 w-3.5" />
        </button>
      </div>
      <PanelTabs defaultValue="code" className="flex min-h-0 flex-1 flex-col">
        <PanelTabsList>
          <PanelTabsTrigger value="code" icon="page" label="Code" />
          <PanelTabsTrigger value="preview" icon="layer-visible" label="Preview" />
        </PanelTabsList>
        <PanelTabsContent value="code" className="min-h-0 flex-1 overflow-auto">
          <pre className="p-3 font-mono text-xs whitespace-pre text-foreground">
            {ARTIFACT.code}
          </pre>
        </PanelTabsContent>
        <PanelTabsContent value="preview" className="min-h-0 flex-1 overflow-auto">
          <div className="p-3">
            <MutedText>{ARTIFACT.description}</MutedText>
          </div>
        </PanelTabsContent>
      </PanelTabs>
    </div>
  );
}

export function Default() {
  const [panelCollapsed, setPanelCollapsed] = useState(false);
  const [panelWidth, setPanelWidth] = useState(360);

  return (
    <div className="flex h-[720px] overflow-hidden bg-background">
      <div className="min-w-0 flex-1 overflow-auto px-6">
        <Transcript />
      </div>
      {panelCollapsed ? (
        <div className="flex items-start border-l p-2">
          <button
            type="button"
            onClick={() => setPanelCollapsed(false)}
            aria-label="Show artifact panel"
            className="rounded p-1 text-foreground-muted hover:bg-background-hover hover:text-foreground"
          >
            <PanelRightOpenIcon className="h-4 w-4" />
          </button>
        </div>
      ) : (
        <CollapsibleResizePanel
          collapsed={panelCollapsed}
          onResizeCollapse={() => setPanelCollapsed(true)}
          width={panelWidth}
          onWidthChange={setPanelWidth}
          side="left"
          className="border-l bg-background"
        >
          <ArtifactPanel onCollapse={() => setPanelCollapsed(true)} />
        </CollapsibleResizePanel>
      )}
    </div>
  );
}
