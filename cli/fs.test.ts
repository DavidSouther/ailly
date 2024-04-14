import { describe, it, expect, beforeEach } from "vitest";
import { cleanState } from "@davidsouther/jiffies/lib/esm/scope/state.js";
import * as ailly from "@ailly/core";
import * as cli from "./fs";
import {
  FileSystem,
  RecordFileSystemAdapter,
} from "@davidsouther/jiffies/lib/esm/fs";

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
    const settings = await ailly.Ailly.makePipelineSettings({ root });
    const fs = new FileSystem(
      new RecordFileSystemAdapter({
        "/root/a": "file a",
        "/root/b": "file b",
      })
    );
    const context = await ailly.content.load(fs, [], settings);
    return { root, settings, context };
  }, beforeEach);

  it("creates a valid Content object with 'none' context", () => {
    const prompt = "Prompt";
    const argContext = "none";
    const argSystem = "System";
    const edit = undefined;
    const content = ["/root/a", "/root/b"];
    const view = { prop: "test" };

    const cliContent = cli.makeCLIContent(
      prompt,
      argContext,
      argSystem,
      state.context,
      state.root,
      edit,
      content,
      view
    );

    expect(cliContent).toEqual({
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

  it("creates a valid Content object with 'folder' context", () => {
    const prompt = "Prompt";
    const argContext = "folder";
    const argSystem = "System";
    const edit = undefined;
    const content = ["/root/a", "/root/b"];
    const view = { prop: "test" };

    const cliContent = cli.makeCLIContent(
      prompt,
      argContext,
      argSystem,
      state.context,
      state.root,
      edit,
      content,
      view
    );

    expect(cliContent).toEqual({
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

  it("creates a valid Content object with 'content' context", () => {
    const prompt = "Prompt";
    const argContext = "content";
    const argSystem = "System";
    const edit = undefined;
    const content = ["/root/a", "/root/b"];
    const view = { prop: "test" };

    const cliContent = cli.makeCLIContent(
      prompt,
      argContext,
      argSystem,
      state.context,
      state.root,
      edit,
      content,
      view
    );

    expect(cliContent).toEqual({
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

  it("handles empty content array with 'content' context", () => {
    const prompt = "Prompt";
    const argContext = "content";
    const argSystem = "System";
    const edit = undefined;
    const content = [];
    const view = { prop: "test" };

    const cliContent = cli.makeCLIContent(
      prompt,
      argContext,
      argSystem,
      state.context,
      state.root,
      edit,
      content,
      view
    );

    expect(cliContent).toEqual({
      name: "stdout",
      outPath: "/dev/stdout",
      path: "/dev/stdout",
      prompt: "Prompt",
      context: {
        view: { prop: "test" },
        predecessor: undefined,
        system: [{ content: "System", view: {} }],
        folder: undefined,
        edit: undefined,
      },
    });
  });
});
