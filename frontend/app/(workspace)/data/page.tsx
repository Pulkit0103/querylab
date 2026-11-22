"use client";

import { Suspense } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Database, Dices, FileUp, History } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { ImportWizard } from "@/components/data-jobs/import-wizard";
import { JobsList } from "@/components/data-jobs/jobs-list";
import { MockDesigner } from "@/components/data-jobs/mock-designer";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useActiveConnection } from "@/hooks/use-connections";

type View = "import" | "mock" | "jobs";
const VIEWS: View[] = ["import", "mock", "jobs"];

function DataView() {
  const { connection, isPending } = useActiveConnection();
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const raw = params.get("view");
  const view: View = VIEWS.includes(raw as View) ? (raw as View) : "import";

  if (isPending) return <Skeleton className="m-4 h-64" />;
  if (!connection) {
    return <EmptyState icon={Database} title="No connection" description="Add a database connection to import or generate data." />;
  }

  return (
    <Tabs value={view} onValueChange={(v) => router.replace(`${pathname}?view=${v}`)} className="flex h-full min-h-0 flex-col gap-0">
      <div className="flex shrink-0 flex-wrap items-center gap-3 border-b bg-panel px-3 py-2">
        <TabsList className="h-8">
          <TabsTrigger value="import" className="text-xs">
            <FileUp className="size-3.5" /> Import
          </TabsTrigger>
          <TabsTrigger value="mock" className="text-xs">
            <Dices className="size-3.5" /> Mock data
          </TabsTrigger>
          <TabsTrigger value="jobs" className="text-xs">
            <History className="size-3.5" /> Jobs
          </TabsTrigger>
        </TabsList>
        <span className="ml-auto text-xs text-muted-foreground">
          {connection.name} · {connection.environment}
          {connection.read_only && " · read-only"}
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto bg-muted/20 p-4">
        <div className="mx-auto max-w-5xl">
          <TabsContent value="import">
            <ImportWizard key={connection.id} connection={connection} />
          </TabsContent>
          <TabsContent value="mock">
            <MockDesigner key={connection.id} connection={connection} />
          </TabsContent>
          <TabsContent value="jobs" className="space-y-2">
            <p className="text-xs text-muted-foreground">
              Exports start from query results (Export all), the Table Editor and the Database Explorer.
            </p>
            <JobsList />
          </TabsContent>
        </div>
      </div>
    </Tabs>
  );
}

export default function DataPage() {
  return (
    <Suspense fallback={<Skeleton className="m-4 h-64" />}>
      <DataView />
    </Suspense>
  );
}
