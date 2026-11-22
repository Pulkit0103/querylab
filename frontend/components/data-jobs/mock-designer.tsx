"use client";

import { useState } from "react";
import { AlertTriangle, Dices, Eye, Loader2, Play, Plus, RotateCcw, ShieldX, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { TablePicker } from "@/components/data-studio/table-picker";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useJob, useMockPreview, useMockRun } from "@/hooks/use-data-jobs";
import { defaultGenerator, describeGenerator, GENERATORS, withParents } from "@/lib/data-jobs";
import { formatNumber } from "@/lib/format";
import type { Connection } from "@/types/api";
import type { Generator, GeneratorKind, MockColumn, MockRequest, MockTable } from "@/types/data-jobs";

import { ConfirmField, confirmValue, isConfirmed } from "./confirm-field";
import { JobReport } from "./job-report";

const MAX_ROWS_PER_TABLE = 100_000;

const numberInput = "h-7 w-24 rounded-md border bg-background px-2 text-xs tabular-nums outline-none focus-visible:ring-2 focus-visible:ring-ring";

/** Inputs for a generator's parameters. */
function GeneratorParams({ generator, onChange }: { generator: Generator; onChange: (g: Generator) => void }) {
  switch (generator.kind) {
    case "integer":
    case "decimal":
      return (
        <span className="flex items-center gap-1">
          <input type="number" aria-label="Minimum" className={numberInput} value={generator.min} onChange={(e) => onChange({ ...generator, min: Number(e.target.value) })} />
          –
          <input type="number" aria-label="Maximum" className={numberInput} value={generator.max} onChange={(e) => onChange({ ...generator, max: Number(e.target.value) })} />
        </span>
      );
    case "date":
    case "timestamp":
      return (
        <span className="flex items-center gap-1">
          <input type="date" aria-label="From" className={numberInput + " w-32"} value={generator.from.slice(0, 10)} onChange={(e) => onChange({ ...generator, from: e.target.value })} />
          –
          <input type="date" aria-label="To" className={numberInput + " w-32"} value={generator.to.slice(0, 10)} onChange={(e) => onChange({ ...generator, to: e.target.value })} />
        </span>
      );
    case "boolean":
      return (
        <label className="flex items-center gap-1">
          <input
            type="number"
            min={0}
            max={100}
            aria-label="Percent true"
            className={numberInput + " w-16"}
            value={Math.round(generator.true_ratio * 100)}
            onChange={(e) => onChange({ ...generator, true_ratio: Math.min(100, Math.max(0, Number(e.target.value))) / 100 })}
          />
          % true
        </label>
      );
    case "constant":
      return (
        <input aria-label="Value" className={numberInput + " w-40 font-mono"} value={generator.value} onChange={(e) => onChange({ ...generator, value: e.target.value })} />
      );
    case "values":
      return (
        <input
          aria-label="Values, comma separated"
          className={numberInput + " w-56 font-mono"}
          value={generator.values.join(", ")}
          onChange={(e) => onChange({ ...generator, values: e.target.value.split(",").map((v) => v.trim()).filter(Boolean) })}
        />
      );
    case "sequence":
      return (
        <input
          type="number"
          aria-label="Start"
          placeholder="auto"
          className={numberInput}
          value={generator.start ?? ""}
          onChange={(e) => onChange({ ...generator, start: e.target.value === "" ? null : Number(e.target.value) })}
        />
      );
    default:
      return null;
  }
}

