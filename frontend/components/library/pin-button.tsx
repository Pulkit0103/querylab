"use client";

import { Pin, PinOff } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { usePinHistory, usePinSavedQuery } from "@/hooks/use-library";
import { cn } from "@/lib/utils";

/** Pins or unpins a saved query or history entry. */
export function PinButton({
  target,
  id,
  pinned,
  label,
  className,
}: {
  target: "saved" | "history";
  id: string;
  pinned: boolean;
  /** What is being pinned, for the accessible name. */
  label: string;
  className?: string;
}) {
  const pinSaved = usePinSavedQuery();
  const pinHistory = usePinHistory();
  const mutation = target === "saved" ? pinSaved : pinHistory;

  return (
    <Button
      size="icon-sm"
      variant="ghost"
      aria-pressed={pinned}
      aria-label={pinned ? `Unpin ${label}` : `Pin ${label}`}
      disabled={mutation.isPending}
      className={cn(pinned && "text-warning hover:text-warning", className)}
      onClick={() => mutation.mutate({ id, pinned: !pinned }, { onError: (e) => toast.error(e.message) })}
    >
      {pinned ? <PinOff className="size-3.5" /> : <Pin className="size-3.5" />}
    </Button>
  );
}
