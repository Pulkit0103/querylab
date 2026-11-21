"use client";

import { Suspense, useState } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Activity, HardDrive, ListOrdered, Lock } from "lucide-react";

import { NoConnection } from "@/components/explorer/no-connection";
import { ActivityPanel } from "@/components/monitoring/activity-panel";
import { LocksPanel } from "@/components/monitoring/locks-panel";
import { SizesPanel } from "@/components/monitoring/sizes-panel";
import { TopQueriesPanel } from "@/components/monitoring/top-queries-panel";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useActiveConnection } from "@/hooks/use-connections";

const TABS = ["activity", "locks", "sizes", "top-queries"] as const;
type Tab = (typeof TABS)[number];

const trigger = "h-9 flex-none gap-1.5 px-3 text-xs group-data-horizontal/tabs:after:bottom-0 after:bg-brand";

function MonitoringView() {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const { connection, isPending } = useActiveConnection();
  // Only the open tab mounts (and therefore refreshes).
  const requested = params.get("tab");
  const tab: Tab = TABS.includes(requested as Tab) ? (requested as Tab) : "activity";
  const [refreshMs, setRefreshMs] = useState(5000);

  if (isPending) return <Skeleton className="m-4 h-64" />;
  if (!connection) return <NoConnection />;

  const panelProps = { interval: refreshMs, onIntervalChange: setRefreshMs };
  return (
    <Tabs
      value={tab}
      onValueChange={(v) => router.replace(`${pathname}?tab=${v}`)}
      className="flex h-full min-h-0 flex-col gap-0"
    >
      <div className="flex h-9 shrink-0 items-center border-b bg-panel">
        <TabsList variant="line" className="h-9 gap-0 rounded-none bg-transparent p-0 px-1">
          <TabsTrigger value="activity" className={trigger}>
            <Activity className="size-3.5" /> Activity
          </TabsTrigger>
          <TabsTrigger value="locks" className={trigger}>
            <Lock className="size-3.5" /> Locks
          </TabsTrigger>
          <TabsTrigger value="sizes" className={trigger}>
            <HardDrive className="size-3.5" /> Sizes
          </TabsTrigger>
          <TabsTrigger value="top-queries" className={trigger}>
            <ListOrdered className="size-3.5" /> Top queries
          </TabsTrigger>
        </TabsList>
        <span className="ml-auto truncate px-3 text-xs text-muted-foreground">
          {connection.name} · {connection.database}
        </span>
      </div>
      <TabsContent value="activity" className="min-h-0 flex-1">
        {tab === "activity" && <ActivityPanel key={connection.id} connection={connection} {...panelProps} />}
      </TabsContent>
      <TabsContent value="locks" className="min-h-0 flex-1">
        {tab === "locks" && <LocksPanel key={connection.id} connection={connection} {...panelProps} />}
      </TabsContent>
      <TabsContent value="sizes" className="min-h-0 flex-1">
        {tab === "sizes" && <SizesPanel key={connection.id} connectionId={connection.id} />}
      </TabsContent>
      <TabsContent value="top-queries" className="min-h-0 flex-1">
        {tab === "top-queries" && <TopQueriesPanel key={connection.id} connectionId={connection.id} {...panelProps} />}
      </TabsContent>
    </Tabs>
  );
}

export default function MonitoringPage() {
  return (
    <Suspense fallback={<Skeleton className="m-4 h-64" />}>
      <MonitoringView />
    </Suspense>
  );
}
