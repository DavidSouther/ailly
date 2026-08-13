import {
  FilePenLine,
  FilePlus,
  Globe,
  ListChecks,
  MessageCircleQuestion,
  Search,
  Sparkles,
  SquareTerminal,
  Text,
  Users,
  Wrench,
} from "lucide-react";

import type { IconType } from "./icons";
import { categoryForTool } from "./summary/rollup";

/**
 * Additive name -> icon table. Holds only entries more specific than the
 * tool's category glyph, or belonging to a category with no shape of its own.
 * Mirrors the additive contract of rollup.ts's CATEGORY_TABLE.
 */
const NAME_ICON_TABLE: Record<string, IconType> = {
  Write: FilePlus,
  write: FilePlus,
  Grep: Search,
  Agent: Users,
  Task: Users,
  TodoWrite: ListChecks,
  update_plan: ListChecks,
  Skill: Sparkles,
  AskUserQuestion: MessageCircleQuestion,
  WebFetch: Globe,
  WebSearch: Globe,
};

/** Icon for each ToolCategory that names a shape of work. "other" is deliberately absent — it falls to the neutral floor. */
const CATEGORY_ICON_TABLE: Record<"exec" | "edit" | "read", IconType> = {
  exec: SquareTerminal,
  edit: FilePenLine,
  read: Text,
};

/**
 * The glyph for a tool call: exact name, then its category, then a neutral
 * wrench for "other" and "unclassified". Never guesses from the name's spelling.
 */
export function toolIcon(name: string): IconType {
  // Own-property only: harness tool names are open strings, and a plain
  // Record lookup would treat Object.prototype keys (constructor, toString, …)
  // as icons and crash when rendered as JSX.
  const byName = Object.hasOwn(NAME_ICON_TABLE, name) ? NAME_ICON_TABLE[name] : undefined;
  if (byName) {
    return byName;
  }
  const category = categoryForTool(name);
  const byCategory =
    category === "exec" || category === "edit" || category === "read"
      ? CATEGORY_ICON_TABLE[category]
      : undefined;
  return byCategory ?? Wrench;
}
