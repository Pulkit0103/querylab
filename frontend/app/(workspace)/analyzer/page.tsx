"use client";

import { Suspense } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { GitCompare, LineChart } from "lucide-react";

import { FingerprintPanel } from "@/components/analyzer/fingerprint-panel";
import { PlansPanel } from "@/components/analyzer/plans-panel";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

const trigger = "h-9 flex-none gap-1.5 px-3 text-xs group-data-horizontal/tabs:after:bottom-0 after:bg-brand";

function AnalyzerView() {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const tab = params.get("tab") === "plans" ? "plans" : "queries";

  return (
    <Tabs value={tab} onValueChange={(v) => router.replace(`${pathname}?tab=${v}`)} className="flex h-full min-h-0 flex-col gap-0">
      <div className="flex h-9 shrink-0 items-center border-b bg-panel">
        <TabsList variant="line" className="h-9 gap-0 rounded-none bg-transparent p-0 px-1">
          <TabsTrigger value="queries" className={trigger}>
            <LineChart className="size-3.5" /> Query performance
          </TabsTrigger>
          <TabsTrigger value="plans" className={trigger}>
            <GitCompare className="size-3.5" /> Plan comparison
          </TabsTrigger>
        </TabsList>
      </div>
      <TabsContent value="queries" className="min-h-0 flex-1">
        {tab === "queries" && <FingerprintPanel />}
      </TabsContent>
      <TabsContent value="plans" className="min-h-0 flex-1">
        {tab === "plans" && <PlansPanel />}
      </TabsContent>
    </Tabs>
  );
}

export default function AnalyzerPage() {
  return (
    <Suspense fallback={<Skeleton className="m-4 h-64" />}>
      <AnalyzerView />
    </Suspense>
  );
}
