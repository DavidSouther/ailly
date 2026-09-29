// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  type AillyEvent,
  EventKind,
  type FileReference,
  type IndexProgress,
  type IndexStatus,
  type SessionListItem,
  type ToolCall,
} from "../../src/tauri";
import { resetSessionsStore } from "../../src/ui/sessions/store";

let indexed: SessionListItem[] = [];
let eventsBySession: Record<string, AillyEvent[]> = {};
const startRefresh = vi.fn<() => Promise<void>>();
let onProgress: ((progress: IndexProgress) => void) | null = null;
let onComplete: ((status: IndexStatus) => void) | null = null;

vi.mock("../../src/tauri", async (importActual) => {
  const actual = await importActual<typeof import("../../src/tauri")>();
  return {
    ...actual,
    listSessions: async () => indexed,
    getEventPage: async (sessionId: string) => eventsBySession[sessionId] ?? [],
    startRefresh: () => startRefresh(),
    onIndexProgress: async (handler: (progress: IndexProgress) => void) => {
      onProgress = handler;
      return () => {
        onProgress = null;
      };
    },
    onIndexComplete: async (handler: (status: IndexStatus) => void) => {
      onComplete = handler;
      return () => {
        onComplete = null;
      };
    },
  };
});

const SESSION: SessionListItem = {
  id: "claude_code:/home/a.jsonl:one",
  harness: "claude_code",
  project: { Recorded: "/Users/dev/repo" },
  event_count: 6,
  token_total: "Absent",
  recorded_price_micros: "Absent",
  estimated_tokens: "Absent",
  estimated_price_micros: "Absent",
  estimated_as_of: "Absent",
  last_activity: { Recorded: "2026-08-20T15:00:00Z" },
};

function fromTool(path: string, operation: string): FileReference {
  return {
    path,
    target: { Recorded: "file" },
    operation: { Recorded: operation },
    provenance: { Recorded: "tool" },
    ambiguity: "Absent",
    cwd: "Absent",
  };
}

function fromShell(path: string, operation: string, cwd: string | null = null): FileReference {
  return {
    path,
    target: { Recorded: "file" },
    operation: { Recorded: operation },
    provenance: { Recorded: "shell" },
    ambiguity: "Absent",
    cwd: cwd === null ? "Absent" : { Recorded: cwd },
  };
}

function ambiguous(fragment: string, operation: string, reason: string): FileReference {
  return {
    path: fragment,
    target: { Recorded: "file" },
    operation: { Recorded: operation },
    provenance: { Recorded: "shell" },
    ambiguity: { Recorded: reason },
    cwd: "Absent",
  };
}

function directory(path: string, operation: string, cwd: string | null = null): FileReference {
  return {
    path,
    target: { Recorded: "directory" },
    operation: { Recorded: operation },
    provenance: { Recorded: "shell" },
    ambiguity: "Absent",
    cwd: cwd === null ? "Absent" : { Recorded: cwd },
  };
}

const TOOL_READ = "packages/auth/src/session.ts";
const TOOL_WRITE = "packages/auth/src/tokens.ts";
const SHELL_READ = "config/base.yml";
const SHELL_WRITE = "build/out.env";
const SHELL_DELETE = "build/stale.env";
const SCRIPTED_READ = "src/main.rs";
const GLOB_FRAGMENT = "logs/*.txt";

function baseEvent(id: string, ordinal: number, kind: EventKind): AillyEvent {
  return {
    id,
    session_id: SESSION.id,
    kind,
    source: { harness: "claude_code", path: "/home/a.jsonl", line: ordinal, ordinal },
    native_id: "Absent",
    response_id: "Absent",
    model: "Absent",
    timestamp: "Absent",
    turn: "Absent",
    tool_call: "Absent",
    tool_result: "Absent",
    token_usage: "Absent",
    files: "Absent",
    detail: "Absent",
    subagent: "Absent",
  };
}

function toolEvent(
  id: string,
  ordinal: number,
  tool: Partial<ToolCall> & { name: string },
  files: FileReference[],
): AillyEvent {
  return {
    ...baseEvent(id, ordinal, EventKind.ToolCall),
    tool_call: {
      Recorded: {
        call_id: "Absent",
        input: "Absent",
        command: "Absent",
        path: "Absent",
        url: "Absent",
        cwd: "Absent",
        ...tool,
      } satisfies ToolCall,
    },
    files: { Recorded: files },
  };
}

