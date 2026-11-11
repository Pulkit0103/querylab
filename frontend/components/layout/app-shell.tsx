import type { ReactNode } from "react";

import { AppSidebar } from "./app-sidebar";
import { TopBar } from "./top-bar";
import { ConnectionSelector } from "./connection-selector";

export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-dvh w-full overflow-hidden">
      <AppSidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar actions={<ConnectionSelector />} />
        <main className="min-h-0 flex-1 overflow-hidden">{children}</main>
      </div>
    </div>
  );
}
