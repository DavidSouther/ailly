import { describe, expect, it } from "vitest";

describe("Tauri shell client seam", () => {
  it("uses the expected application command name", async () => {
    const source = await import("../../src/tauri");
    expect(source.appReady).toBeTypeOf("function");
  });
});
