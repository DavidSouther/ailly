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
 * a relative day when recent (Today / Yesterday / weekday for 2–5 days ago),
 * otherwise a compact locale date (year only when not the current year), always
 * with hour:minute and no seconds. Unparseable strings stay verbatim;
 * non-recorded states stay explicit.
 */
export function lastActivityLabel(lastActivity: SourceValue<string>): string {
  if (!isRecorded(lastActivity)) {
    return "No timestamp";
  }
  const raw = lastActivity.Recorded;
  const parsed = new Date(raw);
  if (Number.isNaN(parsed.getTime())) {
    return raw;
  }
  const time = parsed.toLocaleString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
  return `${activityDayLabel(parsed)}, ${time}`;
}

/** Calendar days between a local date and today: 0 today, 1 yesterday, …. */
function localDaysAgo(when: Date, now = new Date()): number {
  const today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
  const then = Date.UTC(when.getFullYear(), when.getMonth(), when.getDate());
  return Math.round((today - then) / 86_400_000);
}

function activityDayLabel(when: Date, now = new Date()): string {
  const daysAgo = localDaysAgo(when, now);
  if (daysAgo === 0) {
    return "Today";
  }
  if (daysAgo === 1) {
    return "Yesterday";
  }
  if (daysAgo >= 2 && daysAgo <= 5) {
    return when.toLocaleString(undefined, { weekday: "long" });
  }
  const sameYear = when.getFullYear() === now.getFullYear();
  return when.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    ...(sameYear ? {} : { year: "numeric" }),
  });
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
