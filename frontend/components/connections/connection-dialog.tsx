"use client";

import { useState, type FormEvent } from "react";
import { AlertTriangle, CheckCircle2, ClipboardPaste, Loader2, PlugZap, ShieldCheck, XCircle } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  useCreateConnection,
  useTestDraftConnection,
  useTestSavedConnection,
  useUpdateConnection,
  type ConnectionTestResult,
} from "@/hooks/use-connections";
import { ApiError } from "@/lib/api-client";
import { parseConnectionString } from "@/lib/connection-string";
import { formatDuration } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { Connection, ConnectionInput, FieldError, SslMode } from "@/types/api";

const SSL_OPTIONS: { value: SslMode; label: string }[] = [
  { value: "disable", label: "Disable" },
  { value: "prefer", label: "Prefer" },
  { value: "require", label: "Require" },
  { value: "verify-ca", label: "Verify CA" },
  { value: "verify-full", label: "Verify full" },
];

type FormState = Omit<ConnectionInput, "port" | "password"> & { port: string; password: string };

function initialState(c: Connection | null): FormState {
  return {
    name: c?.name ?? "",
    host: c?.host ?? "",
    port: String(c?.port ?? 5432),
    database: c?.database ?? "",
    username: c?.username ?? "",
    password: "",
    ssl_mode: c?.ssl_mode ?? "require",
    read_only: c?.read_only ?? true,
  };
}

function toInput(f: FormState, includePassword: boolean): ConnectionInput {
  return {
    name: f.name.trim(),
    host: f.host.trim(),
    port: Number(f.port) || 0,
    database: f.database.trim(),
    username: f.username.trim(),
    ...(includePassword ? { password: f.password } : {}),
    ssl_mode: f.ssl_mode,
    read_only: f.read_only,
  };
}

function fieldErrors(error: ApiError | null): Record<string, string> {
  if (!error || error.code !== "validation_error") return {};
  const fields = (error.details as { fields?: FieldError[] } | undefined)?.fields ?? [];
  return Object.fromEntries(fields.map((f) => [f.field, f.message]));
}

type TestState =
  | { status: "idle" }
  | { status: "running" }
  | { status: "ok"; result: ConnectionTestResult }
  | { status: "error"; message: string };

function TestResult({ state }: { state: TestState }) {
  if (state.status === "idle") return null;
  if (state.status === "running") {
    return (
      <div className="flex items-center gap-2 rounded-md border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" /> Connecting…
      </div>
    );
  }
  if (state.status === "error") {
    return (
      <div role="alert" className="flex gap-2 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
        <XCircle className="mt-0.5 size-3.5 shrink-0" />
        <span className="break-words">{state.message}</span>
      </div>
    );
  }
  const r = state.result;
  return (
    <div className="space-y-2 rounded-md border border-success/30 bg-success/10 px-3 py-2 text-xs">
      <p className="flex items-center gap-2 font-medium text-success">
        <CheckCircle2 className="size-3.5" /> Connected in {formatDuration(r.latency_ms)}
      </p>
      <p className="text-muted-foreground">
        PostgreSQL {r.server_version} · {r.user}@{r.database}
        {r.read_only && (
          <span className="ml-2 inline-flex items-center gap-1 text-success">
            <ShieldCheck className="size-3" /> read-only
          </span>
        )}
      </p>
      {r.warnings.map((w) => (
        <p key={w} className="flex gap-1.5 text-warning">
          <AlertTriangle className="mt-0.5 size-3 shrink-0" /> {w}
        </p>
      ))}
    </div>
  );
}

