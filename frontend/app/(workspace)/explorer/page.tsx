"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";

import { NoConnection } from "@/components/explorer/no-connection";
import { ObjectDetails } from "@/components/explorer/object-details";
import { SchemaTree } from "@/components/explorer/schema-tree";
import { TableDetails } from "@/components/explorer/table-details";
import { Skeleton } from "@/components/ui/skeleton";
import { useActiveConnection } from "@/hooks/use-connections";
import type { ExplorerObject } from "@/lib/explorer";
import { previewSql } from "@/lib/sql-ident";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";

export default function ExplorerPage() {
  const router = useRouter();
  const { connection, isPending } = useActiveConnection();
  const selected = useWorkspaceStore((s) => s.selectedTable);
  const selectTable = useWorkspaceStore((s) => s.selectTable);
  const newTab = useEditorStore((s) => s.newTab);
  // Functions, sequences and types are selected per visit; tables persist.
  const [object, setObject] = useState<{ connectionId: string; value: ExplorerObject } | null>(null);

  if (isPending) return <Skeleton className="m-4 h-64" />;
  if (!connection) return <NoConnection />;

  const openSql = (title: string, sql: string) => {
    newTab({ title, sql });
    router.push("/editor");
  };
  const preview = (schema: string, table: string) => openSql(table, previewSql(schema, table));
  const select = (schema: string, name: string) => {
    setObject(null);
    selectTable({ schema, name });
  };
  const selectedObject = object?.connectionId === connection.id ? object.value : null;

  return (
    <div className="flex h-full min-h-0 flex-col md:flex-row">
      <div className="h-72 shrink-0 border-b bg-panel md:h-full md:w-80 md:border-r md:border-b-0">
        <SchemaTree
          connectionId={connection.id}
          databaseName={connection.database}
          selected={selected}
          selectedObject={selectedObject}
          onSelectTable={select}
          onSelectObject={(value) => setObject({ connectionId: connection.id, value })}
          onPreviewTable={preview}
          onOpenSql={openSql}
        />
      </div>
      <div className="min-h-0 min-w-0 flex-1">
        {selectedObject ? (
          <ObjectDetails connectionId={connection.id} object={selectedObject} onOpenSql={openSql} />
        ) : (
          <TableDetails
            connectionId={connection.id}
            table={selected}
            onPreview={preview}
            onSelectTable={select}
            onOpenSql={openSql}
          />
        )}
      </div>
    </div>
  );
}
