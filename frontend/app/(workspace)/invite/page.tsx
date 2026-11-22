"use client";

import { Suspense } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Loader2, MailOpen } from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useMe } from "@/hooks/use-auth";
import { useAcceptInvitation, useInvitationPreview } from "@/hooks/use-workspaces";
import { ROLE_LABEL } from "@/lib/workspaces";

function InviteView() {
  const token = useSearchParams().get("token");
  const router = useRouter();
  const { data: me } = useMe();
  const preview = useInvitationPreview(token);
  const accept = useAcceptInvitation();

  if (!token) return <EmptyState icon={MailOpen} title="No invitation" description="This link is missing its invitation token." />;
  if (preview.isPending) return <Skeleton className="m-6 h-40" />;
  if (preview.isError) {
    return (
      <EmptyState
        icon={MailOpen}
        title="This invitation is no longer valid"
        description="It may have expired, been revoked or already been used. Ask for a new invitation."
      />
    );
  }
  const p = preview.data;
  const wrongAccount = me && me.email.toLowerCase() !== p.email.toLowerCase();

  return (
    <div className="flex h-full items-center justify-center p-6">
      <div className="w-full max-w-md space-y-4 rounded-lg border bg-card p-6 text-sm">
        <MailOpen className="size-6 text-brand" />
        <h1 className="text-lg font-semibold">Join {p.workspace_name}</h1>
        <p className="text-muted-foreground">
          {p.invited_by_email ?? "A workspace admin"} invited <strong className="text-foreground">{p.email}</strong> to join as{" "}
          <strong className="text-foreground">{ROLE_LABEL[p.role].toLowerCase()}</strong>.
        </p>
        {wrongAccount ? (
          <p className="text-destructive" role="alert">
            You are signed in as {me.email}. Sign out and sign in (or create an account) with {p.email} to accept.
          </p>
        ) : (
          <Button
            onClick={() =>
              accept.mutate(token, {
                onSuccess: (ws) => {
                  toast.success(`You joined ${ws.name}`);
                  router.replace("/team");
                },
                onError: (e) => toast.error(e.message),
              })
            }
            disabled={accept.isPending}
          >
            {accept.isPending && <Loader2 className="size-4 animate-spin" />} Join workspace
          </Button>
        )}
      </div>
    </div>
  );
}

export default function InvitePage() {
  return (
    <Suspense fallback={<Skeleton className="m-6 h-40" />}>
      <InviteView />
    </Suspense>
  );
}
