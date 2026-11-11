"use client";

import { AlertTriangle, CheckCircle2, Info, MessageSquare, XCircle } from "lucide-react";

import { EmptyState } from "@/components/common/empty-state";
import { cn } from "@/lib/utils";
import type { Message } from "@/stores/editor-store";

const icons = {
  info: Info,
  success: CheckCircle2,
  warning: AlertTriangle,
  error: XCircle,
};

export function MessagesView({ messages }: { messages: Message[] }) {
  if (messages.length === 0) {
    return (
      <EmptyState
        icon={MessageSquare}
        title="No messages yet"
        description="Execution details, notices and errors will appear here."
      />
    );
  }
  return (
    <div className="h-full overflow-auto p-2 font-mono text-xs scrollbar-thin" role="log" aria-live="polite">
      {[...messages].reverse().map((m) => {
        const Icon = icons[m.level];
        return (
          <div key={m.id} className="flex gap-2 rounded-md px-2 py-1.5 hover:bg-accent/40">
            <span className="shrink-0 text-muted-foreground/60 tabular-nums">
              {new Date(m.at).toLocaleTimeString([], { hour12: false })}
            </span>
            <Icon
              className={cn(
                "mt-0.5 size-3.5 shrink-0",
                m.level === "success" && "text-success",
                m.level === "error" && "text-destructive",
                m.level === "warning" && "text-warning",
                m.level === "info" && "text-muted-foreground",
              )}
            />
            <div className="min-w-0">
              <p className={cn("break-words", m.level === "error" && "text-destructive")}>{m.text}</p>
              {m.detail && (
                <pre className="mt-1 whitespace-pre-wrap break-words text-muted-foreground">{m.detail}</pre>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
