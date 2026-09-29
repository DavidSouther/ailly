import { ChevronDown, ChevronRight, FileText, Folder, Search } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";

import { type SourceValue, isRecorded } from "../../tauri";
import { CapturedOutput } from "../CapturedOutput";
import { ToolPayload } from "../ToolPayload";
import { Badge, badgeColorClass } from "../badges/badge";
import { BadgeColor } from "../colors";
import { shouldShowToolCwd } from "../conversation/format";
import { toolIcon } from "../toolIcons";
import { cn } from "../utils";
import type { FileAccess, SourceCall, ToolCategory, ToolFrequency } from "./rollup";

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

export function CallsByTool({
  tools,
  project,
}: { tools: ToolFrequency[]; project: SourceValue<string> }) {
  const shown = tools.slice(0, LIST_CAP);
  const sessionCwd = isRecorded(project) ? project.Recorded : null;
  return (
    <div className="flex flex-col gap-1.5">
      <SectionHeading>Tools</SectionHeading>
      <ul aria-label="Tools" className="rounded-md border">
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

/**
 * An operation is an open string from the index rather than a closed union, so
 * an operation this table has not seen still renders — in the neutral colour
 * instead of vanishing.
 */
const OPERATION_COLOR: Record<string, BadgeColor> = {
  read: BadgeColor.MINT,
  write: BadgeColor.SKY,
  delete: BadgeColor.BERRY,
};

const OPERATION_ORDER = ["read", "write", "delete"];
const PROVENANCE_ORDER = ["tool", "shell"];
const AMBIGUOUS = "ambiguous";
const DIRECTORY = "directory";

type Dimension = "operation" | "source" | "ambiguity" | "kind";
const DIMENSIONS: Dimension[] = ["operation", "source", "ambiguity", "kind"];

type ChipState = "hidden" | "only";

function labelsInOrder(present: Iterable<string>, preferred: string[]): string[] {
  const seen = new Set(present);
  const rest = [...seen].filter((label) => !preferred.includes(label)).sort();
  return [...preferred.filter((label) => seen.has(label)), ...rest];
}

/** Pick the colour while the label's dimension is still known. */
interface AccessFilter {
  label: string;
  color: BadgeColor;
  dimension: Dimension;
}

function fileAccessFilters(files: FileAccess[]): AccessFilter[] {
  const operations = new Set<string>();
  const provenances = new Set<string>();
  let ambiguous = false;
  let directory = false;
  for (const file of files) {
    for (const operation of file.operations) {
      operations.add(operation);
    }
    for (const provenance of file.provenances) {
      provenances.add(provenance);
    }
    if (file.ambiguity !== null) {
      ambiguous = true;
    }
    if (isDirectory(file)) {
      directory = true;
    }
  }
  return [
    ...labelsInOrder(operations, OPERATION_ORDER).map((label): AccessFilter => {
      return {
        label,
        color: OPERATION_COLOR[label] ?? BadgeColor.METAL_DARK,
        dimension: "operation",
      };
    }),
    ...labelsInOrder(provenances, PROVENANCE_ORDER).map((label): AccessFilter => {
      return { label, color: BadgeColor.METAL, dimension: "source" };
    }),
    ...(ambiguous
      ? [{ label: AMBIGUOUS, color: BadgeColor.AMBER, dimension: "ambiguity" as Dimension }]
      : []),
    ...(directory
      ? [{ label: DIRECTORY, color: BadgeColor.PLUM, dimension: "kind" as Dimension }]
      : []),
  ];
}

function isDirectory(file: FileAccess): boolean {
  return file.target === DIRECTORY;
}

/**
 * A file carries no kind label: `directory` is the claim worth making, and a row
 * without it is the ordinary case. Hiding the chip therefore leaves files, and
 * isolating it leaves directories.
 */
function fileLabels(file: FileAccess): Record<Dimension, string[]> {
  return {
    operation: file.operations,
    source: file.provenances,
    ambiguity: file.ambiguity === null ? [] : [AMBIGUOUS],
    kind: isDirectory(file) ? [DIRECTORY] : [],
  };
}

const CHIP_HINT: Record<ChipState | "shown", (label: string) => string> = {
  shown: (label) => `Hide ${label}`,
  hidden: (label) => `Show only ${label}`,
  only: (label) => `Stop filtering by ${label}`,
};

/** Cycle: shown → hidden → only → shown. */
function nextChipState(
  current: ReadonlyMap<string, ChipState>,
  label: string,
): Map<string, ChipState> {
  const next = new Map(current);
  const state = current.get(label);
  if (state === undefined) {
    next.set(label, "hidden");
  } else if (state === "hidden") {
    next.set(label, "only");
  } else {
    next.delete(label);
  }
  return next;
}

/**
 * Hidden labels exclude any row carrying that label. "Only" labels are ORed
 * within a dimension and ANDed across dimensions.
 */
function fileMatchesToggles(
  file: FileAccess,
  filters: AccessFilter[],
  states: ReadonlyMap<string, ChipState>,
): boolean {
  if (states.size === 0) {
    return true;
  }
  const labels = fileLabels(file);
  for (const dimension of DIMENSIONS) {
    const carried = labels[dimension];
    if (carried.some((label) => states.get(label) === "hidden")) {
      return false;
    }
    const isolated = filters.filter(
      (filter) => filter.dimension === dimension && states.get(filter.label) === "only",
    );
    if (isolated.length > 0 && !isolated.some((filter) => carried.includes(filter.label))) {
      return false;
    }
  }
  return true;
}

/**
 * Displays indexed filesystem accesses, including directories and ambiguous
 * fragments. Rows expose provenance and ambiguity and can be filtered by those
 * labels.
 */
export function FilesystemList({ files }: { files: FileAccess[] }) {
  const [filter, setFilter] = useState("");
  const [showAll, setShowAll] = useState(false);
  const [chipStates, setChipStates] = useState<Map<string, ChipState>>(() => new Map());
  const toggles = useMemo(() => fileAccessFilters(files), [files]);
  const query = filter.trim().toLocaleLowerCase();
  const filtering = query !== "" || chipStates.size > 0;
  const filtered = files.filter((file) => {
    if (!fileMatchesToggles(file, toggles, chipStates)) {
      return false;
    }
    if (query === "") {
      return true;
    }
    // Every label a row wears is searchable alongside its path and working
    // directory, so a label can be typed instead of hunted for as a chip.
    return [file.path, file.cwd, file.ambiguity, ...Object.values(fileLabels(file)).flat()].some(
      (value) => value?.toLocaleLowerCase().includes(query),
    );
  });
  const shown = query !== "" || showAll ? filtered : filtered.slice(0, LIST_CAP);
  const hidden = filtered.length - shown.length;
  const title = "Filesystem";
  const noun = "paths";

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-3">
        <SectionHeading>{title}</SectionHeading>
        <span className="text-foreground-muted text-xs">
          {filtering ? `${filtered.length} of ${files.length} ${noun}` : `${files.length} ${noun}`}
        </span>
      </div>
      <label className="flex items-center gap-2 rounded-md border bg-background px-2.5 py-1.5 focus-within:border-ring">
        <Search size={14} className="shrink-0 text-foreground-muted" aria-hidden="true" />
        <input
          type="search"
          aria-label={`Filter ${noun}`}
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
          placeholder={`Filter ${noun}, operations, or sources`}
          className="min-w-0 flex-1 bg-transparent text-foreground text-xs outline-none placeholder:text-foreground-muted"
        />
      </label>
      {toggles.length > 0 ? (
        <fieldset
          aria-label="Filesystem filters"
          className="m-0 flex min-w-0 flex-wrap gap-1 border-0 p-0"
        >
          {toggles.map(({ label, color }) => {
            const state = chipStates.get(label);
            return (
              <button
                key={label}
                type="button"
                // `aria-pressed` announces that a click did something; the name
                // says which of the two filtering states it landed in. A
                // struck-through label needs that said in words, while the
                // isolated one already reads "only …" on its face.
                aria-pressed={state !== undefined}
                aria-label={state === "hidden" ? `${label}, hidden` : undefined}
                title={CHIP_HINT[state ?? "shown"](label)}
                onClick={() => setChipStates((current) => nextChipState(current, label))}
                className={cn(
                  "focus-ring rounded px-1.5 py-0.5 text-xs",
                  state === "hidden"
                    ? "border border-dashed text-foreground-muted line-through hover:bg-background-hover-solid"
                    : badgeColorClass(color),
                  state === "only" && "font-medium",
                )}
              >
                {state === "only" ? `only ${label}` : label}
              </button>
            );
          })}
        </fieldset>
      ) : null}
      <ul aria-label={title} className="flex flex-col gap-1.5">
        {shown.map((file) => {
          const showCwd = shouldShowToolCwd({
            cwd: file.cwd,
            path: file.path,
            sessionCwd: null,
          });
          return (
            <li key={file.id} className="flex min-w-0 flex-col gap-0.5">
              <div className="flex min-w-0 items-center gap-2">
                {isDirectory(file) ? (
                  <Folder size={14} className="shrink-0 text-foreground-muted" />
                ) : (
                  <FileText size={14} className="shrink-0 text-foreground-muted" />
                )}
                <span
                  title={file.path}
                  className="truncate-start min-w-0 flex-1 font-mono text-foreground text-xs"
                >
                  {file.path}
                </span>
                <div className="flex shrink-0 gap-1">
                  {isDirectory(file) ? (
                    <Badge color={BadgeColor.PLUM} textSize="sm">
                      {DIRECTORY}
                    </Badge>
                  ) : null}
                  {file.operations.map((operation) => (
                    <Badge
                      key={operation}
                      color={OPERATION_COLOR[operation] ?? BadgeColor.METAL_DARK}
                      textSize="sm"
                    >
                      {operation}
                    </Badge>
                  ))}
                  {file.provenances.map((provenance) => (
                    <Badge key={provenance} color={BadgeColor.METAL} textSize="sm">
                      {provenance}
                    </Badge>
                  ))}
                  {file.ambiguity === null ? null : (
                    <Badge color={BadgeColor.AMBER} textSize="sm">
                      ambiguous
                    </Badge>
                  )}
                </div>
                <span className="shrink-0 text-foreground-muted text-xs">
                  {file.touches} {file.touches === 1 ? "touch" : "touches"}
                </span>
              </div>
              {showCwd ? (
                <span className="pl-[22px] font-mono text-foreground-muted text-xs">
                  {file.cwd}
                </span>
              ) : null}
              {file.ambiguity === null ? null : (
                <span className="pl-[22px] text-foreground-muted text-xs italic">
                  {file.ambiguity}
                </span>
              )}
            </li>
          );
        })}
      </ul>
      {filtered.length === 0 ? (
        <p className="py-2 text-foreground-muted text-xs">
          {query === ""
            ? `No ${noun} match these filters.`
            : `No ${noun} match “${filter.trim()}”.`}
        </p>
      ) : null}
      {query === "" && (hidden > 0 || showAll) ? (
        <button
          type="button"
          onClick={() => setShowAll((value) => !value)}
          className="focus-ring w-fit rounded-sm text-left font-medium text-foreground-muted text-xs hover:text-foreground"
        >
          {showAll ? `Show fewer ${noun}` : `Show all ${filtered.length} ${noun} (+${hidden} more)`}
        </button>
      ) : null}
    </div>
  );
}
