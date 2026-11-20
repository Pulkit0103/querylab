"use client";

import { Suspense } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Table2 } from "lucide-react";

import { TablePicker } from "@/components/data-studio/table-picker";
import { TableEditor } from "@/components/data-studio/table-editor";
import { NoConnection } from "@/components/explorer/no-connection";
import { Skeleton } from "@/components/ui/skeleton";
import { useActiveConnection } from "@/hooks/use-connections";
import { tableEditorHref } from "@/lib/data-studio";

function TablesWorkspace() {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const { connection, isPending } = useActiveConnection();
  const schema = params.get("schema");
  const table = params.get("table");

  if (isPending) return <Skeleton className="m-4 h-64" />;
  if (!connection) return <NoConnection />;

  const pick = (s: string, t: string) => router.replace(tableEditorHref(s, t, pathname));

  if (!schema || !table) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <Table2 className="size-8 text-muted-foreground" />
        <div className="space-y-1">
          <h1 className="text-sm font-medium">Table Editor</h1>
          <p className="max-w-sm text-xs text-muted-foreground">
            Browse, filter, edit and export table rows. Changes are staged, shown as SQL and applied together in one
            transaction.
          </p>
        </div>
        <TablePicker connectionId={connection.id} value={null} onChange={pick} />
      </div>
    );
  }

  // Remount per table so filters, sorting and paging start fresh.
  return (
    <TableEditor
      key={`${connection.id}:${schema}.${table}`}
      connection={connection}
      schema={schema}
      table={table}
      onPickTable={pick}
    />
  );
}

export default function TablesPage() {
  return (
    <Suspense fallback={<Skeleton className="m-4 h-64" />}>
      <TablesWorkspace />
    </Suspense>
  );
}