const EVENTS: AillyEvent[] = [
  toolEvent("evt-1", 1, { name: "Read", path: { Recorded: TOOL_READ } }, [
    fromTool(TOOL_READ, "read"),
  ]),
  toolEvent("evt-2", 2, { name: "Write", path: { Recorded: TOOL_WRITE } }, [
    fromTool(TOOL_WRITE, "write"),
  ]),
  toolEvent(
    "evt-3",
    3,
    { name: "Bash", command: { Recorded: `cat ${SHELL_READ} >> ${SHELL_WRITE}` } },
    [fromShell(SHELL_READ, "read"), fromShell(SHELL_WRITE, "write")],
  ),
  toolEvent("evt-4", 4, { name: "Bash", command: { Recorded: `rm -f ${SHELL_DELETE}` } }, [
    fromShell(SHELL_DELETE, "delete"),
  ]),
  toolEvent(
    "evt-5",
    5,
    { name: "Bash", command: { Recorded: `sed -n '1,220p' ${SCRIPTED_READ}` } },
    [fromShell(SCRIPTED_READ, "read")],
  ),
  toolEvent("evt-6", 6, { name: "Bash", command: { Recorded: `cat ${GLOB_FRAGMENT}` } }, [
    ambiguous(GLOB_FRAGMENT, "read", "glob not expanded"),
  ]),
];

beforeEach(() => {
  indexed = [SESSION];
  eventsBySession = { [SESSION.id]: EVENTS };
  startRefresh.mockResolvedValue(undefined);
  resetSessionsStore();
});

afterEach(() => {
  cleanup();
  startRefresh.mockReset();
  onProgress = null;
  onComplete = null;
  resetSessionsStore();
});

async function renderApp() {
  const { App } = await import("../../src/App");
  render(<App />);
}

function accessRow(list: HTMLElement, path: string): HTMLElement {
  const row = within(list)
    .getAllByRole("listitem")
    .find((item) => item.textContent?.includes(path));
  if (row === undefined) {
    throw new Error(`No Filesystem row for ${path}`);
  }
  return row;
}

