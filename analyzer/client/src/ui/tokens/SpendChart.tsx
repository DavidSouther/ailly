import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { SPEND_STROKE, tokenLabel } from "./palette";
import type { SpendReading, SpendSeriesPoint } from "./rollup";

const SERIES_HEIGHT = 200;

/**
 * The overview: orchestrator and subagent spend stacked across session time.
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
        No spend could be placed on a time axis: nothing recorded a timestamp.
      </p>
    );
  }
  return (
    <div className="w-full" style={{ height: SERIES_HEIGHT }}>
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={series} margin={{ top: 8, right: 8, bottom: 0, left: 8 }}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis
            dataKey="time"
            type="number"
            scale="time"
            domain={["auto", "auto"]}
            tickFormatter={clockTime}
          />
          <YAxis tickFormatter={tokenLabel} />
          <Tooltip labelFormatter={formatClock} formatter={formatTokens} />
          {/* Both stroke and fill, per Recharts' own pitfall: an Area with only
              one of them draws as a bare line or a bare region. */}
          <Area
            type={reading === "cumulative" ? "monotone" : "linear"}
            dataKey="orchestrator"
            name="Orchestrator"
            stackId="spend"
            stroke={SPEND_STROKE.orchestrator}
            fill={SPEND_STROKE.orchestrator}
            isAnimationActive={false}
          />
          <Area
            type={reading === "cumulative" ? "monotone" : "linear"}
            dataKey="subagent"
            name="Subagent"
            stackId="spend"
            stroke={SPEND_STROKE.subagent}
            fill={SPEND_STROKE.subagent}
            isAnimationActive={false}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

/**
 * Local wall-clock, matching the session list's own timestamps: a UTC axis here
 * would read against the session header's local one.
 */
function clockTime(time: number): string {
  return new Date(time).toLocaleString(undefined, { hour: "numeric", minute: "2-digit" });
}

/** Recharts hands its formatters a loose `ReactNode`, so both narrow before use. */
function formatClock(label: unknown): string {
  return typeof label === "number" ? clockTime(label) : "";
}

function formatTokens(value: unknown): string {
  return typeof value === "number" ? tokenLabel(value) : "";
}
