import { describe, expect, it } from "vitest";

import type { SessionListItem } from "../../src/tauri";
import {
  type SessionsState,
  initialSessionsState,
  visibleSessions,
} from "../../src/ui/sessions/store";

const SESSIONS: SessionListItem[] = [
  {
    id: "claude_code:/home/a.jsonl:one",
    harness: "claude_code",
    project: { Recorded: "ailly-analyzer" },
    event_count: 42,
    token_total: "Absent",
    recorded_price_micros: "Absent",
    estimated_tokens: "Absent",
    estimated_price_micros: "Absent",
    estimated_as_of: "Absent",
    last_activity: { Recorded: "2026-08-12T15:00:00Z" },
  },
  {
    id: "codex:/home/b.jsonl:two",
    harness: "codex",
    project: { Recorded: "billing-service" },
    event_count: 7,
    token_total: "Absent",
    recorded_price_micros: "Absent",
    estimated_tokens: "Absent",
    estimated_price_micros: "Absent",
    estimated_as_of: "Absent",
    last_activity: "Absent",
  },
  {
    id: "pi:/home/c.jsonl:three",
    harness: "pi",
    project: "Absent",
    event_count: 1,
    token_total: "Absent",
    recorded_price_micros: "Absent",
    estimated_tokens: "Absent",
    estimated_price_micros: "Absent",
    estimated_as_of: "Absent",
    last_activity: { Recorded: "2026-08-11T09:30:00Z" },
  },
];

function state(search: string): SessionsState {
  return { ...initialSessionsState, sessions: SESSIONS, search, indexing: false };
}

describe("visibleSessions", () => {
  it("shows all sessions when no harness token is present", () => {
    expect(visibleSessions(state(""))).toHaveLength(3);
  });

  it("still applies residual free-text when no harness token is present", () => {
    expect(visibleSessions(state("billing")).map((s) => s.harness)).toEqual(["codex"]);
  });

  it("matches nothing when harness tokens are present but none resolve", () => {
    expect(visibleSessions(state("harness: foo"))).toEqual([]);
    expect(visibleSessions(state("harness: gemini"))).toEqual([]);
  });

  it("unions resolved harnesses and ignores unrecognized companions", () => {
    expect(visibleSessions(state("harness: codex harness: foo")).map((s) => s.harness)).toEqual([
      "codex",
    ]);
  });
});
