"use client";

import { useRef, useState } from "react";
import { AlertTriangle, FileUp, Loader2, Play, RotateCcw, XCircle } from "lucide-react";
import { toast } from "sonner";

import { TablePicker } from "@/components/data-studio/table-picker";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  useAnalyzeImport,
  useCheckImport,
  useCreateImport,
  useDiscardImport,
  useJob,
  useRunImport,
} from "@/hooks/use-data-jobs";
import { ApiError } from "@/lib/api-client";
import { issuesFor, setMapping, TYPE_LABEL, uploadChunks } from "@/lib/data-jobs";
import { formatBytes, formatNumber } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { Connection } from "@/types/api";
import type {
  CheckImportResponse,
  ColumnMapping,
  DuplicateMode,
  ErrorMode,
  FileFormat,
  ImportPreview,
  ParseOptions,
} from "@/types/data-jobs";

import { ConfirmField, confirmValue, isConfirmed } from "./confirm-field";
import { JobReport, ProgressBar } from "./job-report";

const MAX_FILE_BYTES = 50 * 1024 * 1024;
const SKIP = "__skip__";

function Step({ n, title, children, disabled }: { n: number; title: string; children: React.ReactNode; disabled?: boolean }) {
  return (
    <section className={cn("space-y-3 rounded-lg border bg-background p-4", disabled && "pointer-events-none opacity-50")} inert={disabled}>
      <h2 className="flex items-center gap-2 text-sm font-medium">
        <span className="flex size-5 items-center justify-center rounded-full bg-brand/15 text-[11px] text-brand">{n}</span>
        {title}
      </h2>
      {children}
    </section>
  );
}

