import { useMemo } from "react";

import { type AillyEvent, EventKind, isRecorded } from "../../tauri";
import { type SessionTokenStats, summarizeTokenUsage } from "./rollup";
import { useChildSpends } from "./useChildSpends";

/**
 * The one entry point for a session's token stats, reactive as each linked
 * child transcript resolves. Both the Tokens lens and the Summary lens's
 * Tokens card call this, so the two surfaces read the same descendant walk and
 * the same fold — a session's spend can never disagree with itself depending
 * on which lens is open.
 */
export function useSessionTokenStats(events: AillyEvent[]): SessionTokenStats {
  const spawnEvents = useMemo(() => recordedSpawns(events), [events]);
  const childSpends = useChildSpends(spawnEvents);
  return useMemo(
    () =>
      summarizeTokenUsage(
        events,
        spawnEvents.flatMap((event) =>
          isRecorded(event.subagent)
            ? [
                {
                  eventId: event.id,
                  subagent: event.subagent.Recorded,
                  childSpend: childSpends.get(event.id) ?? { status: "pending" },
                },
              ]
            : [],
        ),
      ),
    [events, spawnEvents, childSpends],
  );
}

/** Every spawn whose payload the index could read, in source order. */
function recordedSpawns(events: AillyEvent[]): AillyEvent[] {
  return events.filter(
    (event) => event.kind === EventKind.SubagentSpawn && isRecorded(event.subagent),
  );
}
