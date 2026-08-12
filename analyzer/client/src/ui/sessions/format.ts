import { type Harness, type SessionListItem, type SourceValue, isRecorded } from "../../tauri";
import { BadgeColor } from "../colors";

export const HARNESS_LABEL: Record<Harness, string> = {
  claude_code: "Claude Code",
  codex: "Codex",
  pi: "Pi",
};

export const HARNESS_BADGE_COLOR: Record<Harness, BadgeColor> = {
  claude_code: BadgeColor.AMBER,
  codex: BadgeColor.SKY,
  pi: BadgeColor.PLUM,
};

/** Human label for a session's project, keeping "not recorded" explicit. */
export function projectLabel(project: SourceValue<string>): string {
  return isRecorded(project) && project.Recorded.length > 0 ? project.Recorded : "Unknown project";
}

/**
 * Human label for the session's latest activity. Recorded timestamps render as
 * a locale date-time when parseable, otherwise verbatim; non-recorded states
 * stay explicit rather than fabricating a time.
 */
export function lastActivityLabel(lastActivity: SourceValue<string>): string {
  if (!isRecorded(lastActivity)) {
    return "No timestamp";
  }
  const raw = lastActivity.Recorded;
  const parsed = new Date(raw);
  return Number.isNaN(parsed.getTime()) ? raw : parsed.toLocaleString();
}

export function eventCountLabel(count: number): string {
  return `${count.toLocaleString()} ${count === 1 ? "event" : "events"}`;
}

/**
 * Case-insensitive match of a session against a free-text query over its
 * project, id, and harness label.
 */
export function matchesQuery(session: SessionListItem, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) {
    return true;
  }
  const haystack = [projectLabel(session.project), session.id, HARNESS_LABEL[session.harness]]
    .join("\n")
    .toLowerCase();
  return haystack.includes(needle);
}
