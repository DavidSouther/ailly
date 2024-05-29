import { describe, expect, it } from "vitest";

import { makeEdit } from "./fs.js";

describe("makeEdit", () => {
  it("parses line range", () => {
    expect(makeEdit("10:20", ["/foo.ts"], true)).toEqual({
      start: 9,
      end: 19,
      file: "/foo.ts",
    });
  });

  it("parses line start", () => {
    expect(makeEdit("10:", ["/foo.ts"], true)).toEqual({
      after: 9,
      file: "/foo.ts",
    });
  });
  it("parses line end", () => {
    expect(makeEdit(":20", ["/foo.ts"], true)).toEqual({
      after: 18,
      file: "/foo.ts",
    });
  });

  it("requires a single content", () => {
    expect(() => makeEdit("10:20", [], true)).toThrow(
      /Edit requires exactly 1 path/
    );
    expect(() => makeEdit("10:20", ["/a", "/b"], true)).toThrow(
      /Edit requires exactly 1 path/
    );
  });
  it("must have prompt", () => {
    expect(() => makeEdit("10:20", ["/a"], false)).toThrow(
      /Edit requires a prompt/
    );
  });
});
