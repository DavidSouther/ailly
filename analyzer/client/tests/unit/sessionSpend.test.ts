import { describe, expect, it } from "vitest";

import type { SessionTokenFigures } from "../../src/tauri";
import {
  NO_SPEND,
  foldSessionSpend,
  priceReading,
  rowTokens,
  usdLabel,
} from "../../src/ui/tokens/sessionSpend";

const NOTHING: SessionTokenFigures = {
  token_total: "Absent",
  recorded_price_micros: "Absent",
  estimated_tokens: "Absent",
  estimated_price_micros: "Absent",
  estimated_as_of: "Absent",
};

function row(figures: Partial<SessionTokenFigures>): SessionTokenFigures {
  return { ...NOTHING, ...figures };
}

/**
 * These are the surfaces' whole account of dollars: the indexer decides what a
 * session cost, and this module only adds those decisions up and says which
 * kind of figure the sum is. Every rule below is about not blurring the two
 * kinds together, or reporting a partial sum as a complete one.
 */
describe("session spend", () => {
  it("keeps a harness's own price and the index's estimate in separate accounts", () => {
    const spend = foldSessionSpend([
      row({ recorded_price_micros: { Recorded: 17_900 } }),
      row({
        estimated_price_micros: { Recorded: 15_000 },
        estimated_as_of: { Recorded: "2026-08-14" },
      }),
    ]);

    expect(spend).toEqual({
      recordedMicros: 17_900,
      estimatedMicros: 15_000,
      recordedCount: 1,
      estimatedCount: 1,
      unpricedCount: 0,
      estimatedAsOf: ["2026-08-14"],
    });
  });

  /**
   * The index only estimates when it found no recorded cost, but a row carrying
   * both must still resolve one way: the figure someone was billed is the fact,
   * and adding the estimate to it would double the session's price.
   */
  it("takes the recorded price of a row that carries both", () => {
    const spend = foldSessionSpend([
      row({
        recorded_price_micros: { Recorded: 17_900 },
        estimated_price_micros: { Recorded: 15_000 },
      }),
    ]);

    expect(spend.recordedMicros).toBe(17_900);
    expect(spend.estimatedMicros).toBe(0);
    expect(spend.estimatedCount).toBe(0);
  });

  /**
   * A session that spent tokens nobody could price is a hole in the total, and
   * the fold counts it so the surfaces can say so. A session that recorded no
   * spend at all is not a hole — there is nothing there to price.
   */
  it("counts a priceless spend as a shortfall and an empty session as neither", () => {
    const spend = foldSessionSpend([row({ token_total: { Recorded: 1000 } }), row({})]);

    expect(spend.unpricedCount).toBe(1);
    expect(spend.recordedCount + spend.estimatedCount).toBe(0);
  });

  it("folds no rows into no spend", () => {
    expect(foldSessionSpend([])).toEqual(NO_SPEND);
  });

  /**
   * Both figures are sums of recorded parts rather than rate arithmetic, so the
   * choice between them is about which the harness stated more directly, not
   * about confidence.
   */
  it("prefers a harness's own token total to the sum the index derived", () => {
    expect(
      rowTokens(row({ token_total: { Recorded: 1000 }, estimated_tokens: { Recorded: 1200 } })),
    ).toBe(1000);
    expect(rowTokens(row({ estimated_tokens: { Recorded: 1200 } }))).toBe(1200);
    expect(rowTokens(row({}))).toBeNull();
  });

  describe("reading a price", () => {
    it("marks a sum with any estimate in it as approximate, and names both bases", () => {
      const reading = priceReading(
        foldSessionSpend([
          row({ recorded_price_micros: { Recorded: 17_900 } }),
          row({ estimated_price_micros: { Recorded: 15_000 } }),
        ]),
      );

      expect(reading.label).toBe("≈$0.0329");
      expect(reading.approximate).toBe(true);
      expect(reading.basis).toBe("$0.0179 (Recorded); $0.0150 (Est)");
    });

    it("states a wholly recorded price without the approximation mark", () => {
      const reading = priceReading(
        foldSessionSpend([row({ recorded_price_micros: { Recorded: 17_900 } })]),
      );

      expect(reading.label).toBe("$0.0179");
      expect(reading.approximate).toBe(false);
      expect(reading.basis).toBe("$0.0179 (Recorded)");
    });

    /**
     * Zero dollars is a claim about the spend. Nothing priced means nobody could
     * make that claim, so the amount says so instead of reporting `$0.00`.
     */
    it("withholds an amount when nothing could be priced", () => {
      const reading = priceReading(foldSessionSpend([row({ token_total: { Recorded: 1000 } })]));

      expect(reading.label).toBe("Not recorded");
      expect(reading.approximate).toBe(false);
      expect(reading.basis).toBe("no price for 1 session");
    });

    /** A price that covers only part of the spend says which part it missed. */
    it("names the sessions the price does not cover", () => {
      const reading = priceReading(
        foldSessionSpend([
          row({ estimated_price_micros: { Recorded: 15_000 } }),
          row({ token_total: { Recorded: 1000 } }),
          row({ estimated_tokens: { Recorded: 2000 } }),
        ]),
      );

      expect(reading.label).toBe("≈$0.0150");
      expect(reading.basis).toBe("$0.0150 (Est); no price for 2 sessions");
    });

    /**
     * The index refreshes its rate table; a written estimate never changes. Left
     * unsaid, a reader would take an estimate for one made against today's
     * rates, which for an old session it is not.
     */
    it("names the day's rates an estimate was priced against", () => {
      const reading = priceReading(
        foldSessionSpend([
          row({
            estimated_price_micros: { Recorded: 15_000 },
            estimated_as_of: { Recorded: "2026-08-14" },
          }),
        ]),
      );

      expect(reading.basis).toBe("$0.0150 (Est, 2026-08-14)");
    });

    /**
     * A sum can cover sessions the index priced weeks apart, each keeping the
     * rates it was given. Neither end of that span is true of the whole figure,
     * so both are named.
     */
    it("names the span when the sessions in one sum were priced on different days", () => {
      const reading = priceReading(
        foldSessionSpend([
          row({
            estimated_price_micros: { Recorded: 15_000 },
            estimated_as_of: { Recorded: "2026-08-14" },
          }),
          row({
            estimated_price_micros: { Recorded: 15_000 },
            estimated_as_of: { Recorded: "2026-07-01" },
          }),
        ]),
      );

      expect(reading.basis).toBe("$0.0300 (Est, 2026-07-01 to 2026-08-14)");
    });

    /**
     * An estimate written before the index stored the date has no date to name,
     * and inventing today's would date it to rates it was never priced against.
     */
    it("says nothing about rates for an estimate that carries no date", () => {
      const reading = priceReading(
        foldSessionSpend([row({ estimated_price_micros: { Recorded: 15_000 } })]),
      );

      expect(reading.basis).toBe("$0.0150 (Est)");
    });
  });

  /**
   * Sub-cent spend is the ordinary case for a short session, so a fixed two
   * decimals would print most of this lens as `$0.00`. Each tier shows enough
   * digits to distinguish the amounts that land in it, and no more.
   */
  describe("a dollar amount", () => {
    it("shows cents once the spend reaches a dollar", () => {
      expect(usdLabel(1_234_500_000)).toBe("$1,234.50");
      expect(usdLabel(1_000_000)).toBe("$1.00");
    });

    it("shows four decimals for cents and six for fractions of one", () => {
      expect(usdLabel(15_000)).toBe("$0.0150");
      expect(usdLabel(10_000)).toBe("$0.0100");
      expect(usdLabel(1_500)).toBe("$0.001500");
    });

    /** A cost too small to print is still a cost, so it is never rounded away. */
    it("refuses to round a real amount down to nothing", () => {
      expect(usdLabel(0)).toBe("$0.00");
      expect(usdLabel(1)).toBe("$0.000001");
      expect(usdLabel(0.5)).toBe("<$0.000001");
    });
  });
});
