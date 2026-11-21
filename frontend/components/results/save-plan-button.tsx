"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Bookmark, Loader2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useSavePlan } from "@/hooks/use-performance";
import type { PlanSource } from "@/stores/editor-store";
import type { ExplainResult } from "@/types/query";

/** Default name: the statement's first line, shortened. */
export function defaultPlanName(sql: string, analyzed: boolean): string {
  const line = sql.replace(/--[^\n]*/g, "").replace(/\s+/g, " ").trim();
  const head = line.length > 60 ? `${line.slice(0, 57)}…` : line || "Plan";
  return analyzed ? `${head} (analyze)` : head;
}

/** Saves the current plan for comparison in the Analyzer. */
export function SavePlanButton({ plan, source }: { plan: ExplainResult; source: PlanSource }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const save = useSavePlan();
  const router = useRouter();

  const submit = () => {
    save.mutate(
      { name: name.trim(), connection_id: source.connectionId, sql: source.sql, plan },
      {
        onSuccess: () => {
          setOpen(false);
          toast.success("Plan saved", {
            action: { label: "Compare", onClick: () => router.push("/analyzer?tab=plans") },
          });
        },
        onError: (e) => toast.error(e.message),
      },
    );
  };

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        if (next) setName(defaultPlanName(source.sql, plan.analyzed));
        setOpen(next);
      }}
    >
      <PopoverTrigger render={<Button variant="ghost" size="xs" className="gap-1" />}>
        <Bookmark className="size-3.5" /> Save plan
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80">
        <form
          className="space-y-2 text-xs"
          onSubmit={(e) => {
            e.preventDefault();
            if (name.trim()) submit();
          }}
        >
          <label className="block space-y-1">
            <span className="font-medium">Plan name</span>
            <Input value={name} onChange={(e) => setName(e.target.value)} maxLength={200} autoFocus className="h-8 text-xs" />
          </label>
          <p className="text-muted-foreground">
            Saved plans can be compared side by side in the{" "}
            <Link href="/analyzer?tab=plans" className="text-brand hover:underline">
              Analyzer
            </Link>
            .
          </p>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" size="xs" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" size="xs" disabled={!name.trim() || save.isPending}>
              {save.isPending && <Loader2 className="animate-spin" />} Save
            </Button>
          </div>
        </form>
      </PopoverContent>
    </Popover>
  );
}