function ColumnRow({
  column,
  override,
  onChange,
}: {
  column: MockColumn;
  override: Generator | undefined;
  onChange: (g: Generator | undefined) => void;
}) {
  const generator = override ?? column.generator;
  const isFk = column.generator.kind === "reference";
  const options = GENERATORS.filter((g) => (g.kind === "reference" ? isFk : true));
  return (
    <tr className="border-t align-middle">
      <td className="px-2 py-1 font-mono">
        {column.name}
        {!column.nullable && <span className="ml-1 text-muted-foreground" title="NOT NULL">*</span>}
      </td>
      <td className="px-2 py-1 font-mono text-muted-foreground">{column.data_type}</td>
      <td className="px-2 py-1">
        <div className="flex flex-wrap items-center gap-2">
          <Select
            items={options.map((g) => ({ value: g.kind, label: g.label }))}
            value={generator.kind}
            onValueChange={(v) => {
              if (typeof v !== "string") return;
              const kind = v as GeneratorKind;
              onChange(kind === column.generator.kind ? undefined : defaultGenerator(kind));
            }}
          >
            <SelectTrigger size="sm" className="h-7 w-40 text-xs" aria-label={`Generator for ${column.name}`}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {options.map((g) => (
                <SelectItem key={g.kind} value={g.kind} className="text-xs">
                  {g.label}
                  <span className="ml-auto text-[10px] text-muted-foreground">{g.group}</span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {override ? (
            <GeneratorParams generator={generator} onChange={onChange} />
          ) : (
            <span className="text-[11px] text-muted-foreground">
              {isFk ? `→ ${column.references}` : describeGenerator(generator)}
              {column.note && <span className="ml-1">({column.note})</span>}
            </span>
          )}
          {override && (
            <Button variant="ghost" size="icon-xs" aria-label="Use the suggested generator" onClick={() => onChange(undefined)}>
              <RotateCcw className="size-3" />
            </Button>
          )}
        </div>
      </td>
    </tr>
  );
}

/** Pick tables, tune generators, preview, then generate in one transaction. */
export function MockDesigner({ connection }: { connection: Connection }) {
  const [tables, setTables] = useState<MockTable[]>([]);
  const [seed, setSeed] = useState<number | null>(null);
  const [typed, setTyped] = useState("");
  const [jobId, setJobId] = useState<string | null>(null);
  const preview = useMockPreview(connection.id);
  const run = useMockRun(connection.id);
  const job = useJob(jobId);

  const request = (): MockRequest => ({ tables, seed: seed ?? undefined });
  const refresh = (next: MockTable[], nextSeed = seed) => {
    if (next.length === 0) return preview.reset();
    preview.mutate(
      { tables: next, seed: nextSeed ?? undefined },
      { onSuccess: (p) => setSeed(p.seed), onError: () => undefined },
    );
  };
  const update = (next: MockTable[]) => {
    setTables(next);
    refresh(next);
  };
  const setOverride = (i: number, column: string, g: Generator | undefined) => {
    const next = tables.map((t, k) => {
      if (k !== i) return t;
      const columns = { ...(t.columns ?? {}) };
      if (g) columns[column] = g;
      else delete columns[column];
      return { ...t, columns };
    });
    setTables(next);
  };

  if (jobId && job.data) {
    return (
      <div className="space-y-4 rounded-lg border bg-background p-4">
        <h2 className="text-sm font-medium">Generating {formatNumber(job.data.rows_total ?? 0)} rows on {connection.name}</h2>
        <JobReport job={job.data} />
        {job.data.status !== "running" && (
          <Button variant="outline" size="sm" onClick={() => setJobId(null)}>
            <RotateCcw className="size-3.5" /> Back to the designer
          </Button>
        )}
      </div>
    );
  }

  const plan = preview.data;
  const blocked = plan?.production_blocked ?? false;
  const canRun =
    plan !== undefined &&
    !preview.isPending &&
    plan.missing_parents.length === 0 &&
    !blocked &&
    !connection.read_only &&
    tables.some((t) => t.rows > 0) &&
    isConfirmed(connection, typed);

  return (
    <div className="space-y-4">
      <section className="space-y-3 rounded-lg border bg-background p-4">
        <div className="flex flex-wrap items-center gap-2">
          <TablePicker
            connectionId={connection.id}
            value={null}
            onChange={(schema, table) => {
              if (tables.some((t) => t.schema === schema && t.table === table)) return;
              update([...tables, { schema, table, rows: 100 }]);
            }}
          />
          <span className="text-xs text-muted-foreground">
            <Plus className="inline size-3" /> Add the tables to fill. Parents are filled first.
          </span>
          <label className="ml-auto flex items-center gap-1.5 text-xs">
            Seed
            <input
              type="number"
              className={numberInput + " w-44"}
              value={seed ?? ""}
              placeholder="random"
              aria-label="Seed"
              onChange={(e) => setSeed(e.target.value === "" ? null : Number(e.target.value))}
            />
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="New seed"
              onClick={() => {
                const s = Math.floor(Math.random() * 2 ** 40);
                setSeed(s);
                refresh(tables, s);
              }}
            >
              <Dices className="size-3.5" />
            </Button>
          </label>
          <Button variant="outline" size="sm" onClick={() => refresh(tables)} disabled={tables.length === 0 || preview.isPending}>
            {preview.isPending ? <Loader2 className="size-3.5 animate-spin" /> : <Eye className="size-3.5" />} Preview
          </Button>
        </div>
        {tables.length > 0 && (
          <ul className="flex flex-wrap gap-2">
            {tables.map((t, i) => (
              <li key={`${t.schema}.${t.table}`} className="flex items-center gap-1.5 rounded-md border bg-panel py-1 pr-1 pl-2 text-xs">
                <span className="font-mono">
                  {t.schema}.{t.table}
                </span>
                <input
                  type="number"
                  min={0}
                  max={MAX_ROWS_PER_TABLE}
                  aria-label={`Rows for ${t.table}`}
                  className={numberInput + " h-6 w-20"}
                  value={t.rows}
                  onChange={(e) => {
                    const rows = Math.min(MAX_ROWS_PER_TABLE, Math.max(0, Math.floor(Number(e.target.value) || 0)));
                    setTables(tables.map((x, k) => (k === i ? { ...x, rows } : x)));
                  }}
                  onBlur={() => refresh(tables)}
                />
                rows
                <Button variant="ghost" size="icon-xs" aria-label={`Remove ${t.table}`} onClick={() => update(tables.filter((_, k) => k !== i))}>
                  <Trash2 className="size-3" />
                </Button>
              </li>
            ))}
          </ul>
        )}
        {preview.isError && <p className="text-xs text-destructive">{preview.error.message}</p>}
        {plan && plan.missing_parents.length > 0 && (
          <div className="flex flex-wrap items-center gap-2 rounded-md border border-warning/40 bg-warning/5 p-2 text-xs">
            <AlertTriangle className="size-3.5 text-warning" />
            {plan.missing_parents.join(", ")} {plan.missing_parents.length === 1 ? "has" : "have"} no rows to reference.
            <Button size="xs" variant="outline" onClick={() => update(withParents(tables, plan.missing_parents))}>
              Add to the run
            </Button>
          </div>
        )}
      </section>

      {plan?.tables.map((t) => {
        const index = tables.findIndex((x) => x.schema === t.schema && x.table === t.table);
        return (
          <section key={`${t.schema}.${t.table}`} className="space-y-2 rounded-lg border bg-background p-4">
            <h3 className="flex items-baseline gap-2 text-sm font-medium">
              <span className="font-mono">
                {t.schema}.{t.table}
              </span>
              <span className="text-xs font-normal text-muted-foreground">{formatNumber(t.rows)} rows</span>
            </h3>
            {t.warnings.map((w) => (
              <p key={w} className="flex items-center gap-1.5 text-xs text-warning">
                <AlertTriangle className="size-3.5" /> {w}
              </p>
            ))}
            <div className="overflow-auto rounded-md border">
              <table className="w-full text-xs">
                <thead className="bg-panel text-left text-muted-foreground">
                  <tr>
                    <th className="px-2 py-1 font-medium">Column</th>
                    <th className="px-2 py-1 font-medium">Type</th>
                    <th className="px-2 py-1 font-medium">Generator</th>
                  </tr>
                </thead>
                <tbody>
                  {t.columns.map((c) => (
                    <ColumnRow
                      key={c.name}
                      column={c}
                      override={tables[index]?.columns?.[c.name]}
                      onChange={(g) => index >= 0 && setOverride(index, c.name, g)}
                    />
                  ))}
                </tbody>
              </table>
            </div>
            {t.sample.length > 0 && (
              <details className="text-xs">
                <summary className="cursor-pointer text-muted-foreground">Sample rows</summary>
                <div className="mt-1 overflow-auto rounded-md border">
                  <table className="w-full">
                    <thead className="bg-panel text-left text-muted-foreground">
                      <tr>
                        {t.columns.map((c) => (
                          <th key={c.name} className="px-2 py-1 font-medium whitespace-nowrap">
                            {c.name}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {t.sample.map((row, r) => (
                        <tr key={r} className="border-t">
                          {row.map((v, k) => (
                            <td key={k} className="max-w-48 truncate px-2 py-1 font-mono" title={v ?? ""}>
                              {v ?? <span className="text-muted-foreground">{t.columns[k].generator.kind === "default" ? "default" : "∅"}</span>}
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </details>
            )}
          </section>
        );
      })}

      {plan && (
        <section className="space-y-3 rounded-lg border bg-background p-4">
          {blocked ? (
            <p className="flex items-center gap-1.5 text-sm text-destructive">
              <ShieldX className="size-4" /> Mock data is never generated on production connections.
            </p>
          ) : connection.read_only ? (
            <p className="text-sm text-destructive">This connection is read-only. Enable read-write mode in its settings to generate data.</p>
          ) : (
            <ConfirmField connection={connection} value={typed} onChange={setTyped} />
          )}
          <div className="flex items-center gap-3">
            <Button
              disabled={!canRun || run.isPending}
              onClick={() =>
                run.mutate(
                  { ...request(), seed: plan.seed, confirm: confirmValue(connection, typed) },
                  { onSuccess: (view) => setJobId(view.id), onError: (e) => toast.error(e.message) },
                )
              }
            >
              {run.isPending ? <Loader2 className="size-4 animate-spin" /> : <Play className="size-4" />}
              Generate {formatNumber(tables.reduce((n, t) => n + t.rows, 0))} rows
            </Button>
            <span className="text-[11px] text-muted-foreground">
              One transaction on {connection.name}. Rows that hit a unique key are skipped and counted.
            </span>
          </div>
        </section>
      )}
    </div>
  );
}
