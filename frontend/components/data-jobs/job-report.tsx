"use client";

import { CheckCircle2, Download, Loader2, Square, XCircle } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { useCancelJob } from "@/hooks/use-data-jobs";
import { errorsCsv, isFinished, progressPercent } from "@/lib/data-jobs";
import { saveBlob } from "@/lib/download";
import { formatDuration, formatNumber } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { JobView } from "@/types/data-jobs";

const SHOWN_ERRORS = 100;

function Count({ label, value, tone }: { label: string; value: number; tone?: "good" | "bad" | "muted" }) {
  return (
    <div className="min-w-20 rounded-md border bg-panel px-2.5 py-1.5">
      <p className="text-[10px] tracking-wide text-muted-foreground uppercase">{label}</p>
      <p
        className={cn(
          "text-sm font-medium tabular-nums",
          tone === "good" && value > 0 && "text-success",
          tone === "bad" && value > 0 && "text-destructive",
          tone === "muted" && "text-muted-foreground",
        )}
      >
        {formatNumber(value)}
      </p>
    </div>
  );
}

export function ProgressBar({ percent, label }: { percent: number | null; label: string }) {
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent ?? undefined}
      className="h-1.5 w-full overflow-hidden rounded-full bg-muted"
    >
      <div
        className={cn("h-full rounded-full bg-brand transition-[width]", percent === null && "w-1/3 animate-pulse")}
        style={percent === null ? undefined : { width: `${percent}%` }}
      />
    </div>
  );
}

/** Live progress of an import or mock data run, then its report. */
export function JobReport({ job }: { job: JobView }) {
  const cancel = useCancelJob();
  const finished = isFinished(job.status);
  const percent = progressPercent(job);
  const ok = job.status === "completed";

  return (
    <section className="space-y-3" aria-live="polite">
      <div className="flex flex-wrap items-center gap-2">
        {!finished ? (
          <Loader2 className="size-4 animate-spin text-brand" />
        ) : ok ? (
          <CheckCircle2 className="size-4 text-success" />
        ) : (
          <XCircle className="size-4 text-destructive" />
        )}
        <p className="text-sm font-medium">
          {!finished
            ? (job.phase ?? "Starting…")
            : ok
              ? job.committed
                ? "Completed"
                : "Completed without changes"
              : job.status === "cancelled"
                ? "Cancelled — nothing was written"
                : "Failed — nothing was written"}
        </p>
        {job.duration_ms !== null && <span className="text-xs text-muted-foreground">in {formatDuration(job.duration_ms)}</span>}
        {job.status === "running" && (
          <Button
            variant="ghost"
            size="xs"
            className="ml-auto"
            onClick={() => cancel.mutate(job.id, { onError: (e) => toast.error(e.message) })}
            disabled={cancel.isPending}
          >
            <Square className="size-3" /> Cancel
          </Button>
        )}
      </div>
      {!finished && <ProgressBar percent={percent} label="Progress" />}
      {job.error && (
        <p className="rounded-md border border-destructive/40 bg-destructive/5 p-2 text-xs text-destructive" role="alert">
          {job.error}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        {job.rows_total !== null && <Count label="Rows" value={job.rows_total} tone="muted" />}
        <Count label="Inserted" value={job.rows_inserted} tone="good" />
        {job.kind === "import" && <Count label="Updated" value={job.rows_updated} tone="good" />}
        <Count label="Skipped" value={job.rows_skipped} tone="muted" />
        {job.kind === "import" && <Count label="Failed" value={job.rows_failed} tone="bad" />}
      </div>

      {job.tables.length > 0 && (
        <table className="w-full text-xs">
          <thead className="text-left text-muted-foreground">
            <tr>
              <th className="py-1 font-medium">Table</th>
              <th className="py-1 text-right font-medium">Requested</th>
              <th className="py-1 text-right font-medium">Inserted</th>
            </tr>
          </thead>
          <tbody>
            {job.tables.map((t) => (
              <tr key={t.table} className="border-t">
                <td className="py-1 font-mono">{t.table}</td>
                <td className="py-1 text-right tabular-nums">{formatNumber(t.requested)}</td>
                <td className="py-1 text-right tabular-nums">{formatNumber(t.inserted)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {job.errors.length > 0 && (
        <div className="space-y-1.5">
          <div className="flex items-center gap-2">
            <p className="text-xs font-medium">
              Rejected rows{" "}
              <span className="font-normal text-muted-foreground">
                ({job.rows_failed > job.errors.length
                  ? `first ${formatNumber(job.errors.length)} of ${formatNumber(job.rows_failed)}`
                  : formatNumber(job.rows_failed)})
              </span>
            </p>
            <Button
              variant="ghost"
              size="xs"
              className="ml-auto"
              onClick={() => saveBlob(new Blob([errorsCsv(job.errors)], { type: "text/csv" }), `${job.file_name ?? "import"}-errors.csv`)}
            >
              <Download className="size-3" /> Errors CSV
            </Button>
          </div>
          <div className="max-h-64 overflow-auto rounded-md border">
            <table className="w-full text-xs">
              <thead className="sticky top-0 bg-panel text-left text-muted-foreground">
                <tr>
                  <th className="px-2 py-1 font-medium">Line</th>
                  <th className="px-2 py-1 font-medium">Column</th>
                  <th className="px-2 py-1 font-medium">Problem</th>
                </tr>
              </thead>
              <tbody>
                {[...job.errors]
                  .sort((a, b) => a.row - b.row)
                  .slice(0, SHOWN_ERRORS)
                  .map((e) => (
                  <tr key={`${e.row}-${e.column ?? ""}`} className="border-t align-top">
                    <td className="px-2 py-1 tabular-nums">{e.line ?? `record ${e.row}`}</td>
                    <td className="px-2 py-1 font-mono">{e.column ?? "—"}</td>
                    <td className="px-2 py-1 break-words">{e.message}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </section>
  );
}
