"use client";

import { AlertTriangle } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";

export default function WorkspaceError({ error, reset }: { error: Error & { digest?: string }; reset: () => void }) {
  return (
    <EmptyState
      icon={AlertTriangle}
      title="Something went wrong"
      description={error.message || "An unexpected error occurred while rendering this view."}
      action={
        <Button size="sm" variant="outline" onClick={reset}>
          Try again
        </Button>
      }
    />
  );
}
