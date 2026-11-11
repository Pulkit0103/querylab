import { AppShell } from "@/components/layout/app-shell";

export default function WorkspaceLayout({ children }: LayoutProps<"/">) {
  return <AppShell>{children}</AppShell>;
}
