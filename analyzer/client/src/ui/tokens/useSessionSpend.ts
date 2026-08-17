import { useMemo } from "react";

import type { SessionListItem } from "../../tauri";
import { useSessionsStore } from "../sessions/store";
import { type SessionSpend, foldSessionSpend } from "./sessionSpend";

/**
 * The three prices a token surface shows, each read off indexed session rows
 * rather than folded out of an event page.
 *
 * The scopes line up with the token figures beside them: a session's own row is
 * the orchestrator's spend, and the rows of the descendants the fold reached are
 * the subagents'. A spawn whose child never resolved contributes to neither,
 * which is the same shortfall the token tiles already name.
 */
export interface SpendByParty {
  orchestrator: SessionSpend;
  subagent: SessionSpend;
  session: SessionSpend;
}

/**
 * Session rows by id, from the list the app already holds.
 *
 * The store carries every indexed session, subagent transcripts included — they
 * are peer rows in the index, not a nested shape — so a descendant's price needs
 * no round trip of its own.
 */
function useSessionRows(): Map<string, SessionListItem> {
  const sessions = useSessionsStore((state) => state.sessions);
  return useMemo(() => new Map(sessions.map((session) => [session.id, session])), [sessions]);
}

export function useSessionSpend(
  sessionId: string | null,
  descendantSessionIds: string[],
): SpendByParty {
  const rows = useSessionRows();
  return useMemo(() => {
    const own = sessionId === null ? undefined : rows.get(sessionId);
    const descendants = descendantSessionIds.flatMap((id) => {
      const row = rows.get(id);
      return row === undefined ? [] : [row];
    });
    const orchestrator = foldSessionSpend(own === undefined ? [] : [own]);
    const subagent = foldSessionSpend(descendants);
    return {
      orchestrator,
      subagent,
      session: foldSessionSpend(own === undefined ? descendants : [own, ...descendants]),
    };
  }, [rows, sessionId, descendantSessionIds]);
}

/**
 * Which session a loaded event page belongs to.
 *
 * A page is always one session's events, so the first event names it. A page
 * with no events names nothing, and the surfaces that ask already have their own
 * "recorded nothing" sentence for that case.
 */
export function sessionIdOf(events: Array<{ session_id: string }>): string | null {
  return events[0]?.session_id ?? null;
}
