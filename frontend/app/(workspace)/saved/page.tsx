"use client";

import { Bookmark } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { PageContainer } from "@/components/common/page-container";

export default function SavedQueriesPage() {
  return (
    <PageContainer title="Saved Queries" description="Queries you have saved for later.">
      <div className="h-80 rounded-lg border bg-card">
        <EmptyState
          icon={Bookmark}
          title="No saved queries"
          description="Press Ctrl + S in the editor to save the current query."
        />
      </div>
    </PageContainer>
  );
}
