"use client";

import { Suspense } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { ReactFlowProvider } from "@xyflow/react";

import { DiagramWorkspace } from "@/components/diagrams/diagram-workspace";
import { NoConnection } from "@/components/explorer/no-connection";
import { Skeleton } from "@/components/ui/skeleton";
import { useActiveConnection } from "@/hooks/use-connections";
import { useSchemas } from "@/hooks/use-schema";

function DiagramsView() {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const { connection, isPending } = useActiveConnection();
  const schemas = useSchemas(connection?.id ?? null);

  if (isPending) return <Skeleton className="m-4 h-64" />;
  if (!connection) return <NoConnection />;
  if (schemas.isPending) return <Skeleton className="m-4 h-64" />;
  if (schemas.isError) return <p className="p-6 text-sm text-destructive">{schemas.error.message}</p>;

  const requested = params.get("schema");
  const schema = requested ?? schemas.data[0]?.name ?? null;
  if (!schema) return <p className="p-6 text-sm text-muted-foreground">This database has no user schemas.</p>;
  const tablesParam = params.get("tables");
  const selection = tablesParam ? tablesParam.split(",").filter(Boolean) : null;
  const includeViews = params.get("views") === "1";

  const update = (next: { schema?: string; tables?: string[] | null; views?: boolean }) => {
    const q = new URLSearchParams(params.toString());
    if (next.schema !== undefined) q.set("schema", next.schema);
    if (next.tables !== undefined) {
      if (next.tables === null) q.delete("tables");
      else q.set("tables", next.tables.join(","));
    }
    if (next.views !== undefined) {
      if (next.views) q.set("views", "1");
      else q.delete("views");
    }
    router.replace(`${pathname}?${q.toString()}`);
  };

  return (
    <ReactFlowProvider key={`${connection.id}:${schema}`}>
      <DiagramWorkspace
        connectionId={connection.id}
        schemas={schemas.data}
        schema={schema}
        selection={selection}
        includeViews={includeViews}
        onChange={update}
      />
    </ReactFlowProvider>
  );
}

export default function DiagramsPage() {
  return (
    <Suspense fallback={<Skeleton className="m-4 h-64" />}>
      <DiagramsView />
    </Suspense>
  );
}
