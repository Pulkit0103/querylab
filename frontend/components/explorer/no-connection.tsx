"use client";

import { Database, Plus } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import { useWorkspaceStore } from "@/stores/workspace-store";

export function NoConnection({ compact = false }: { compact?: boolean }) {
  const open = useWorkspaceStore((s) => s.openConnectionDialog);
  return (
    <EmptyState
      icon={Database}
      title="No database connected"
      description={compact ? undefined : "Add a PostgreSQL connection to browse its schema and run queries."}
      action={
        <Button size="sm" variant="outline" className="gap-1.5" onClick={() => open()}>
          <Plus className="size-3.5" /> Add connection
        </Button>
      }
    />
  );
}
