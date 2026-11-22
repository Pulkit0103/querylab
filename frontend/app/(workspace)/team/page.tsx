"use client";

import { useState } from "react";
import { Copy, LogOut, MailPlus, Trash2, Users } from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { PageContainer } from "@/components/common/page-container";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useMe } from "@/hooks/use-auth";
import {
  useActiveWorkspace,
  useCreateWorkspace,
  useDeleteWorkspace,
  useInvitations,
  useInvite,
  useMembers,
  useRemoveMember,
  useRenameWorkspace,
  useRevokeInvitation,
  useSetRole,
} from "@/hooks/use-workspaces";
import { formatRelativeTime } from "@/lib/format";
import { canManageMembers, ROLE_HELP, ROLE_LABEL, rolesAssignableBy } from "@/lib/workspaces";
import type { Role, Workspace } from "@/types/workspaces";

function RoleSelect({ value, options, onChange, label, disabled }: { value: Role; options: Role[]; onChange: (r: Role) => void; label: string; disabled?: boolean }) {
  return (
    <Select items={options.map((r) => ({ value: r, label: ROLE_LABEL[r] }))} value={value} onValueChange={(v) => typeof v === "string" && onChange(v as Role)}>
      <SelectTrigger size="sm" className="h-7 w-28 text-xs" aria-label={label} disabled={disabled}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((r) => (
          <SelectItem key={r} value={r} className="text-xs">
            {ROLE_LABEL[r]}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function Invite({ workspace }: { workspace: Workspace }) {
  const invite = useInvite(workspace.id);
  const invitations = useInvitations(workspace.id, true);
  const revoke = useRevokeInvitation(workspace.id);
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<Role>("editor");
  const [link, setLink] = useState<{ url: string; emailed: boolean } | null>(null);
  const copy = (text: string) =>
    void navigator.clipboard.writeText(text).then(
      () => toast.success("Invitation link copied"),
      () => toast.error("Clipboard access was denied by the browser."),
    );

  return (
    <section className="space-y-3 rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium">Invite people</h3>
      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          invite.mutate(
            { email: email.trim(), role },
            {
              onSuccess: (r) => {
                setEmail("");
                setLink({ url: r.link, emailed: r.emailed });
              },
              onError: (err) => toast.error(err.message),
            },
          );
        }}
      >
        <Input value={email} onChange={(e) => setEmail(e.target.value)} type="email" placeholder="name@company.com" aria-label="Email" className="h-8 w-64" />
        <RoleSelect value={role} options={rolesAssignableBy(workspace.role).filter((r) => r !== "owner")} onChange={setRole} label="Role for the invitation" />
        <Button type="submit" size="sm" disabled={!email.includes("@") || invite.isPending}>
          <MailPlus className="size-3.5" /> Invite
        </Button>
        <span className="text-[11px] text-muted-foreground">{ROLE_HELP[role]}</span>
      </form>
      {link && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-brand/30 bg-brand/5 p-2 text-xs">
          <span>{link.emailed ? "Invitation emailed. You can also share this link:" : "Share this link with the invitee (it works once, for 7 days):"}</span>
          <code className="max-w-full truncate rounded bg-muted px-1.5 py-0.5 font-mono">{link.url}</code>
          <Button size="xs" variant="outline" onClick={() => copy(link.url)}>
            <Copy className="size-3" /> Copy
          </Button>
        </div>
      )}
      {(invitations.data?.length ?? 0) > 0 && (
        <div>
          <p className="mb-1 text-xs font-medium">Pending invitations</p>
          <ul className="divide-y rounded-md border text-xs">
            {invitations.data!.map((i) => (
              <li key={i.id} className="flex items-center gap-3 px-3 py-1.5">
                <span className="min-w-0 flex-1 truncate">{i.email}</span>
                <span className="text-muted-foreground">{ROLE_LABEL[i.role]}</span>
                <span className="text-muted-foreground">expires {formatRelativeTime(i.expires_at)}</span>
                <Button size="xs" variant="ghost" onClick={() => revoke.mutate(i.id, { onError: (e) => toast.error(e.message) })}>
                  Revoke
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

function Members({ workspace }: { workspace: Workspace }) {
  const { data: me } = useMe();
  const members = useMembers(workspace.id);
  const setRole = useSetRole(workspace.id);
  const remove = useRemoveMember(workspace.id);
  const manage = canManageMembers(workspace.role);

  return (
    <section className="rounded-lg border bg-card">
      <div className="flex items-center justify-between border-b px-4 py-3">
        <h3 className="text-sm font-medium">Members</h3>
        <span className="text-xs text-muted-foreground">{members.data?.length ?? 0} people</span>
      </div>
      {members.isPending ? (
        <Skeleton className="m-4 h-24" />
      ) : (
        <ul className="divide-y">
          {members.data?.map((m) => {
            const self = m.user_id === me?.id;
            const options = rolesAssignableBy(workspace.role, m.role);
            return (
              <li key={m.user_id} className="flex flex-wrap items-center gap-3 px-4 py-2 text-xs">
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium">
                    {m.display_name ?? m.email}
                    {self && <span className="ml-1.5 text-muted-foreground">(you)</span>}
                  </p>
                  {m.display_name && <p className="truncate text-muted-foreground">{m.email}</p>}
                </div>
                {manage && !workspace.personal && options.includes(m.role) ? (
                  <RoleSelect
                    value={m.role}
                    options={options}
                    label={`Role of ${m.email}`}
                    disabled={setRole.isPending}
                    onChange={(role) =>
                      setRole.mutate(
                        { userId: m.user_id, role },
                        { onSuccess: () => toast.success(`${m.email} is now ${ROLE_LABEL[role].toLowerCase()}`), onError: (e) => toast.error(e.message) },
                      )
                    }
                  />
                ) : (
                  <span className="w-28 text-muted-foreground">{ROLE_LABEL[m.role]}</span>
                )}
                {!workspace.personal && manage && !self && (m.role !== "owner" || workspace.role === "owner") && (
                  <Button
                    size="icon-xs"
                    variant="ghost"
                    aria-label={`Remove ${m.email}`}
                    onClick={() => remove.mutate(m.user_id, { onSuccess: () => toast.success(`${m.email} removed`), onError: (e) => toast.error(e.message) })}
                  >
                    <Trash2 className="size-3" />
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

function Settings({ workspace }: { workspace: Workspace }) {
  const { data: me } = useMe();
  const rename = useRenameWorkspace(workspace.id);
  const del = useDeleteWorkspace();
  const leave = useRemoveMember(workspace.id);
  const [name, setName] = useState(workspace.name);
  const [confirm, setConfirm] = useState("");
  const admin = workspace.role === "owner" || workspace.role === "admin";

  return (
    <section className="space-y-4 rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium">Workspace</h3>
      {admin && (
        <form
          className="flex flex-wrap items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            rename.mutate(name.trim(), { onSuccess: () => toast.success("Workspace renamed"), onError: (err) => toast.error(err.message) });
          }}
        >
          <Input value={name} onChange={(e) => setName(e.target.value)} aria-label="Workspace name" className="h-8 w-64" maxLength={100} />
          <Button type="submit" size="sm" variant="outline" disabled={!name.trim() || name.trim() === workspace.name}>
            Rename
          </Button>
        </form>
      )}
      <div className="flex flex-wrap gap-6">
        <div className="space-y-1">
          <p className="text-xs font-medium">Leave</p>
          <Button
            size="sm"
            variant="outline"
            disabled={!me}
            onClick={() => me && leave.mutate(me.id, { onSuccess: () => toast.success(`You left ${workspace.name}`), onError: (e) => toast.error(e.message) })}
          >
            <LogOut className="size-3.5" /> Leave workspace
          </Button>
        </div>
        {workspace.role === "owner" && (
          <div className="space-y-1">
            <p className="text-xs font-medium text-destructive">Delete the workspace, its connections, queries and snippets</p>
            <div className="flex gap-2">
              <Input value={confirm} onChange={(e) => setConfirm(e.target.value)} placeholder={workspace.name} aria-label="Type the workspace name to delete it" className="h-8 w-48" />
              <Button
                size="sm"
                variant="destructive"
                disabled={confirm !== workspace.name || del.isPending}
                onClick={() => del.mutate(workspace.id, { onSuccess: () => toast.success("Workspace deleted"), onError: (e) => toast.error(e.message) })}
              >
                Delete
              </Button>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

export default function TeamPage() {
  const { workspace, isPending } = useActiveWorkspace();
  const create = useCreateWorkspace();

  if (isPending) return <Skeleton className="m-4 h-64" />;
  if (!workspace) return null;
  if (workspace.personal) {
    return (
      <PageContainer title="Team Workspace" description="Share connections, saved queries and snippets with your team.">
        <EmptyState
          icon={Users}
          title="This is your personal workspace"
          description="Only you can see it. Create a team workspace to invite people; members use shared connections without ever seeing their credentials."
          action={
            <Button size="sm" onClick={() => create.mutate("My team", { onError: (e) => toast.error(e.message) })} disabled={create.isPending}>
              <Users className="size-3.5" /> Create a team workspace
            </Button>
          }
          className="h-72 rounded-lg border bg-card"
        />
      </PageContainer>
    );
  }

  return (
    <PageContainer title={workspace.name} description={`You are ${ROLE_LABEL[workspace.role].toLowerCase()} of this workspace. ${ROLE_HELP[workspace.role]}`}>
      <div className="grid max-w-4xl gap-4">
        {canManageMembers(workspace.role) && <Invite workspace={workspace} />}
        <Members workspace={workspace} />
        <Settings key={workspace.id} workspace={workspace} />
      </div>
    </PageContainer>
  );
}
