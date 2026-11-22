"use client";

import { useState } from "react";
import { Copy, Laptop, Loader2, LogOut, ShieldCheck, ShieldOff } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import {
  useChangePassword,
  useConfirmTotp,
  useDisableTotp,
  useEnrollTotp,
  useRegenerateRecoveryCodes,
  useRevokeOtherSessions,
  useRevokeSession,
  useSecurity,
  useSessions,
} from "@/hooks/use-account";
import { ApiError } from "@/lib/api-client";
import { formatRelativeTime } from "@/lib/format";
import { describeUserAgent } from "@/lib/user-agent";
import type { FieldError } from "@/types/api";

function errorText(error: ApiError | null): string | null {
  if (!error) return null;
  if (error.code === "validation_error") {
    return ((error.details as { fields?: FieldError[] } | undefined)?.fields ?? []).map((f) => f.message).join(" ");
  }
  return error.message;
}

function Block({ title, description, children }: { title: string; description?: string; children: React.ReactNode }) {
  return (
    <div className="space-y-3 border-b px-4 py-4 last:border-0">
      <div>
        <h4 className="text-sm font-medium">{title}</h4>
        {description && <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>}
      </div>
      {children}
    </div>
  );
}

function ChangePassword() {
  const change = useChangePassword();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  return (
    <Block title="Password" description="Changing it signs out every other session.">
      <form
        className="grid max-w-md gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          change.mutate(
            { current_password: current, new_password: next },
            {
              onSuccess: (r) => {
                setCurrent("");
                setNext("");
                toast.success(
                  r.other_sessions_revoked > 0
                    ? `Password changed; ${r.other_sessions_revoked} other session(s) signed out`
                    : "Password changed",
                );
              },
            },
          );
        }}
      >
        <Label htmlFor="current-password" className="text-xs">
          Current password
        </Label>
        <Input id="current-password" type="password" autoComplete="current-password" value={current} onChange={(e) => setCurrent(e.target.value)} />
        <Label htmlFor="new-password" className="text-xs">
          New password (at least 10 characters)
        </Label>
        <Input id="new-password" type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} />
        {change.isError && <p className="text-xs text-destructive">{errorText(change.error)}</p>}
        <Button type="submit" size="sm" variant="outline" className="w-fit" disabled={change.isPending || !current || next.length < 10}>
          {change.isPending && <Loader2 className="size-3.5 animate-spin" />} Change password
        </Button>
      </form>
    </Block>
  );
}

function RecoveryCodeList({ codes, onDone }: { codes: string[]; onDone: () => void }) {
  return (
    <div className="space-y-2 rounded-md border border-warning/40 bg-warning/5 p-3">
      <p className="text-xs">
        Save these recovery codes somewhere safe. Each works once if you lose your authenticator. They are shown only now.
      </p>
      <ul className="grid grid-cols-2 gap-1 font-mono text-sm sm:grid-cols-5">
        {codes.map((c) => (
          <li key={c}>{c}</li>
        ))}
      </ul>
      <div className="flex gap-2">
        <Button
          size="xs"
          variant="outline"
          onClick={() =>
            void navigator.clipboard.writeText(codes.join("\n")).then(
              () => toast.success("Recovery codes copied"),
              () => toast.error("Clipboard access was denied by the browser."),
            )
          }
        >
          <Copy className="size-3" /> Copy
        </Button>
        <Button size="xs" onClick={onDone}>
          I saved them
        </Button>
      </div>
    </div>
  );
}

