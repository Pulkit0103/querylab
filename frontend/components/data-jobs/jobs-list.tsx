"use client";

import { useState } from "react";
import { History } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { useJob, useJobs } from "@/hooks/use-data-jobs";
import { formatDuration, formatNumber, formatRelativeTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { JobKind, JobRecord, JobStatus } from "@/types/data-jobs";

import { JobReport } from "./job-report";

const KIND_LABEL: Record<JobKind, string> = { import: "Import", export: "Export", mock_data: "Mock data" };

const STATUS_CLASS: Partial<Record<JobStatus, string>> = {
  completed: "bg-success/15 text-success",
  failed: "bg-destructive/15 text-destructive",
  cancelled: "bg-muted text-muted-foreground",
  running: "bg-brand/15 text-brand",
};

function duration(j: JobRecord): string {
  if (!j.finished_at) return "—";
  return formatDuration(new Date(j.finished_at).getTime() - new Date(j.started_at).getTime());
}

function JobDialog({ id, onClose }: { id: string | null; onClose: () => void }) {
  const job = useJob(id);
  return (
    <Dialog open={id !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {job.data ? `${KIND_LABEL[job.data.kind]} · ${job.data.target ?? job.data.file_name ?? ""}` : "Job"}
          </DialogTitle>
        </DialogHeader>
        {job.data ? <JobReport job={job.data} /> : job.isError ? <p className="text-sm text-destructive">{job.error.message}</p> : <Skeleton className="h-32" />}
      </DialogContent>
    </Dialog>
  );
}

/** Recent imports, exports and mock data runs. */
export function JobsList() {
  const jobs = useJobs();
  const [open, setOpen] = useState<string | null>(null);
  if (jobs.isPending) return <Skeleton className="h-48" />;
  if (jobs.isError) return <p className="text-sm text-destructive">{jobs.error.message}</p>;
  if (jobs.data.length === 0) {
    return <EmptyState icon={History} title="No jobs yet" description="Imports, exports and mock data runs appear here." />;
  }
  return (
    <>
      <div className="overflow-auto rounded-lg border bg-background">
        <table className="w-full text-xs">
          <thead className="bg-panel text-left text-muted-foreground">
            <tr>
              <th className="px-3 py-2 font-medium">Kind</th>
              <th className="px-3 py-2 font-medium">Target</th>
              <th className="px-3 py-2 font-medium">Connection</th>
              <th className="px-3 py-2 font-medium">Status</th>
              <th className="px-3 py-2 text-right font-medium">Rows</th>
              <th className="px-3 py-2 text-right font-medium">Failed</th>
              <th className="px-3 py-2 font-medium">Started</th>
              <th className="px-3 py-2 text-right font-medium">Duration</th>
            </tr>
          </thead>
          <tbody>
            {jobs.data.map((j) => (
              <tr
                key={j.id}
                className="cursor-pointer border-t hover:bg-muted/40 focus-visible:bg-muted/40 focus-visible:outline-none"
                tabIndex={0}
                onClick={() => setOpen(j.id)}
                onKeyDown={(e) => e.key === "Enter" && setOpen(j.id)}
              >
                <td className="px-3 py-1.5">{KIND_LABEL[j.kind]}</td>
                <td className="max-w-72 truncate px-3 py-1.5 font-mono" title={j.target ?? ""}>
                  {j.target ?? "—"}
                </td>
                <td className="px-3 py-1.5">{j.connection_name ?? <span className="text-muted-foreground">deleted</span>}</td>
                <td className="px-3 py-1.5">
                  <span className={cn("rounded px-1.5 py-0.5 text-[10px] font-medium", STATUS_CLASS[j.status])}>{j.status}</span>
                </td>
                <td className="px-3 py-1.5 text-right tabular-nums">{formatNumber(j.rows_processed)}</td>
                <td className={cn("px-3 py-1.5 text-right tabular-nums", j.rows_failed > 0 && "text-destructive")}>
                  {formatNumber(j.rows_failed)}
                </td>
                <td className="px-3 py-1.5 whitespace-nowrap text-muted-foreground">{formatRelativeTime(j.started_at)}</td>
                <td className="px-3 py-1.5 text-right tabular-nums">{duration(j)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <JobDialog id={open} onClose={() => setOpen(null)} />
    </>
  );
}
