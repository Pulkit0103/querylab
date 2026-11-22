"use client";

import { toast } from "sonner";

import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useGrants, useSetGrant } from "@/hooks/use-workspaces";
import { ACCESS_LABEL, grantOptions, ROLE_LABEL } from "@/lib/workspaces";
import type { Connection } from "@/types/api";
import type { GrantAccess } from "@/types/workspaces";

const DEFAULT = "__role_default__";

/** Who can use a shared connection, and how. Credentials are never shown. */
export function ConnectionAccessDialog({ connection, onClose }: { connection: Connection | null; onClose: () => void }) {
  const grants = useGrants(connection?.id ?? null);
  const setGrant = useSetGrant(connection?.id ?? "");

  return (
    <Dialog open={connection !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Access to {connection?.name}</DialogTitle>
          <DialogDescription>
            Members use this connection without seeing its password. Without an explicit setting, access follows the
            member&apos;s role; viewers never write and admins always manage.
          </DialogDescription>
        </DialogHeader>
        {grants.isPending ? (
          <Skeleton className="h-32" />
        ) : grants.isError ? (
          <p className="text-sm text-destructive">{grants.error.message}</p>
        ) : (
          <ul className="divide-y rounded-md border text-xs">
            {grants.data.map((m) => {
              const options = grantOptions(m.role);
              return (
                <li key={m.user_id} className="flex items-center gap-3 px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-medium">{m.display_name ?? m.email}</p>
                    <p className="text-muted-foreground">{ROLE_LABEL[m.role]}</p>
                  </div>
                  {options.length === 0 ? (
                    <span className="text-muted-foreground">{ACCESS_LABEL[m.effective]} (role)</span>
                  ) : (
                    <Select
                      items={[{ value: DEFAULT, label: "Role default" }, ...options.map((o) => ({ value: o, label: ACCESS_LABEL[o] }))]}
                      value={m.grant ?? DEFAULT}
                      onValueChange={(v) => {
                        if (typeof v !== "string") return;
                        setGrant.mutate(
                          { userId: m.user_id, access: v === DEFAULT ? null : (v as GrantAccess) },
                          { onError: (e) => toast.error(e.message) },
                        );
                      }}
                    >
                      <SelectTrigger size="sm" className="h-7 w-40 text-xs" aria-label={`Access for ${m.email}`}>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value={DEFAULT} className="text-xs">
                          Role default ({ACCESS_LABEL[m.role === "viewer" ? "read" : "write"]})
                        </SelectItem>
                        {options.map((o) => (
                          <SelectItem key={o} value={o} className="text-xs">
                            {ACCESS_LABEL[o]}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}
