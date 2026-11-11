"use client";

import { History } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { PageContainer } from "@/components/common/page-container";

export default function HistoryPage() {
  return (
    <PageContainer title="Query History" description="Every statement you have executed.">
      <div className="h-80 rounded-lg border bg-card">
        <EmptyState icon={History} title="No history yet" description="Queries you run will be recorded here." />
      </div>
    </PageContainer>
  );
}
