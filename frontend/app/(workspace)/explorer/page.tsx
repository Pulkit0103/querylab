"use client";

import { useRouter } from "next/navigation";

import { SchemaTree } from "@/components/explorer/schema-tree";
import { TableDetails } from "@/components/explorer/table-details";
import { previewSql } from "@/lib/sql-ident";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";

export default function ExplorerPage() {
  const router = useRouter();
  const selected = useWorkspaceStore((s) => s.selectedTable);
  const selectTable = useWorkspaceStore((s) => s.selectTable);
  const newTab = useEditorStore((s) => s.newTab);
  const connectionId = "demo";

  const preview = (schema: string, table: string) => {
    newTab({ title: table, sql: previewSql(schema, table) });
    router.push("/editor");
  };

  return (
    <div className="flex h-full min-h-0 flex-col md:flex-row">
      <div className="h-72 shrink-0 border-b bg-panel md:h-full md:w-72 md:border-r md:border-b-0">
        <SchemaTree
          connectionId={connectionId}
          databaseName="Demo database"
          selected={selected}
          onSelectTable={(schema, name) => selectTable({ schema, name })}
          onPreviewTable={preview}
        />
      </div>
      <div className="min-h-0 min-w-0 flex-1">
        <TableDetails connectionId={connectionId} table={selected} onPreview={preview} />
      </div>
    </div>
  );
}
