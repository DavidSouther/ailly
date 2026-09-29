import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke,
}));

describe("listSessions", () => {
  beforeEach(() => {
    // `initIndex` memoizes a module-level promise; reset so each test gets a
    // fresh `initialized` rather than leaking across cases or file order.
    vi.resetModules();
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "index_init") {
        return;
      }
      if (cmd === "list_sessions") {
        return { items: [] };
      }
      throw new Error(`unexpected invoke: ${cmd}`);
    });
  });

  it("requests a high ceiling, not 500", async () => {
    const { listSessions } = await import("../../src/tauri");

    await listSessions();

    const listCall = invoke.mock.calls.find(([cmd]) => cmd === "list_sessions");
    expect(listCall).toBeDefined();
    const [, args] = listCall as [string, { query: { limit: number } }];
    expect(args.query.limit).toBe(100_000);
    expect(args.query.limit).not.toBe(500);
  });
});
