"use client";

import { useState, type FormEvent } from "react";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { useCreateMigration, useUpdateMigration } from "@/hooks/use-migrations";
import type { Migration } from "@/types/migrations";

function EditorForm({ projectId, migration, onDone }: { projectId: string; migration: Migration | null; onDone: () => void }) {
  const [version, setVersion] = useState(migration ? String(migration.version) : "");
  const [name, setName] = useState(migration?.name ?? "");
  const [up, setUp] = useState(migration?.up_sql ?? "");
  const [down, setDown] = useState(migration?.down_sql ?? "");
  const create = useCreateMigration();
  const update = useUpdateMigration();
  const pending = create.isPending || update.isPending;
  const error = create.error ?? update.error;
  const versionValid = version.trim() === "" || /^[1-9]\d{0,17}$/.test(version.trim());

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const body = {
      version: version.trim() ? Number(version.trim()) : undefined,
      name: name.trim(),
      up_sql: up,
      down_sql: down.trim() ? down : null,
    };
    const done = { onSuccess: () => (toast.success(migration ? "Migration updated" : "Migration created"), onDone()) };
    if (migration) update.mutate({ id: migration.id, body }, done);
    else create.mutate({ projectId, body }, done);
  };

  return (
    <form onSubmit={submit} className="space-y-3">
      <div className="grid grid-cols-[10rem_1fr] gap-3">
        <div className="space-y-1.5">
          <Label htmlFor="mig-version">Version</Label>
          <Input
            id="mig-version"
            value={version}
            onChange={(e) => setVersion(e.target.value)}
            placeholder="automatic"
            inputMode="numeric"
            className="font-mono"
            aria-invalid={!versionValid}
          />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="mig-name2">Name</Label>
          <Input id="mig-name2" value={name} onChange={(e) => setName(e.target.value)} maxLength={200} autoFocus />
        </div>
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="mig-up">Up script</Label>
        <Textarea
          id="mig-up"
          value={up}
          onChange={(e) => setUp(e.target.value)}
          rows={10}
          spellCheck={false}
          className="font-mono text-xs"
          placeholder="ALTER TABLE ... ;"
        />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="mig-down">Down script (optional, enables rollback)</Label>
        <Textarea
          id="mig-down"
          value={down}
          onChange={(e) => setDown(e.target.value)}
          rows={5}
          spellCheck={false}
          className="font-mono text-xs"
        />
      </div>
      <p className="text-xs text-muted-foreground">
        Scripts run inside one transaction, so leave out BEGIN and COMMIT. Statements that cannot run in a transaction (for
        example CREATE INDEX CONCURRENTLY) are not supported.
      </p>
      {error && <p className="text-sm text-destructive">{error.message}</p>}
      <DialogFooter>
        <Button type="submit" disabled={!name.trim() || !up.trim() || !versionValid || pending}>
          {pending && <Loader2 className="animate-spin" />} {migration ? "Save changes" : "Create migration"}
        </Button>
      </DialogFooter>
    </form>
  );
}

export function MigrationEditor({
  projectId,
  migration,
  open,
  onOpenChange,
}: {
  projectId: string;
  migration: Migration | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{migration ? `Edit migration ${migration.version}` : "New migration"}</DialogTitle>
          <DialogDescription>Scripts can be edited until they have been applied to a database.</DialogDescription>
        </DialogHeader>
        {open && <EditorForm projectId={projectId} migration={migration} onDone={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  );
}
