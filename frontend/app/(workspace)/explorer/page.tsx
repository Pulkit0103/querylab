"use client";

import { useRouter } from "next/navigation";

import { NoConnection } from "@/components/explorer/no-connection";
import { SchemaTree } from "@/components/explorer/schema-tree";
import { TableDetails } from "@/components/explorer/table-details";
import { Skeleton } from "@/components/ui/skeleton";
import { useActiveConnection } from "@/hooks/use-connections";
import { previewSql } from "@/lib/sql-ident";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";

export default function ExplorerPage() {
  const router = useRouter();
  const { connection, isPending } = useActiveConnection();
  const selected = useWorkspaceStore((s) => s.selectedTable);
  const selectTable = useWorkspaceStore((s) => s.selectTable);
  const newTab = useEditorStore((s) => s.newTab);

  if (isPending) return <Skeleton className="m-4 h-64" />;
  if (!connection) return <NoConnection />;

  const preview = (schema: string, table: string) => {
    newTab({ title: table, sql: previewSql(schema, table) });
    router.push("/editor");
  };
  const select = (schema: string, name: string) => selectTable({ schema, name });

  return (
    <div className="flex h-full min-h-0 flex-col md:flex-row">
      <div className="h-72 shrink-0 border-b bg-panel md:h-full md:w-72 md:border-r md:border-b-0">
        <SchemaTree
          connectionId={connection.id}
          databaseName={connection.database}
          selected={selected}
          onSelectTable={select}
          onPreviewTable={preview}
        />
      </div>
      <div className="min-h-0 min-w-0 flex-1">
        <TableDetails connectionId={connection.id} table={selected} onPreview={preview} onSelectTable={select} />
      </div>
    </div>
  );
}
