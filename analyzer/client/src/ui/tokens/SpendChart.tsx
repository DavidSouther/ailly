import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { SPEND_STROKE, tokenLabel } from "./palette";
import type { SpendReading, SpendSeriesPoint } from "./rollup";

const SERIES_HEIGHT = 200;
const MARGIN = { top: 8, right: 8, bottom: 0, left: 8 } as const;

/**
 * The overview: orchestrator and subagent spend along the conversation.
 *
 * The two readings ask different questions, so they draw differently. Per
 * response is a discrete amount per message, which bars read honestly and a
 * continuous shape would imply spend between messages. Cumulative is a running
 * total, so it stays a stacked area: the shape is the point, and the top of the
 * stack is the session total.
 *
 * Presentation only — every figure it draws comes from `spendSeries`, which is
 * where the arithmetic is tested. Under jsdom this renders nothing at all, since
 * the container has no measurable size, which is why the ranked list beside it
 * is the accessible route to the same facts rather than a convenience.
 */
export function SpendChart({
  series,
  reading,
}: { series: SpendSeriesPoint[]; reading: SpendReading }) {
  if (series.length === 0) {
    return (
      <p className="text-foreground-muted text-xs">
        No spend to chart: nothing in the session recorded token usage.
      </p>
    );
  }
  return (
    <div className="w-full" style={{ height: SERIES_HEIGHT }}>
      <ResponsiveContainer width="100%" height="100%">
        {reading === "per-response" ? (
          <BarChart data={series} margin={MARGIN}>
            <CartesianGrid strokeDasharray="3 3" />
            {/* A band axis, not the numeric one the cumulative reading uses:
                bars need a band to size themselves against. */}
            <XAxis dataKey="message" tickFormatter={tickFormatter} />
            <YAxis tickFormatter={tokenLabel} />
            <Tooltip labelFormatter={formatLabel} formatter={formatTokens} />
            <Bar
              dataKey="orchestrator"
              name="Orchestrator"
              stackId="spend"
              fill={SPEND_STROKE.orchestrator}
              isAnimationActive={false}
            />
            <Bar
              dataKey="subagent"
              name="Subagent"
              stackId="spend"
              fill={SPEND_STROKE.subagent}
              isAnimationActive={false}
            />
          </BarChart>
        ) : (
          <AreaChart data={series} margin={MARGIN}>
            <CartesianGrid strokeDasharray="3 3" />
            <XAxis
              dataKey="message"
              type="number"
              allowDecimals={false}
              tickFormatter={tickFormatter}
            />
            <YAxis tickFormatter={tokenLabel} />
            <Tooltip labelFormatter={formatLabel} formatter={formatTokens} />
            {/* Stacked, so the top of the stack is the session's running total
                and each band is one party's share of it. Both stroke and fill,
                per Recharts' own pitfall: an Area with only one of them draws as
                a bare line or a bare region. */}
            <Area
              type="monotone"
              dataKey="orchestrator"
              name="Orchestrator"
              stackId="spend"
              stroke={SPEND_STROKE.orchestrator}
              fill={SPEND_STROKE.orchestrator}
              isAnimationActive={false}
            />
            <Area
              type="monotone"
              dataKey="subagent"
              name="Subagent"
              stackId="spend"
              stroke={SPEND_STROKE.subagent}
              fill={SPEND_STROKE.subagent}
              isAnimationActive={false}
            />
          </AreaChart>
        )}
      </ResponsiveContainer>
    </div>
  );
}

/** A tick is the bare message number: the axis is a dense index, not a clock. */
function tickFormatter(message: number): string {
  return String(message);
}

/**
 * Local wall-clock, matching the session list's own timestamps: a UTC axis here
 * would read against the session header's local one.
 */
function clockTime(time: number): string {
  return new Date(time).toLocaleString(undefined, { hour: "numeric", minute: "2-digit" });
}

/**
 * The tooltip names the message; clock time is a secondary fact, appended
 * only when the point's own timestamp parsed.
 */
function tooltipLabel(point: SpendSeriesPoint): string {
  const base = `Message ${point.message}`;
  return point.time === null ? base : `${base} · ${clockTime(point.time)}`;
}

/** Recharts hands its label formatter the row alongside a loose `ReactNode` label. */
function formatLabel(_label: unknown, payload: readonly { payload?: SpendSeriesPoint }[]): string {
  const point = payload?.[0]?.payload;
  return point ? tooltipLabel(point) : "";
}

function formatTokens(value: unknown): string {
  return typeof value === "number" ? tokenLabel(value) : "";
}