describe("Reviewing the files a session's shell commands touched", () => {
  it("lists shell-derived and tool-derived accesses with operation and provenance", async () => {
    await renderApp();

    const summary = await screen.findByRole("region", { name: /session summary/i });
    const fileAccess = within(summary).getByRole("list", { name: /filesystem/i });

    // A path the harness named in its own field: read, and known to be so
    // because a tool said it, not because anyone read a command.
    const toolRead = accessRow(fileAccess, TOOL_READ);
    expect(toolRead).toHaveTextContent(/read/i);
    expect(toolRead).toHaveTextContent(/tool/i);
    expect(toolRead).not.toHaveTextContent(/shell/i);

    expect(accessRow(fileAccess, TOOL_WRITE)).toHaveTextContent(/write/i);

    // The same two operations, recovered from one recorded command and labelled
    // as evidence about that command rather than as a tool's own claim.
    const shellRead = accessRow(fileAccess, SHELL_READ);
    expect(shellRead).toHaveTextContent(/read/i);
    expect(shellRead).toHaveTextContent(/shell/i);

    const shellWrite = accessRow(fileAccess, SHELL_WRITE);
    expect(shellWrite).toHaveTextContent(/write/i);
    expect(shellWrite).toHaveTextContent(/shell/i);

    // Deletion is its own operation, not a write and not a silent omission.
    const shellDelete = accessRow(fileAccess, SHELL_DELETE);
    expect(shellDelete).toHaveTextContent(/delete/i);
    expect(shellDelete).toHaveTextContent(/shell/i);

    // `sed -n '1,220p' src/main.rs` reads one file. Its script operand is not a
    // second one, however confidently the parse could report it as a path.
    expect(accessRow(fileAccess, SCRIPTED_READ)).toHaveTextContent(/read/i);
    expect(within(fileAccess).queryByText(/1,220p/)).not.toBeInTheDocument();

    // A glob is shown, labelled as unresolved, and never presented as a path.
    const glob = accessRow(fileAccess, GLOB_FRAGMENT);
    expect(glob).toHaveTextContent(/ambiguous/i);
    expect(glob).toHaveTextContent(/glob not expanded/i);

    // Every row is either a path or an ambiguous fragment, and the tile counts
    // only the paths.
    expect(within(fileAccess).getAllByRole("listitem")).toHaveLength(7);
    const tile = within(summary).getByRole("group", { name: /files touched/i });
    expect(within(tile).getByText("6")).toBeInTheDocument();
  });

  it("keeps the same relative path under different cwds as two rows", async () => {
    eventsBySession = {
      [SESSION.id]: [
        toolEvent(
          "evt-1",
          1,
          {
            name: "Bash",
            command: { Recorded: "cat config.toml" },
            cwd: { Recorded: "/work/app" },
          },
          [fromShell("config.toml", "read", "/work/app")],
        ),
        toolEvent(
          "evt-2",
          2,
          {
            name: "Bash",
            command: { Recorded: "cat config.toml" },
            cwd: { Recorded: "/work/other" },
          },
          [fromShell("config.toml", "read", "/work/other")],
        ),
      ],
    };

    await renderApp();

    const summary = await screen.findByRole("region", { name: /session summary/i });
    const fileAccess = within(summary).getByRole("list", { name: /filesystem/i });
    const rows = within(fileAccess).getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent(/config\.toml/);
    expect(rows[1]).toHaveTextContent(/config\.toml/);
    expect(fileAccess).toHaveTextContent("/work/app");
    expect(fileAccess).toHaveTextContent("/work/other");
    const tile = within(summary).getByRole("group", { name: /files touched/i });
    expect(within(tile).getByText("2")).toBeInTheDocument();
  });

  it("reveals and filters files beyond the ranked-list cap", async () => {
    const files = Array.from({ length: 12 }, (_, index) =>
      fromShell(`src/generated/file-${index + 1}.ts`, "read"),
    );
    eventsBySession = {
      [SESSION.id]: [
        toolEvent("evt-1", 1, { name: "Bash", command: { Recorded: "find src -type f" } }, files),
      ],
    };

    await renderApp();

    const summary = await screen.findByRole("region", { name: /session summary/i });
    const fileAccess = within(summary).getByRole("list", { name: /filesystem/i });
    expect(within(fileAccess).getAllByRole("listitem")).toHaveLength(10);

    const filter = within(summary).getByRole("searchbox", { name: /filter paths/i });
    fireEvent.change(filter, { target: { value: "file-12" } });
    expect(within(fileAccess).getAllByRole("listitem")).toHaveLength(1);
    expect(fileAccess).toHaveTextContent("src/generated/file-12.ts");

    fireEvent.change(filter, { target: { value: "" } });
    fireEvent.click(within(summary).getByRole("button", { name: /show all 12 paths/i }));
    expect(within(fileAccess).getAllByRole("listitem")).toHaveLength(12);
    expect(within(summary).getByRole("button", { name: /show fewer paths/i })).toBeInTheDocument();
  });

  it("hides rows by operation and provenance without changing the files-touched count", async () => {
    await renderApp();

    const summary = await screen.findByRole("region", { name: /session summary/i });
    const fileAccess = within(summary).getByRole("list", { name: /filesystem/i });
    const filters = within(summary).getByRole("group", { name: /filesystem filters/i });

    fireEvent.click(within(filters).getByRole("button", { name: /^write$/i }));
    expect(fileAccess).not.toHaveTextContent(TOOL_WRITE);
    expect(fileAccess).not.toHaveTextContent(SHELL_WRITE);
    expect(accessRow(fileAccess, TOOL_READ)).toBeInTheDocument();
    expect(accessRow(fileAccess, SHELL_READ)).toBeInTheDocument();

    fireEvent.click(within(filters).getByRole("button", { name: /^shell$/i }));
    expect(fileAccess).not.toHaveTextContent(SHELL_READ);
    expect(fileAccess).not.toHaveTextContent(SHELL_DELETE);
    expect(accessRow(fileAccess, TOOL_READ)).toBeInTheDocument();

    const tile = within(summary).getByRole("group", { name: /files touched/i });
    expect(within(tile).getByText("6")).toBeInTheDocument();
  });

  it("hides a row that was written even when it was also read", async () => {
    eventsBySession = {
      [SESSION.id]: [
        toolEvent(
          "evt-1",
          1,
          { name: "Bash", command: { Recorded: `sed -i 's/alpha/beta/' ${SHELL_READ}` } },
          [fromShell(SHELL_READ, "read"), fromShell(SHELL_READ, "write")],
        ),
        toolEvent("evt-2", 2, { name: "Read", path: { Recorded: TOOL_READ } }, [
          fromTool(TOOL_READ, "read"),
        ]),
      ],
    };

    await renderApp();

    const summary = await screen.findByRole("region", { name: /session summary/i });
    const fileAccess = within(summary).getByRole("list", { name: /filesystem/i });
    expect(accessRow(fileAccess, SHELL_READ)).toHaveTextContent("write");

    const filters = within(summary).getByRole("group", { name: /filesystem filters/i });
    fireEvent.click(within(filters).getByRole("button", { name: /^write$/i }));

    expect(fileAccess).not.toHaveTextContent(SHELL_READ);
    expect(accessRow(fileAccess, TOOL_READ)).toBeInTheDocument();
  });

  it("isolates a label on a second click and stops filtering on a third", async () => {
    await renderApp();

    const summary = await screen.findByRole("region", { name: /session summary/i });
    const fileAccess = within(summary).getByRole("list", { name: /filesystem/i });
    const filters = within(summary).getByRole("group", { name: /filesystem filters/i });
    const clickAmbiguous = () =>
      fireEvent.click(within(filters).getByRole("button", { name: /ambiguous/i }));

    clickAmbiguous();
    expect(fileAccess).not.toHaveTextContent(GLOB_FRAGMENT);
    expect(accessRow(fileAccess, TOOL_READ)).toBeInTheDocument();

    clickAmbiguous();
    expect(within(filters).getByRole("button", { name: /^only ambiguous$/i })).toBeInTheDocument();
    expect(within(fileAccess).getAllByRole("listitem")).toHaveLength(1);
    expect(fileAccess).toHaveTextContent(GLOB_FRAGMENT);

    clickAmbiguous();
    expect(accessRow(fileAccess, TOOL_READ)).toBeInTheDocument();
    expect(fileAccess).toHaveTextContent(GLOB_FRAGMENT);
  });

  it("isolates either operation within a dimension and both conditions across them", async () => {
    await renderApp();

    const summary = await screen.findByRole("region", { name: /session summary/i });
    const fileAccess = within(summary).getByRole("list", { name: /filesystem/i });
    const filters = within(summary).getByRole("group", { name: /filesystem filters/i });
    // A chip's accessible name gains its state, so the second click looks the
    // label up by prefix rather than by the name the first click left behind.
    const isolate = (label: string) => {
      const chip = () =>
        within(filters).getByRole("button", { name: new RegExp(`^${label}`, "i") });
      fireEvent.click(chip());
      fireEvent.click(chip());
    };

    isolate("write");
    isolate("delete");
    expect(accessRow(fileAccess, TOOL_WRITE)).toBeInTheDocument();
    expect(accessRow(fileAccess, SHELL_DELETE)).toBeInTheDocument();
    expect(fileAccess).not.toHaveTextContent(TOOL_READ);

    isolate("shell");
    expect(accessRow(fileAccess, SHELL_WRITE)).toBeInTheDocument();
    expect(accessRow(fileAccess, SHELL_DELETE)).toBeInTheDocument();
    expect(fileAccess).not.toHaveTextContent(TOOL_WRITE);
  });

  it("labels a directory in the same list and filters by that label", async () => {
    eventsBySession = {
      [SESSION.id]: [
        toolEvent(
          "evt-1",
          1,
          { name: "Bash", command: { Recorded: "ls ." }, cwd: { Recorded: "/work/app" } },
          [directory(".", "read", "/work/app")],
        ),
        toolEvent("evt-2", 2, { name: "Read", path: { Recorded: TOOL_READ } }, [
          fromTool(TOOL_READ, "read"),
        ]),
      ],
    };

    await renderApp();

    const summary = await screen.findByRole("region", { name: /session summary/i });
    const filesystem = within(summary).getByRole("list", { name: /^filesystem$/i });
    expect(accessRow(filesystem, ".")).toHaveTextContent("directory");
    expect(accessRow(filesystem, ".")).toHaveTextContent("/work/app");
    expect(accessRow(filesystem, TOOL_READ)).not.toHaveTextContent("directory");
    // The tile counts files, and a directory the session listed is not one.
    expect(within(summary).getByRole("group", { name: /files touched/i })).toHaveTextContent("1");

    const filters = within(summary).getByRole("group", { name: /filesystem filters/i });
    const chip = () => within(filters).getByRole("button", { name: /^(only )?directory/i });
    fireEvent.click(chip());
    expect(filesystem).not.toHaveTextContent("/work/app");
    expect(accessRow(filesystem, TOOL_READ)).toBeInTheDocument();

    fireEvent.click(chip());
    expect(within(filesystem).getAllByRole("listitem")).toHaveLength(1);
    expect(filesystem).toHaveTextContent("/work/app");
  });
});
