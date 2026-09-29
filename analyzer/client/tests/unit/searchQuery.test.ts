import { describe, expect, it } from "vitest";

import {
  parseHarnessFilter,
  parseSearchQuery,
  removeHarnessTokenAt,
  resolveHarnessValue,
} from "../../src/ui/sessions/searchQuery";

describe("parseSearchQuery", () => {
  it("extracts a harness token and leaves residual free text", () => {
    expect(parseSearchQuery("harness: codex foo")).toEqual({
      harnessValues: ["codex"],
      residual: "foo",
    });
  });

  it("allows an optional space after the colon", () => {
    expect(parseSearchQuery("harness:codex")).toEqual({
      harnessValues: ["codex"],
      residual: "",
    });
  });

  it("treats the harness key as case-insensitive", () => {
    expect(parseSearchQuery("Harness: Codex bar")).toEqual({
      harnessValues: ["Codex"],
      residual: "bar",
    });
  });

  it("collects multiple harness tokens", () => {
    expect(parseSearchQuery("harness: codex harness: pi")).toEqual({
      harnessValues: ["codex", "pi"],
      residual: "",
    });
  });

  it("leaves unknown keys in the residual", () => {
    expect(parseSearchQuery("project: foo harness: codex")).toEqual({
      harnessValues: ["codex"],
      residual: "project: foo",
    });
  });

  it("returns empty harness values when none are present", () => {
    expect(parseSearchQuery("billing service")).toEqual({
      harnessValues: [],
      residual: "billing service",
    });
  });
});

describe("resolveHarnessValue", () => {
  it("matches harness ids case-insensitively", () => {
    expect(resolveHarnessValue("codex")).toBe("codex");
    expect(resolveHarnessValue("Codex")).toBe("codex");
    expect(resolveHarnessValue("claude_code")).toBe("claude_code");
  });

  it("matches display labels treating _, -, and space as equivalent", () => {
    expect(resolveHarnessValue("Claude Code")).toBe("claude_code");
    expect(resolveHarnessValue("claude-code")).toBe("claude_code");
    expect(resolveHarnessValue("claude_code")).toBe("claude_code");
  });

  it("returns null for unrecognized values", () => {
    expect(resolveHarnessValue("gemini")).toBeNull();
    expect(resolveHarnessValue("")).toBeNull();
  });
});

describe("parseHarnessFilter", () => {
  it("returns unique resolved harnesses and residual free text", () => {
    expect(parseHarnessFilter("harness: Codex harness: codex foo")).toEqual({
      harnesses: ["codex"],
      residual: "foo",
    });
  });

  it("unions multiple distinct harness tokens", () => {
    expect(parseHarnessFilter("harness: codex harness: pi")).toEqual({
      harnesses: ["codex", "pi"],
      residual: "",
    });
  });

  it("keeps resolved harnesses when mixed with unrecognized tokens", () => {
    expect(parseHarnessFilter("harness: codex harness: foo")).toEqual({
      harnesses: ["codex"],
      residual: "",
    });
  });

  it("returns an empty harness array when tokens are present but none resolve", () => {
    expect(parseHarnessFilter("harness: gemini bar")).toEqual({
      harnesses: [],
      residual: "bar",
    });
    expect(parseHarnessFilter("harness: foo")).toEqual({
      harnesses: [],
      residual: "",
    });
  });

  it("returns null harnesses when no harness tokens are present (all)", () => {
    expect(parseHarnessFilter("billing")).toEqual({
      harnesses: null,
      residual: "billing",
    });
    expect(parseHarnessFilter("")).toEqual({
      harnesses: null,
      residual: "",
    });
  });
});

describe("removeHarnessTokenAt", () => {
  it("splices the indexed harness token and leaves residual text", () => {
    expect(removeHarnessTokenAt("harness: codex foo", 0)).toBe("foo");
    expect(removeHarnessTokenAt("billing harness: codex harness: pi", 1)).toBe(
      "billing harness: codex",
    );
  });
});
