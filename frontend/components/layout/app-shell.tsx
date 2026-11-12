import type { ReactNode } from "react";

import { ConnectionDialog } from "@/components/connections/connection-dialog";

import { AppSidebar } from "./app-sidebar";
import { ConnectionSelector } from "./connection-selector";
import { TopBar } from "./top-bar";

export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-dvh w-full overflow-hidden">
      <AppSidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar actions={<ConnectionSelector />} />
        <main className="min-h-0 flex-1 overflow-hidden">{children}</main>
      </div>
      <ConnectionDialog />
    </div>
  );
}