function Choice<T extends string>({
  label,
  value,
  options,
  onChange,
  className,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  className?: string;
}) {
  return (
    <Select items={options} value={value} onValueChange={(v) => typeof v === "string" && onChange(v as T)}>
      <SelectTrigger size="sm" className={cn("h-8 text-xs", className)} aria-label={label}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value} className="text-xs">
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/** Upload → preview → map → run, for one file into one table. */
export function ImportWizard({ connection }: { connection: Connection }) {
  const fileInput = useRef<HTMLInputElement>(null);
  const create = useCreateImport(connection.id);
  const analyze = useAnalyzeImport();
  const checkImport = useCheckImport();
  const run = useRunImport();
  const discard = useDiscardImport();

  const [file, setFile] = useState<File | null>(null);
  const [importId, setImportId] = useState<string | null>(null);
  const [sent, setSent] = useState(0);
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [options, setOptions] = useState<ParseOptions>({ format: "auto", delimiter: null, has_header: true });
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [target, setTarget] = useState<{ schema: string; table: string } | null>(null);
  const [check, setCheck] = useState<CheckImportResponse | null>(null);
  const [mapping, setMappingState] = useState<ColumnMapping[]>([]);
  const [duplicateMode, setDuplicateMode] = useState<DuplicateMode>("error");
  const [conflictKey, setConflictKey] = useState<string>("");
  const [errorMode, setErrorMode] = useState<ErrorMode>("all_or_nothing");
  const [emptyAsNull, setEmptyAsNull] = useState(true);
  const [typed, setTyped] = useState("");
  const [jobId, setJobId] = useState<string | null>(null);
  const job = useJob(jobId);

  const reset = () => {
    if (importId && !jobId) discard.mutate(importId);
    setFile(null);
    setImportId(null);
    setSent(0);
    setUploadError(null);
    setPreview(null);
    setTarget(null);
    setCheck(null);
    setMappingState([]);
    setTyped("");
    setJobId(null);
    if (fileInput.current) fileInput.current.value = "";
  };

  const runAnalyze = (id: string, opts: ParseOptions) =>
    analyze.mutate(
      { id, options: opts },
      {
        onSuccess: (p) => {
          setPreview(p);
          setCheck(null);
          setMappingState([]);
          if (target) recheck(id, target, undefined);
        },
        onError: () => setPreview(null),
      },
    );

  const pick = async (f: File) => {
    reset();
    setFile(f);
    if (f.size === 0) return setUploadError("The file is empty.");
    if (f.size > MAX_FILE_BYTES) return setUploadError(`Files up to ${formatBytes(MAX_FILE_BYTES)} can be imported.`);
    setUploading(true);
    try {
      const created = await create.mutateAsync({ file_name: f.name, size: f.size });
      setImportId(created.import_id);
      await uploadChunks(created.import_id, f, created.max_chunk_bytes, setSent);
      const opts: ParseOptions = { format: "auto", delimiter: null, has_header: true };
      setOptions(opts);
      runAnalyze(created.import_id, opts);
    } catch (e) {
      setUploadError(e instanceof ApiError || e instanceof Error ? e.message : "The upload failed.");
    } finally {
      setUploading(false);
    }
  };

  const recheck = (id: string, t: { schema: string; table: string }, m: ColumnMapping[] | undefined) =>
    checkImport.mutate(
      { id, schema: t.schema, table: t.table, mapping: m },
      {
        onSuccess: (c) => {
          setCheck(c);
          setMappingState(c.mapping);
          const keys = c.unique_keys.map((k) => k.join(","));
          setConflictKey((k) => (keys.includes(k) ? k : (keys[0] ?? "")));
        },
        onError: (e) => toast.error(e.message),
      },
    );

  const changeMapping = (targetColumn: string, source: number | null) => {
    if (!importId || !target) return;
    const next = setMapping(mapping, targetColumn, source);
    setMappingState(next);
    recheck(importId, target, next);
  };

  const errors = check?.issues.filter((i) => i.severity === "error") ?? [];
  const canRun =
    importId !== null &&
    check !== null &&
    errors.length === 0 &&
    mapping.length > 0 &&
    !connection.read_only &&
    check.can_insert &&
    (duplicateMode !== "update" || (conflictKey !== "" && check.can_update)) &&
    isConfirmed(connection, typed);

  const start = () => {
    if (!importId || !target || !canRun) return;
    run.mutate(
      {
        id: importId,
        body: {
          schema: target.schema,
          table: target.table,
          mapping,
          duplicate_mode: duplicateMode,
          conflict_columns: duplicateMode === "error" || !conflictKey ? null : conflictKey.split(","),
          error_mode: errorMode,
          empty_as_null: emptyAsNull,
          confirm: confirmValue(connection, typed),
        },
      },
      { onSuccess: (view) => setJobId(view.id), onError: (e) => toast.error(e.message) },
    );
  };

  if (jobId && job.data) {
    return (
      <div className="space-y-4">
        <Step n={4} title={`Import ${file?.name ?? ""} into ${target?.schema}.${target?.table}`}>
          <JobReport job={job.data} />
        </Step>
        {job.data.status !== "running" && (
          <Button variant="outline" size="sm" onClick={reset}>
            <RotateCcw className="size-3.5" /> Import another file
          </Button>
        )}
      </div>
    );
  }

  const sourceItems = [
    { value: SKIP, label: "— not imported —" },
    ...(preview?.columns.map((c) => ({ value: String(c.index), label: `${c.name} (${TYPE_LABEL[c.inferred_type]})` })) ?? []),
  ];

  return (
    <div className="space-y-4">
      <Step n={1} title="Choose a file">
        <div className="flex flex-wrap items-center gap-3">
          <input
            ref={fileInput}
            type="file"
            accept=".csv,.tsv,.txt,.json,.ndjson,.jsonl,text/csv,application/json"
            className="sr-only"
            id="import-file"
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void pick(f);
            }}
          />
          <Button variant="outline" size="sm" onClick={() => fileInput.current?.click()} disabled={uploading}>
            <FileUp className="size-3.5" /> {file ? "Choose another file" : "Choose CSV or JSON file"}
          </Button>
          {file && (
            <span className="text-xs text-muted-foreground">
              {file.name} · {formatBytes(file.size)}
            </span>
          )}
          <span className="ml-auto text-[11px] text-muted-foreground">
            CSV (any delimiter) or JSON (an array of objects or one object per line), up to {formatBytes(MAX_FILE_BYTES)}
          </span>
        </div>
        {uploading && <ProgressBar percent={file ? Math.round((sent / file.size) * 100) : null} label="Upload" />}
        {uploadError && (
          <p className="flex items-center gap-1.5 text-xs text-destructive" role="alert">
            <XCircle className="size-3.5" /> {uploadError}
          </p>
        )}
      </Step>

      <Step n={2} title="Check the data" disabled={!importId || uploading}>
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <Choice<FileFormat>
            label="Format"
            value={options.format ?? "auto"}
            options={[
              { value: "auto", label: "Detect format" },
              { value: "csv", label: "CSV" },
              { value: "json", label: "JSON" },
            ]}
            onChange={(format) => {
              const next = { ...options, format };
              setOptions(next);
              if (importId) runAnalyze(importId, next);
            }}
            className="w-32"
          />
          {preview?.format === "csv" && (
            <>
              <Choice
                label="Delimiter"
                value={preview.delimiter === "\t" ? "tab" : (preview.delimiter ?? ",")}
                options={[
                  { value: ",", label: "Comma ," },
                  { value: ";", label: "Semicolon ;" },
                  { value: "tab", label: "Tab" },
                  { value: "|", label: "Pipe |" },
                ]}
                onChange={(d) => {
                  const next = { ...options, delimiter: d === "tab" ? "\t" : d };
                  setOptions(next);
                  if (importId) runAnalyze(importId, next);
                }}
                className="w-32"
              />
              <label className="flex items-center gap-1.5">
                <Switch
                  size="sm"
                  checked={options.has_header ?? true}
                  onCheckedChange={(v) => {
                    const next = { ...options, has_header: v };
                    setOptions(next);
                    if (importId) runAnalyze(importId, next);
                  }}
                  aria-label="First row is a header"
                />
                First row is a header
              </label>
            </>
          )}
          {analyze.isPending && <Loader2 className="size-3.5 animate-spin text-muted-foreground" />}
          {preview && (
            <span className="ml-auto text-muted-foreground">
              {formatNumber(preview.total_rows)} rows · types inferred from {formatNumber(preview.sampled_rows)}
            </span>
          )}
        </div>
        {analyze.isError && <p className="text-xs text-destructive">{analyze.error.message}</p>}
        {preview && (
          <>
            <div className="max-h-72 overflow-auto rounded-md border">
              <table className="w-full text-xs">
                <thead className="sticky top-0 bg-panel">
                  <tr>
                    {preview.columns.map((c) => (
                      <th key={c.index} className="border-b px-2 py-1 text-left font-medium whitespace-nowrap">
                        {c.name}{" "}
                        <span className="rounded bg-muted px-1 font-mono text-[10px] font-normal text-muted-foreground">{TYPE_LABEL[c.inferred_type]}</span>
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.map((row, i) => (
                    <tr key={i} className="border-b last:border-0">
                      {row.map((v, j) => (
                        <td key={j} className="max-w-56 truncate px-2 py-1 font-mono" title={v ?? ""}>
                          {v === null || v === "" ? <span className="text-muted-foreground">∅</span> : v}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {preview.parse_error_count > 0 && (
              <p className="flex items-center gap-1.5 text-xs text-warning">
                <AlertTriangle className="size-3.5" />
                {formatNumber(preview.parse_error_count)} rows cannot be read (first on line {preview.parse_errors[0]?.line ?? preview.parse_errors[0]?.row}:{" "}
                {preview.parse_errors[0]?.message}). They are reported as failed rows.
              </p>
            )}
          </>
        )}
      </Step>

      <Step n={3} title="Map columns to a table" disabled={!preview}>
        <div className="flex flex-wrap items-center gap-2">
          <TablePicker
            connectionId={connection.id}
            value={target}
            onChange={(schema, table) => {
              setTarget({ schema, table });
              if (importId) recheck(importId, { schema, table }, undefined);
            }}
          />
          {checkImport.isPending && <Loader2 className="size-3.5 animate-spin text-muted-foreground" />}
          {check && (
            <span className="text-xs text-muted-foreground">
              {mapping.length} of {check.columns.filter((c) => !c.generated).length} columns mapped
            </span>
          )}
        </div>
        {check && (
          <div className="overflow-auto rounded-md border">
            <table className="w-full text-xs">
              <thead className="bg-panel text-left text-muted-foreground">
                <tr>
                  <th className="px-2 py-1 font-medium">Column</th>
                  <th className="px-2 py-1 font-medium">Type</th>
                  <th className="px-2 py-1 font-medium">From file</th>
                  <th className="px-2 py-1 font-medium">Notes</th>
                </tr>
              </thead>
              <tbody>
                {check.columns.map((c) => {
                  const m = mapping.find((x) => x.target === c.name);
                  const notes = issuesFor(check.issues, c.name, null);
                  return (
                    <tr key={c.name} className="border-t align-top">
                      <td className="px-2 py-1.5 font-mono">
                        {c.name}
                        {c.required && <span className="ml-1 text-destructive" title="Required">*</span>}
                      </td>
                      <td className="px-2 py-1.5 font-mono text-muted-foreground">
                        {c.data_type}
                        {c.identity && <span className="ml-1 text-[10px]">identity</span>}
                        {c.has_default && !c.identity && <span className="ml-1 text-[10px]">default</span>}
                      </td>
                      <td className="px-2 py-1">
                        {c.generated ? (
                          <span className="text-muted-foreground">generated</span>
                        ) : (
                          <Choice
                            label={`Source for ${c.name}`}
                            value={m ? String(m.source) : SKIP}
                            options={sourceItems}
                            onChange={(v) => changeMapping(c.name, v === SKIP ? null : Number(v))}
                            className="h-7 w-52"
                          />
                        )}
                      </td>
                      <td className="px-2 py-1.5">
                        {notes.map((n) => (
                          <p key={n.message} className={n.severity === "error" ? "text-destructive" : "text-warning"}>
                            {n.message}
                          </p>
                        ))}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
        {check?.issues
          .filter((i) => i.target === null || !check.columns.some((c) => c.name === i.target))
          .map((i) => (
            <p key={i.message} className="text-xs text-destructive">
              {i.message}
            </p>
          ))}
      </Step>

      <Step n={4} title="Options and run" disabled={!check}>
        <div className="grid gap-3 text-xs sm:grid-cols-2">
          <div className="space-y-1">
            <p className="font-medium">Rows whose key already exists</p>
            <div className="flex flex-wrap gap-2">
              <Choice<DuplicateMode>
                label="Duplicate handling"
                value={duplicateMode}
                options={[
                  { value: "error", label: "Report as errors" },
                  { value: "skip", label: "Skip them" },
                  { value: "update", label: "Update them" },
                ]}
                onChange={setDuplicateMode}
                className="w-40"
              />
              {duplicateMode !== "error" && check && check.unique_keys.length > 0 && (
                <Choice
                  label="Key"
                  value={conflictKey}
                  options={check.unique_keys.map((k) => ({ value: k.join(","), label: `Key: ${k.join(", ")}` }))}
                  onChange={setConflictKey}
                  className="w-48"
                />
              )}
            </div>
            {duplicateMode === "update" && check && check.unique_keys.length === 0 && (
              <p className="text-destructive">This table has no primary key or unique index to match rows on.</p>
            )}
          </div>
          <div className="space-y-1">
            <p className="font-medium">Invalid rows</p>
            <Choice<ErrorMode>
              label="Error handling"
              value={errorMode}
              options={[
                { value: "all_or_nothing", label: "Import nothing if any row is invalid" },
                { value: "skip_invalid", label: "Skip invalid rows and report them" },
              ]}
              onChange={setErrorMode}
              className="w-72"
            />
          </div>
          <label className="flex items-center gap-2">
            <Switch size="sm" checked={emptyAsNull} onCheckedChange={setEmptyAsNull} aria-label="Empty values are NULL" />
            Empty values are NULL
          </label>
        </div>
        {connection.read_only && (
          <p className="text-xs text-destructive">This connection is read-only. Enable read-write mode in its settings to import data.</p>
        )}
        {check && !check.can_insert && (
          <p className="text-xs text-destructive">The connection&apos;s role cannot insert into this table.</p>
        )}
        <ConfirmField connection={connection} value={typed} onChange={setTyped} />
        <div className="flex items-center gap-3">
          <Button onClick={start} disabled={!canRun || run.isPending} variant={connection.environment === "production" ? "destructive" : "default"}>
            {run.isPending ? <Loader2 className="size-4 animate-spin" /> : <Play className="size-4" />}
            Import {preview ? formatNumber(preview.total_rows) : ""} rows
          </Button>
          <span className="text-[11px] text-muted-foreground">
            Runs in one transaction on {connection.name} ({connection.environment}).
          </span>
        </div>
      </Step>
    </div>
  );
}