function ConnectionForm({ editing, onDone }: { editing: Connection | null; onDone: () => void }) {
  const [form, setForm] = useState<FormState>(() => initialState(editing));
  const [test, setTest] = useState<TestState>({ status: "idle" });
  const [paste, setPaste] = useState("");
  const create = useCreateConnection();
  const update = useUpdateConnection();
  const testDraft = useTestDraftConnection();
  const testSaved = useTestSavedConnection();
  const save = editing ? update : create;
  const errors = fieldErrors(save.error ?? testDraft.error);

  const set = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((f) => ({ ...f, [key]: value }));
    setTest({ status: "idle" });
  };

  const original = initialState(editing);
  const targetChanged = (["host", "port", "database", "username", "ssl_mode", "read_only"] as const).some(
    (k) => form[k] !== original[k],
  );

  const applyConnectionString = (value: string) => {
    setPaste(value);
    const parsed = parseConnectionString(value);
    if (!parsed) return;
    setForm((f) => ({
      ...f,
      host: parsed.host ?? f.host,
      port: parsed.port ? String(parsed.port) : f.port,
      database: parsed.database ?? f.database,
      username: parsed.username ?? f.username,
      password: parsed.password ?? f.password,
      ssl_mode: parsed.ssl_mode ?? f.ssl_mode,
      name: f.name || parsed.database || parsed.host || "",
    }));
    setPaste("");
    toast.success("Connection details filled from URL");
  };

  const runTest = () => {
    setTest({ status: "running" });
    const onError = (e: ApiError) =>
      setTest({ status: "error", message: e.code === "validation_error" ? "Fix the highlighted fields first." : e.message });
    if (editing && !form.password) {
      if (targetChanged) {
        setTest({ status: "error", message: "Enter the password to test the changed settings." });
        return;
      }
      testSaved.mutate(editing.id, { onSuccess: (result) => setTest({ status: "ok", result }), onError });
      return;
    }
    testDraft.mutate(toInput({ ...form, name: form.name || "draft" }, true), {
      onSuccess: (result) => setTest({ status: "ok", result }),
      onError,
    });
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    const onSuccess = () => {
      toast.success(editing ? "Connection updated" : "Connection saved");
      onDone();
    };
    if (editing) {
      update.mutate({ id: editing.id, body: toInput(form, form.password.length > 0) }, { onSuccess });
    } else {
      create.mutate(toInput(form, true), { onSuccess });
    }
  };

  const generalError =
    save.error && save.error.code !== "validation_error" ? save.error.message : null;

  const field = (id: keyof FormState, label: string, props: React.ComponentProps<typeof Input> = {}) => (
    <div className="space-y-1.5">
      <Label htmlFor={`conn-${id}`} className="text-xs">
        {label}
      </Label>
      <Input
        id={`conn-${id}`}
        value={String(form[id])}
        onChange={(e) => set(id, e.target.value as never)}
        aria-invalid={!!errors[id === "database" ? "database" : id]}
        autoComplete="off"
        spellCheck={false}
        {...props}
      />
      {errors[id] && <p className="text-xs text-destructive">{errors[id]}</p>}
    </div>
  );

  return (
    <form onSubmit={onSubmit} className="space-y-4" noValidate>
      {!editing && (
        <div className="relative">
          <ClipboardPaste className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={paste}
            onChange={(e) => applyConnectionString(e.target.value)}
            placeholder="Paste a postgres:// connection string to fill the form"
            className="pl-8 font-mono text-xs"
            aria-label="Connection string"
            autoComplete="off"
            spellCheck={false}
          />
        </div>
      )}

      {field("name", "Display name", { placeholder: "Production analytics", autoFocus: true })}
      <div className="grid grid-cols-[1fr_6rem] gap-3">
        {field("host", "Host", { placeholder: "db.example.com" })}
        {field("port", "Port", { inputMode: "numeric" })}
      </div>
      {field("database", "Database", { placeholder: "postgres" })}
      <div className="grid grid-cols-2 gap-3">
        {field("username", "Username")}
        {field("password", "Password", {
          type: "password",
          autoComplete: "new-password",
          placeholder: editing ? "Unchanged" : "",
        })}
      </div>

      <div className="grid grid-cols-2 items-end gap-3">
        <div className="space-y-1.5">
          <Label htmlFor="conn-ssl" className="text-xs">
            SSL mode
          </Label>
          <Select items={SSL_OPTIONS} value={form.ssl_mode} onValueChange={(v) => v && set("ssl_mode", v as SslMode)}>
            <SelectTrigger id="conn-ssl" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SSL_OPTIONS.map((o) => (
                <SelectItem key={o.value} value={o.value}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <label className="flex h-8 items-center justify-between gap-2 rounded-lg border px-3 text-xs">
          <span className="flex items-center gap-1.5">
            <ShieldCheck className={cn("size-3.5", form.read_only ? "text-success" : "text-muted-foreground")} />
            Read-only
          </span>
          <Switch checked={form.read_only} onCheckedChange={(v) => set("read_only", v)} aria-label="Read-only mode" />
        </label>
      </div>
      {!form.read_only && (
        <p className="flex gap-1.5 text-xs text-warning">
          <AlertTriangle className="mt-0.5 size-3 shrink-0" />
          Read-write mode lets queries modify data. Database permissions remain the final safeguard.
        </p>
      )}

      <TestResult state={test} />
      {generalError && (
        <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {generalError}
        </p>
      )}

      <DialogFooter className="gap-2 sm:justify-between">
        <Button type="button" variant="outline" onClick={runTest} disabled={test.status === "running"} className="gap-1.5">
          {test.status === "running" ? <Loader2 className="size-3.5 animate-spin" /> : <PlugZap className="size-3.5" />}
          Test connection
        </Button>
        <Button type="submit" disabled={save.isPending} className="gap-1.5 bg-brand text-brand-foreground hover:bg-brand/85">
          {save.isPending && <Loader2 className="size-3.5 animate-spin" />}
          {editing ? "Save changes" : "Save connection"}
        </Button>
      </DialogFooter>
    </form>
  );
}

/** Global add/edit connection dialog, opened via the workspace store. */
export function ConnectionDialog() {
  const { open, editing } = useWorkspaceStore((s) => s.connectionDialog);
  const close = useWorkspaceStore((s) => s.closeConnectionDialog);
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{editing ? "Edit connection" : "Add PostgreSQL connection"}</DialogTitle>
          <DialogDescription>
            Credentials are encrypted at rest and never sent back to the browser. Use a role with only the privileges
            you need.
          </DialogDescription>
        </DialogHeader>
        {/* Remount per target so the form resets cleanly. */}
        {open && <ConnectionForm key={editing?.id ?? "new"} editing={editing} onDone={close} />}
      </DialogContent>
    </Dialog>
  );
}
