import { describe, expect, it } from "vitest";

import { lastActivityLabel } from "../../src/ui/sessions/format";

/** Local noon on a calendar day offset from today — avoids UTC-edge flakes. */
function localIsoDaysAgo(daysAgo: number, hour = 13, minute = 45): string {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() - daysAgo);
  d.setHours(hour, minute, 30, 0);
  return d.toISOString();
}

function timePart(raw: string): string {
  return new Date(raw).toLocaleString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });
}

function compactDay(raw: string, sameYear: boolean): string {
  return new Date(raw).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

describe("lastActivityLabel", () => {
  it('renders today as "Today" with hour:minute', () => {
    const raw = localIsoDaysAgo(0);
    expect(lastActivityLabel({ Recorded: raw })).toBe(`Today, ${timePart(raw)}`);
  });

  it('renders yesterday as "Yesterday" with hour:minute', () => {
    const raw = localIsoDaysAgo(1);
    expect(lastActivityLabel({ Recorded: raw })).toBe(`Yesterday, ${timePart(raw)}`);
  });

  it("renders 2–5 days ago as the weekday with hour:minute", () => {
    for (const daysAgo of [2, 3, 4, 5]) {
      const raw = localIsoDaysAgo(daysAgo);
      const weekday = new Date(raw).toLocaleString(undefined, { weekday: "long" });
      expect(lastActivityLabel({ Recorded: raw })).toBe(`${weekday}, ${timePart(raw)}`);
    }
  });

  it("renders older current-year stamps as day/month and hour:minute", () => {
    const raw = localIsoDaysAgo(10);
    expect(lastActivityLabel({ Recorded: raw })).toBe(`${compactDay(raw, true)}, ${timePart(raw)}`);
  });

  it("renders a prior-year timestamp with the year included", () => {
    const raw = "2020-06-15T09:05:59.000Z";
    expect(lastActivityLabel({ Recorded: raw })).toBe(
      `${compactDay(raw, false)}, ${timePart(raw)}`,
    );
    expect(lastActivityLabel({ Recorded: raw })).toContain("2020");
  });

  it("renders a recorded unparseable string verbatim", () => {
    const raw = "not-a-timestamp";
    expect(lastActivityLabel({ Recorded: raw })).toBe(raw);
  });

  it('renders Absent as "No timestamp"', () => {
    expect(lastActivityLabel("Absent")).toBe("No timestamp");
  });
});
