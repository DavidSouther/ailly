import { useEffect, useState } from "react";

import { type AillyEvent, errorMessage, getEventPage, isRecorded } from "../../tauri";
import { loadDescendantEvents } from "../subagents/descendants";
import { type ChildSpendResult, foldOwnUsage } from "./rollup";

/**
 * What each linked spawn's own subtree spent, resolving per spawn as its read
 * settles.
 *
 * One walk per spawn rather than one for the whole page, unlike Summary's
 * `useDescendantEvents`: seeding with a single spawn event scopes the walk to
 * exactly that delegation's descendants, so a sibling delegation's spend is
 * never folded into this one's figure. A grandchild's cost is still this
 * delegation's cost, so the walk goes all the way down.
 */
export function useChildSpends(spawnEvents: AillyEvent[]): Map<string, ChildSpendResult> {
  const [spends, setSpends] = useState<Map<string, ChildSpendResult>>(new Map());

  useEffect(() => {
    let cancelled = false;
    const settle = (eventId: string, result: ChildSpendResult) => {
      if (!cancelled) {
        setSpends((current) => new Map(current).set(eventId, result));
      }
    };

    const started = spawnEvents.map((event) => [event, initialResult(event)] as const);
    setSpends(new Map(started.map(([event, result]) => [event.id, result])));
    for (const [event, result] of started) {
      if (result.status === "unlinkable") {
        continue;
      }
      loadDescendantEvents([event], getEventPage)
        .then((events) => {
          settle(event.id, {
            status: "resolved",
            buckets: foldOwnUsage(events).buckets,
            events,
          });
        })
        .catch((cause: unknown) => {
          settle(event.id, { status: "failed", message: errorMessage(cause) });
        });
    }
    return () => {
      cancelled = true;
    };
  }, [spawnEvents]);

  return spends;
}

/**
 * Where a spawn starts: unlinkable the moment its source named no child, since
 * there is no read to wait for, and otherwise pending until one settles.
 */
function initialResult(event: AillyEvent): ChildSpendResult {
  const linked = isRecorded(event.subagent) && isRecorded(event.subagent.Recorded.child_session_id);
  return linked ? { status: "pending" } : { status: "unlinkable" };
}