function TwoFactor() {
  const security = useSecurity();
  const enroll = useEnrollTotp();
  const confirm = useConfirmTotp();
  const disable = useDisableTotp();
  const regenerate = useRegenerateRecoveryCodes();
  const [code, setCode] = useState("");
  const [password, setPassword] = useState("");
  const [codes, setCodes] = useState<string[] | null>(null);
  const [disabling, setDisabling] = useState(false);

  if (security.isPending) return <Skeleton className="m-4 h-16" />;
  const enabled = security.data?.two_factor_enabled ?? false;

  return (
    <Block
      title="Two-factor authentication"
      description="Sign-ins also ask for a code from an authenticator app (Google Authenticator, 1Password, Authy, ...)."
    >
      {codes ? (
        <RecoveryCodeList codes={codes} onDone={() => setCodes(null)} />
      ) : enabled ? (
        <div className="space-y-3">
          <p className="flex items-center gap-2 text-sm">
            <ShieldCheck className="size-4 text-success" /> Enabled
            <span className="text-xs text-muted-foreground">· {security.data?.recovery_codes_left} recovery codes left</span>
          </p>
          <div className="flex max-w-md flex-wrap items-end gap-2">
            <Input
              value={code}
              onChange={(e) => setCode(e.target.value)}
              placeholder="Current code"
              aria-label="Current authenticator code"
              autoComplete="one-time-code"
              className="h-8 w-36"
            />
            {disabling && (
              <Input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Password"
                aria-label="Password"
                autoComplete="current-password"
                className="h-8 w-44"
              />
            )}
            <Button
              size="sm"
              variant="outline"
              disabled={code.trim().length < 6 || regenerate.isPending}
              onClick={() =>
                regenerate.mutate(code.trim(), {
                  onSuccess: (r) => {
                    setCode("");
                    setCodes(r.recovery_codes);
                  },
                })
              }
            >
              New recovery codes
            </Button>
            <Button
              size="sm"
              variant="destructive"
              disabled={disabling && (code.trim().length < 6 || !password || disable.isPending)}
              onClick={() => {
                if (!disabling) return setDisabling(true);
                disable.mutate(
                  { password, code: code.trim() },
                  {
                    onSuccess: () => {
                      setCode("");
                      setPassword("");
                      setDisabling(false);
                      toast.success("Two-factor authentication turned off");
                    },
                  },
                );
              }}
            >
              <ShieldOff className="size-3.5" /> {disabling ? "Confirm turning off" : "Turn off"}
            </Button>
          </div>
          {(regenerate.isError || disable.isError) && (
            <p className="text-xs text-destructive">{errorText(regenerate.error ?? disable.error)}</p>
          )}
        </div>
      ) : enroll.data ? (
        <div className="flex flex-wrap gap-4">
          {/* An <img> never runs script, so the server's SVG is safe to show;
              a data URI gains nothing from next/image. */}
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src={`data:image/svg+xml;utf8,${encodeURIComponent(enroll.data.qr_svg)}`}
            alt="QR code for your authenticator app"
            width={176}
            height={176}
            className="rounded-md bg-white p-1"
          />
          <form
            className="grid max-w-sm content-start gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              confirm.mutate(code.trim(), {
                onSuccess: (r) => {
                  setCode("");
                  setCodes(r.recovery_codes);
                  enroll.reset();
                  toast.success("Two-factor authentication is on");
                },
              });
            }}
          >
            <p className="text-xs text-muted-foreground">
              Scan the code with your authenticator app, or enter this key manually:
            </p>
            <code className="rounded bg-muted px-2 py-1 font-mono text-xs break-all">{enroll.data.secret}</code>
            <Label htmlFor="totp-code" className="text-xs">
              Code from the app
            </Label>
            <Input id="totp-code" value={code} onChange={(e) => setCode(e.target.value)} autoComplete="one-time-code" inputMode="numeric" className="h-8 w-36" />
            {confirm.isError && <p className="text-xs text-destructive">{errorText(confirm.error)}</p>}
            <Button type="submit" size="sm" className="w-fit" disabled={code.trim().length !== 6 || confirm.isPending}>
              {confirm.isPending && <Loader2 className="size-3.5 animate-spin" />} Turn on
            </Button>
          </form>
        </div>
      ) : (
        <Button size="sm" variant="outline" onClick={() => enroll.mutate(undefined, { onError: (e) => toast.error(e.message) })} disabled={enroll.isPending}>
          <ShieldCheck className="size-3.5" /> Set up two-factor authentication
        </Button>
      )}
    </Block>
  );
}

function Sessions() {
  const sessions = useSessions();
  const revoke = useRevokeSession();
  const revokeOthers = useRevokeOtherSessions();
  const others = sessions.data?.filter((s) => !s.current).length ?? 0;
  return (
    <Block title="Sessions" description="Browsers and devices signed in to your account.">
      {sessions.isPending ? (
        <Skeleton className="h-16" />
      ) : sessions.isError ? (
        <p className="text-xs text-destructive">{sessions.error.message}</p>
      ) : (
        <ul className="divide-y rounded-md border">
          {sessions.data.map((s) => (
            <li key={s.id} className="flex items-center gap-3 px-3 py-2 text-xs">
              <Laptop className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium">
                  {describeUserAgent(s.user_agent)}
                  {s.current && <span className="ml-2 rounded bg-success/15 px-1.5 py-0.5 text-[10px] text-success">This session</span>}
                </p>
                <p className="text-muted-foreground">
                  {s.ip ?? "unknown address"} · active {formatRelativeTime(s.last_seen_at)} · signed in {formatRelativeTime(s.created_at)}
                </p>
              </div>
              {!s.current && (
                <Button
                  size="xs"
                  variant="ghost"
                  onClick={() => revoke.mutate(s.id, { onSuccess: () => toast.success("Session signed out") })}
                  disabled={revoke.isPending}
                >
                  <LogOut className="size-3" /> Sign out
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      {others > 0 && (
        <Button
          size="sm"
          variant="outline"
          onClick={() => revokeOthers.mutate(undefined, { onSuccess: (r) => toast.success(`${r.revoked} session(s) signed out`) })}
          disabled={revokeOthers.isPending}
        >
          Sign out all other sessions
        </Button>
      )}
    </Block>
  );
}

/** Password, two-factor authentication and sessions. */
export function AccountSecurity() {
  return (
    <section id="security" className="scroll-mt-6 rounded-lg border bg-card">
      <div className="border-b px-4 py-3">
        <h3 className="text-sm font-medium">Account security</h3>
      </div>
      <ChangePassword />
      <TwoFactor />
      <Sessions />
    </section>
  );
}
