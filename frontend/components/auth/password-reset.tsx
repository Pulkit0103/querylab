"use client";

import { useState } from "react";
import Link from "next/link";
import { CheckCircle2, Loader2 } from "lucide-react";

import { Logo } from "@/components/layout/logo";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useConfirmReset, useRequestReset, useResetCheck } from "@/hooks/use-account";
import type { FieldError } from "@/types/api";

const submit = "w-full bg-brand text-brand-foreground hover:bg-brand/85";

/** Asks for an email and sends a reset link if the account exists. */
export function ForgotPassword() {
  const request = useRequestReset();
  const [email, setEmail] = useState("");
  return (
    <div className="w-full max-w-sm">
      <Logo className="mb-8" />
      <h1 className="text-xl font-semibold tracking-tight">Reset your password</h1>
      {request.isSuccess ? (
        <p className="mt-4 flex items-start gap-2 text-sm text-muted-foreground" role="status">
          <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-success" />
          If an account exists for {email}, we sent a link to reset its password. It works for one hour.
        </p>
      ) : (
        <form
          className="mt-6 space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            request.mutate(email.trim());
          }}
        >
          <p className="text-sm text-muted-foreground">Enter your account&apos;s email and we will send you a reset link.</p>
          <div className="space-y-1.5">
            <Label htmlFor="email">Email</Label>
            <Input id="email" type="email" autoComplete="email" required value={email} onChange={(e) => setEmail(e.target.value)} />
          </div>
          {request.isError && (
            <p role="alert" className="text-sm text-destructive">
              {request.error.message}
            </p>
          )}
          <Button type="submit" size="lg" className={submit} disabled={request.isPending || !email.includes("@")}>
            {request.isPending && <Loader2 className="size-4 animate-spin" />} Send reset link
          </Button>
        </form>
      )}
      <p className="mt-6 text-sm text-muted-foreground">
        <Link href="/login" className="text-brand hover:underline">
          Back to sign in
        </Link>
      </p>
    </div>
  );
}

/** Sets a new password from the emailed link. */
export function ResetPassword({ token }: { token: string | null }) {
  const check = useResetCheck(token);
  const confirm = useConfirmReset();
  const [password, setPassword] = useState("");
  const [again, setAgain] = useState("");
  const fieldError =
    confirm.error?.code === "validation_error"
      ? ((confirm.error.details as { fields?: FieldError[] } | undefined)?.fields ?? []).map((f) => f.message).join(" ")
      : confirm.error?.message;
  const mismatch = again.length > 0 && again !== password;
  const invalid = !token || (check.data && !check.data.valid);

  return (
    <div className="w-full max-w-sm">
      <Logo className="mb-8" />
      <h1 className="text-xl font-semibold tracking-tight">Choose a new password</h1>
      {confirm.isSuccess ? (
        <div className="mt-4 space-y-4" role="status">
          <p className="flex items-start gap-2 text-sm text-muted-foreground">
            <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-success" />
            Your password was changed and every session was signed out.
          </p>
          <Button render={<Link href="/login" />} size="lg" className={submit}>
            Sign in
          </Button>
        </div>
      ) : invalid ? (
        <p className="mt-4 text-sm text-destructive" role="alert">
          This reset link is invalid or has expired.{" "}
          <Link href="/forgot-password" className="underline">
            Request a new one
          </Link>
          .
        </p>
      ) : (
        <form
          className="mt-6 space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (token && !mismatch) confirm.mutate({ token, new_password: password });
          }}
        >
          <div className="space-y-1.5">
            <Label htmlFor="new-password">New password</Label>
            <Input id="new-password" type="password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} />
            <p className="text-xs text-muted-foreground">At least 10 characters.</p>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="again">Repeat the new password</Label>
            <Input id="again" type="password" autoComplete="new-password" value={again} onChange={(e) => setAgain(e.target.value)} aria-invalid={mismatch} />
            {mismatch && <p className="text-xs text-destructive">The passwords do not match.</p>}
          </div>
          {fieldError && (
            <p role="alert" className="text-sm text-destructive">
              {fieldError}
            </p>
          )}
          <Button type="submit" size="lg" className={submit} disabled={confirm.isPending || password.length < 10 || mismatch || check.isPending}>
            {confirm.isPending && <Loader2 className="size-4 animate-spin" />} Set password
          </Button>
        </form>
      )}
    </div>
  );
}
