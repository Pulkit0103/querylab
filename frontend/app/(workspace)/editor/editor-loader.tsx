"use client";

import dynamic from "next/dynamic";

import { Skeleton } from "@/components/ui/skeleton";

// The workspace reads persisted tabs from localStorage, so it renders on the
// client only to avoid hydration mismatches.
const EditorWorkspace = dynamic(
  () => import("@/components/editor/editor-workspace").then((m) => m.EditorWorkspace),
  {
    ssr: false,
    loading: () => (
      <div className="flex h-full flex-col gap-2 p-3">
        <Skeleton className="h-8 w-1/3" />
        <Skeleton className="h-1/2 w-full" />
        <Skeleton className="h-1/3 w-full" />
      </div>
    ),
  },
);

export function EditorLoader() {
  return <EditorWorkspace />;
}
