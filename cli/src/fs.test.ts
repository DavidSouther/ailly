import {
  loadContent,
  makeCLIContent,
} from "@ailly/core/lib/content/content.js";
import { makePipelineSettings } from "@ailly/core/lib/index.js";
import {
  FileSystem,
  RecordFileSystemAdapter,
} from "@davidsouther/jiffies/lib/cjs/fs";
import { cleanState } from "@davidsouther/jiffies/lib/cjs/scope/state.js";
import { beforeEach, describe, expect, it } from "vitest";
import * as cli from "./fs";

describe("makeEdit", () => {
  it("parses line range", () => {
    expect(cli.makeEdit("10:20", ["/foo.ts"], true)).toEqual({
      start: 9,
      end: 19,
      file: "/foo.ts",
    });
  });

  it("parses line start", () => {
    expect(cli.makeEdit("10:", ["/foo.ts"], true)).toEqual({
      after: 9,
      file: "/foo.ts",
    });
  });
  it("parses line end", () => {
    expect(cli.makeEdit(":20", ["/foo.ts"], true)).toEqual({
      after: 18,
      file: "/foo.ts",
    });
  });

  it("requires a single content", () => {
    expect(() => cli.makeEdit("10:20", [], true)).toThrow(
      /Edit requires exactly 1 path/
    );
    expect(() => cli.makeEdit("10:20", ["/a", "/b"], true)).toThrow(
      /Edit requires exactly 1 path/
    );
  });
  it("must have prompt", () => {
    expect(() => cli.makeEdit("10:20", ["/a"], false)).toThrow(
      /Edit requires a prompt/
    );
  });
});

describe("makeCLIContent", () => {
  const state = cleanState(async () => {
    const root = "/root";
    const settings = await makePipelineSettings({ root });
    const fs = new FileSystem(
      new RecordFileSystemAdapter({
        "/root/a": "file a",
        "/root/b": "file b",
      })
    );
    const context = await loadContent(fs, [], settings);
    return { root, settings, context };
  }, beforeEach);

  it("creates a valid Content object with 'none' context", async () => {
    const prompt = "Prompt";
    const argContext = "none";
    const argSystem = "System";
    const edit = undefined;
    const view = { prop: "test" };

    const cliContent = await makeCLIContent({
      prompt,
      argContext,
      argSystem,
      context: state.context,
      root: state.root,
      edit,
      view,
    });

    expect(cliContent).toMatchObject({
      name: "stdout",
      outPath: "/dev/stdout",
      path: "/dev/stdout",
      prompt: "Prompt",
      context: {
        view: { prop: "test" },
        predecessor: undefined,
        system: [],
        folder: undefined,
        edit: undefined,
      },
    });
  });

  it("creates a valid Content object with 'folder' context", async () => {
    const prompt = "Prompt";
    const argContext = "folder";
    const argSystem = "System";
    const edit = undefined;
    const view = { prop: "test" };

    const cliContent = await makeCLIContent({
      prompt,
      argContext,
      argSystem,
      context: state.context,
      root: state.root,
      edit,
      view,
    });

    expect(cliContent).toMatchObject({
      name: "stdout",
      outPath: "/dev/stdout",
      path: "/dev/stdout",
      prompt: "Prompt",
      context: {
        view: { prop: "test" },
        predecessor: undefined,
        system: [{ content: "System", view: {} }],
        folder: ["/root/a", "/root/b"],
        edit: undefined,
      },
    });
  });

  it("creates a valid Content object with 'conversation' context", async () => {
    const prompt = "Prompt";
    const argContext = "conversation";
    const argSystem = "System";
    const edit = undefined;
    const view = { prop: "test" };

    const cliContent = await makeCLIContent({
      prompt,
      argContext,
      argSystem,
      context: state.context,
      root: state.root,
      edit,
      view,
    });

    expect(cliContent).toMatchObject({
      name: "stdout",
      outPath: "/dev/stdout",
      path: "/dev/stdout",
      prompt: "Prompt",
      context: {
        view: { prop: "test" },
        predecessor: "/root/b",
        system: [{ content: "System", view: {} }],
        folder: undefined,
        edit: undefined,
      },
    });
  });

  it("handles empty content array with 'conversation' context", async () => {
    const prompt = "Prompt";
    const argContext = "conversation";
    const argSystem = "System";
    const edit = undefined;
    const view = { prop: "test" };

    const cliContent = await makeCLIContent({
      prompt,
      argContext,
      argSystem,
      context: state.context,
      root: state.root,
      edit,
      view,
    });

    expect(cliContent).toMatchObject({
      name: "stdout",
      outPath: "/dev/stdout",
      path: "/dev/stdout",
      prompt: "Prompt",
      context: {
        view: { prop: "test" },
        predecessor: "/root/b",
        system: [{ content: "System", view: {} }],
        folder: undefined,
        edit: undefined,
      },
    });
  });
});
