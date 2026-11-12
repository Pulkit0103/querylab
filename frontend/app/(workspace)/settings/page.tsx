"use client";

import { useTheme } from "next-themes";
import { Plus } from "lucide-react";

import { PageContainer, Section } from "@/components/common/page-container";
import { ConnectionList } from "@/components/connections/connection-list";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { formatNumber } from "@/lib/format";
import { useWorkspaceStore } from "@/stores/workspace-store";

const ROW_LIMITS = [100, 500, 1000, 5000, 10000];
const THEMES = [
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light" },
];

export default function SettingsPage() {
  const { theme, setTheme } = useTheme();
  const maxRows = useWorkspaceStore((s) => s.maxRows);
  const setMaxRows = useWorkspaceStore((s) => s.setMaxRows);
  const openConnectionDialog = useWorkspaceStore((s) => s.openConnectionDialog);

  return (
    <PageContainer title="Settings" description="Preferences for this browser.">
      <div className="grid max-w-3xl gap-4">
        <section id="connections" className="scroll-mt-6 rounded-lg border bg-card">
          <div className="flex items-center justify-between border-b px-4 py-3">
            <div>
              <h3 className="text-sm font-medium">Database connections</h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                PostgreSQL databases available in the editor and explorer.
              </p>
            </div>
            <Button size="sm" variant="outline" className="gap-1.5" onClick={() => openConnectionDialog()}>
              <Plus className="size-3.5" /> Add
            </Button>
          </div>
          <ConnectionList />
        </section>
        <Section title="Appearance">
          <div className="flex items-center justify-between gap-4">
            <Label htmlFor="theme">Theme</Label>
            <Select items={THEMES} value={theme ?? "dark"} onValueChange={(v) => v && setTheme(String(v))}>
              <SelectTrigger id="theme" className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {THEMES.map((t) => (
                  <SelectItem key={t.value} value={t.value}>
                    {t.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </Section>
        <Section title="Query execution" description="The server enforces its own upper bound regardless of this value.">
          <div className="flex items-center justify-between gap-4">
            <Label htmlFor="max-rows">Maximum rows returned</Label>
            <Select
              items={ROW_LIMITS.map((n) => ({ value: n, label: formatNumber(n) }))}
              value={maxRows}
              onValueChange={(v) => typeof v === "number" && setMaxRows(v)}
            >
              <SelectTrigger id="max-rows" className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ROW_LIMITS.map((n) => (
                  <SelectItem key={n} value={n}>
                    {formatNumber(n)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </Section>
      </div>
    </PageContainer>
  );
}
