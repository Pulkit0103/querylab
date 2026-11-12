"use client";

import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
  type TooltipContentProps,
} from "recharts";

import { formatDuration, formatNumber } from "@/lib/format";
import type { Stats } from "@/types/library";

const AXIS = { fontSize: 11, fill: "var(--muted-foreground)" };

function dayLabel(day: string) {
  return new Date(`${day}T00:00:00Z`).toLocaleDateString(undefined, { month: "short", day: "numeric", timeZone: "UTC" });
}

function ChartTooltip({ active, payload, label, unit }: TooltipContentProps<number, string> & { unit: "count" | "ms" }) {
  if (!active || !payload?.length) return null;
  return (
    <div className="rounded-md border bg-popover px-3 py-2 text-xs shadow-md">
      <p className="mb-1 font-medium text-foreground">{dayLabel(String(label))}</p>
      {payload.map((p) => (
        <p key={String(p.dataKey)} className="flex items-center gap-2 text-muted-foreground">
          <span className="size-2 rounded-sm" style={{ background: p.color }} />
          {p.name}
          <span className="ml-auto pl-4 text-foreground tabular-nums">
            {p.value === null || p.value === undefined
              ? "—"
              : unit === "ms"
                ? formatDuration(Number(p.value))
                : formatNumber(Number(p.value))}
          </span>
        </p>
      ))}
    </div>
  );
}

/** Daily executions split into succeeded and failed (stacked; one axis). */
export function ActivityChart({ daily }: { daily: Stats["daily"] }) {
  return (
    <ResponsiveContainer width="100%" height={220}>
      <BarChart data={daily} margin={{ top: 8, right: 8, left: -16, bottom: 0 }} barCategoryGap="20%">
        <CartesianGrid vertical={false} stroke="var(--border)" />
        <XAxis dataKey="day" tickFormatter={dayLabel} tick={AXIS} axisLine={false} tickLine={false} minTickGap={16} />
        <YAxis allowDecimals={false} tick={AXIS} axisLine={false} tickLine={false} width={40} />
        <Tooltip
          cursor={{ fill: "var(--accent)", opacity: 0.5 }}
          content={(props) => <ChartTooltip {...(props as TooltipContentProps<number, string>)} unit="count" />}
        />
        <Legend
          verticalAlign="top"
          align="right"
          height={24}
          iconType="square"
          iconSize={8}
          formatter={(v) => <span className="text-xs text-muted-foreground">{v}</span>}
        />
        <Bar dataKey="successful" name="Succeeded" stackId="runs" fill="var(--viz-1)" stroke="var(--card)" strokeWidth={2} />
        <Bar
          dataKey="failed"
          name="Failed"
          stackId="runs"
          fill="var(--viz-2)"
          stroke="var(--card)"
          strokeWidth={2}
          radius={[4, 4, 0, 0]}
        />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Average execution time per day (single series, so no legend). */
export function LatencyChart({ daily }: { daily: Stats["daily"] }) {
  return (
    <ResponsiveContainer width="100%" height={220}>
      <LineChart data={daily} margin={{ top: 8, right: 8, left: -8, bottom: 0 }}>
        <CartesianGrid vertical={false} stroke="var(--border)" />
        <XAxis dataKey="day" tickFormatter={dayLabel} tick={AXIS} axisLine={false} tickLine={false} minTickGap={16} />
        <YAxis tick={AXIS} axisLine={false} tickLine={false} width={48} tickFormatter={(v: number) => formatDuration(v)} />
        <Tooltip
          cursor={{ stroke: "var(--muted-foreground)", strokeDasharray: "3 3" }}
          content={(props) => <ChartTooltip {...(props as TooltipContentProps<number, string>)} unit="ms" />}
        />
        <Line
          type="monotone"
          dataKey="avg_execution_ms"
          name="Average"
          stroke="var(--viz-1)"
          strokeWidth={2}
          dot={{ r: 3, strokeWidth: 2, stroke: "var(--card)", fill: "var(--viz-1)" }}
          activeDot={{ r: 5, strokeWidth: 2, stroke: "var(--card)" }}
          connectNulls
        />
      </LineChart>
    </ResponsiveContainer>
  );
}
