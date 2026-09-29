import { type AillyEvent, EventKind, isRecorded } from "../../tauri";

/**
 * Every child session id a spawn in these events named, in source order and
 * without duplicates. Spawns that recorded no child are skipped — there is
 * nothing recursive to open.
 */
export function namedChildSessionIds(events: AillyEvent[]): string[] {
  const seen = new Set<string>();
  const ids: string[] = [];
  for (const event of events) {
    if (event.kind !== EventKind.SubagentSpawn || !isRecorded(event.subagent)) {
      continue;
    }
    const child = event.subagent.Recorded.child_session_id;
    if (!isRecorded(child) || seen.has(child.Recorded)) {
      continue;
    }
    seen.add(child.Recorded);
    ids.push(child.Recorded);
  }
  return ids;
}

/**
 * Every event belonging to a descendant of `rootEvents`, walking spawn→child
 * links breadth-first and never revisiting a session. The root's own events
 * are not returned — callers merge them when they want the combined fold.
 */
export async function loadDescendantEvents(
  rootEvents: AillyEvent[],
  fetchPage: (sessionId: string) => Promise<AillyEvent[]>,
): Promise<AillyEvent[]> {
  // Seed with every session the root events already belong to so a child that
  // names its parent cannot pull the walk back into the starting session.
  const visited = new Set(rootEvents.map((event) => event.session_id));
  const queue = namedChildSessionIds(rootEvents);
  const collected: AillyEvent[] = [];

  while (queue.length > 0) {
    const sessionId = queue.shift();
    if (sessionId === undefined || visited.has(sessionId)) {
      continue;
    }
    visited.add(sessionId);
    const events = await fetchPage(sessionId);
    collected.push(...events);
    for (const childId of namedChildSessionIds(events)) {
      if (!visited.has(childId)) {
        queue.push(childId);
      }
    }
  }

  return collected;
}
