import { FilePenLine, FilePlus, Search, SquareTerminal, Text, Wrench } from "lucide-react";
import { describe, expect, it } from "vitest";

import { categoryForTool } from "../../src/ui/summary/rollup";
import { toolIcon } from "../../src/ui/toolIcons";

/**
 * Tool names known to `rollup.ts`'s additive category table, kept here rather
 * than imported since the table itself is private — this list only needs to
 * stay a superset of names actually exercised below, not an exact mirror.
 */
const KNOWN_CATEGORY_TABLE_NAMES = [
  "Bash",
  "Shell",
  "shell",
  "Edit",
  "Write",
  "MultiEdit",
  "apply_patch",
  "Read",
  "Glob",
  "Grep",
  "NotebookRead",
  "Agent",
  "Task",
  "TodoWrite",
  "Skill",
  "AskUserQuestion",
  "WebFetch",
  "WebSearch",
  "exec_command",
  "write_stdin",
  "read_file",
  "update_plan",
  "bash",
  "read",
  "edit",
  "write",
];

describe("toolIcon", () => {
  it("prefers the name table over the tool's category", () => {
    expect(toolIcon("Write")).toBe(FilePlus);
    expect(toolIcon("Grep")).toBe(Search);
  });

  it("falls back to the tool's category when the name table has no entry", () => {
    expect(toolIcon("Bash")).toBe(SquareTerminal);
    expect(toolIcon("Read")).toBe(Text);
    expect(toolIcon("edit")).toBe(FilePenLine);
  });

  it("falls to the neutral wrench for a name neither table recognizes", () => {
    expect(toolIcon("mcp__acme__lookup")).toBe(Wrench);
  });

  it("falls to the neutral wrench for an empty name", () => {
    expect(toolIcon("")).toBe(Wrench);
  });

  it("falls to the neutral wrench for Object.prototype key names", () => {
    expect(toolIcon("constructor")).toBe(Wrench);
    expect(toolIcon("toString")).toBe(Wrench);
  });

  it("resolves every category-table tool name to some icon, not undefined", () => {
    for (const name of KNOWN_CATEGORY_TABLE_NAMES) {
      expect(categoryForTool(name)).not.toBe("unclassified");
      expect(toolIcon(name)).toBeDefined();
    }
  });
});
