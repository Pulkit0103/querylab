"use client";

import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { useSaveWorkspaceSettings, useWorkspaceSettings } from "@/hooks/use-library";

const MIN_DAYS = 1;
const MAX_DAYS = 3650;

function RetentionForm({ initial }: { initial: number }) {
  const save = useSaveWorkspaceSettings();
  const [days, setDays] = useState(String(initial));
  const value = Number(days);
  const valid = Number.isInteger(value) && value >= MIN_DAYS && value <= MAX_DAYS;

  return (
    <form
      className="flex flex-wrap items-center justify-between gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (valid) {
          save.mutate(
            { history_retention_days: value },
            { onSuccess: () => toast.success("History retention updated"), onError: (err) => toast.error(err.message) },
          );
        }
      }}
    >
      <Label htmlFor="retention">Keep history for</Label>
      <div className="flex items-center gap-2">
        <Input
          id="retention"
          type="number"
          inputMode="numeric"
          min={MIN_DAYS}
          max={MAX_DAYS}
          value={days}
          onChange={(e) => setDays(e.target.value)}
          aria-invalid={!valid}
          className="w-24"
        />
        <span className="text-sm text-muted-foreground">days</span>
        <Button type="submit" size="sm" variant="outline" disabled={!valid || value === initial || save.isPending}>
          Save
        </Button>
      </div>
    </form>
  );
}

/** Account setting: how long query history is kept (pinned entries are kept). */
export function RetentionSetting() {
  const settings = useWorkspaceSettings();
  if (settings.isPending) return <Skeleton className="h-9" />;
  if (settings.isError) return <p className="text-sm text-destructive">{settings.error.message}</p>;
  return <RetentionForm key={settings.data.history_retention_days} initial={settings.data.history_retention_days} />;
}
