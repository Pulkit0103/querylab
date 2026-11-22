"use client";

import { useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useCreateMigration, useCreateProject, useProjects } from "@/hooks/use-migrations";

const NEW = "__new__";

function SaveForm({
  defaultName,
  upSql,
  downSql,
  downMissing,
  onSaved,
}: {
  defaultName: string;
  upSql: string;
  downSql: string;
  downMissing: string[];
  onSaved: () => void;
}) {
  const router = useRouter();
  const projects = useProjects();
  const createProject = useCreateProject();
  const createMigration = useCreateMigration();
  const [project, setProject] = useState<string>(NEW);
  const [projectName, setProjectName] = useState("");
  const [name, setName] = useState(defaultName);
  const [withDown, setWithDown] = useState(downMissing.length === 0);
  const pending = createProject.isPending || createMigration.isPending;
  const error = createProject.error ?? createMigration.error;
  const projectItems = [
    ...(projects.data ?? []).map((p) => ({ value: p.id, label: p.name })),
    { value: NEW, label: "New project…" },
  ];

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      const projectId = project === NEW ? (await createProject.mutateAsync(projectName.trim())).id : project;
      const saved = await createMigration.mutateAsync({
        projectId,
        body: { name: name.trim(), up_sql: upSql, down_sql: withDown ? downSql : null },
      });
      toast.success(`Saved migration ${saved.version}`);
      onSaved();
      router.push(`/migrations?project=${projectId}`);
    } catch {
      // Shown below.
    }
  };

  return (
    <form onSubmit={submit} className="space-y-4">
      <div className="space-y-1.5">
        <Label>Project</Label>
        <Select items={projectItems} value={project} onValueChange={(v) => typeof v === "string" && setProject(v)}>
          <SelectTrigger className="w-full" aria-label="Project">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {projectItems.map((p) => (
              <SelectItem key={p.value} value={p.value}>
                {p.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {project === NEW && (
          <Input
            value={projectName}
            onChange={(e) => setProjectName(e.target.value)}
            placeholder="Project name, e.g. billing-service"
            aria-label="New project name"
            maxLength={100}
          />
        )}
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="mig-name">Migration name</Label>
        <Input id="mig-name" value={name} onChange={(e) => setName(e.target.value)} maxLength={200} />
        <p className="text-xs text-muted-foreground">The version is the current time (YYYYMMDDHHMMSS); you can change it later.</p>
      </div>
      <label className="flex items-start gap-2 text-sm">
        <Checkbox checked={withDown} onCheckedChange={(v) => setWithDown(v === true)} className="mt-0.5" />
        <span>
          Save the generated down script for rollback
          {downMissing.length > 0 && (
            <span className="block text-xs text-warning">
              {downMissing.length} selected change{downMissing.length === 1 ? " is" : "s are"} not reversible (
              {downMissing.slice(0, 3).join(", ")}
              {downMissing.length > 3 ? ", …" : ""}); the down script leaves them as comments.
            </span>
          )}
        </span>
      </label>
      {error && <p className="text-sm text-destructive">{error.message}</p>}
      <DialogFooter>
        <Button type="submit" disabled={!name.trim() || (project === NEW && !projectName.trim()) || pending}>
          {pending && <Loader2 className="animate-spin" />} Save migration
        </Button>
      </DialogFooter>
    </form>
  );
}

export function SaveMigrationDialog({
  open,
  onOpenChange,
  ...props
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaultName: string;
  upSql: string;
  downSql: string;
  downMissing: string[];
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Save as migration</DialogTitle>
          <DialogDescription>
            Migrations are versioned scripts you can preview and apply to any connection from the Migrations page.
          </DialogDescription>
        </DialogHeader>
        {open && <SaveForm {...props} onSaved={() => onOpenChange(false)} />}
      </DialogContent>
    </Dialog>
  );
}
