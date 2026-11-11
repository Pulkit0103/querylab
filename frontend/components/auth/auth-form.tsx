"use client";

import { useState, type FormEvent } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Loader2 } from "lucide-react";

import { Logo } from "@/components/layout/logo";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useLogin, useRegister } from "@/hooks/use-auth";
import { ApiError } from "@/lib/api-client";
import { safeNextPath } from "@/lib/auth";
import type { FieldError } from "@/types/api";

function fieldErrors(error: ApiError | null): Record<string, string> {
  if (!error || error.code !== "validation_error") return {};
  const fields = (error.details as { fields?: FieldError[] } | undefined)?.fields ?? [];
  return Object.fromEntries(fields.map((f) => [f.field, f.message]));
}

export function AuthForm({ mode, next }: { mode: "login" | "register"; next: string | null }) {
  const router = useRouter();
  const login = useLogin();
  const register = useRegister();
  const mutation = mode === "login" ? login : register;
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");

  const errors = fieldErrors(mutation.error);
  const generalError = mutation.error && mutation.error.code !== "validation_error" ? mutation.error.message : null;

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    mutation.mutate(
      { email, password, ...(mode === "register" && displayName ? { display_name: displayName } : {}) },
      { onSuccess: () => router.replace(safeNextPath(next)) },
    );
  };

  const isLogin = mode === "login";

  return (
    <div className="w-full max-w-sm">
      <Logo className="mb-8" />
      <h1 className="text-xl font-semibold tracking-tight">{isLogin ? "Sign in to QueryLab" : "Create your account"}</h1>
      <p className="mt-1 text-sm text-muted-foreground">
        {isLogin ? "Welcome back. Pick up where you left off." : "Start exploring your databases in seconds."}
      </p>

      <form onSubmit={onSubmit} className="mt-6 space-y-4" noValidate>
        {!isLogin && (
          <div className="space-y-1.5">
            <Label htmlFor="display_name">Name</Label>
            <Input
              id="display_name"
              autoComplete="name"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              aria-invalid={!!errors.display_name}
            />
            {errors.display_name && <p className="text-xs text-destructive">{errors.display_name}</p>}
          </div>
        )}
        <div className="space-y-1.5">
          <Label htmlFor="email">Email</Label>
          <Input
            id="email"
            type="email"
            autoComplete="email"
            required
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            aria-invalid={!!errors.email}
          />
          {errors.email && <p className="text-xs text-destructive">{errors.email}</p>}
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="password">Password</Label>
          <Input
            id="password"
            type="password"
            autoComplete={isLogin ? "current-password" : "new-password"}
            required
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            aria-invalid={!!errors.password}
          />
          {errors.password ? (
            <p className="text-xs text-destructive">{errors.password}</p>
          ) : (
            !isLogin && <p className="text-xs text-muted-foreground">At least 10 characters.</p>
          )}
        </div>

        {generalError && (
          <p role="alert" className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {generalError}
          </p>
        )}

        <Button
          type="submit"
          className="w-full bg-brand text-brand-foreground hover:bg-brand/85"
          size="lg"
          disabled={mutation.isPending}
        >
          {mutation.isPending && <Loader2 className="size-4 animate-spin" />}
          {isLogin ? "Sign in" : "Create account"}
        </Button>
      </form>

      <p className="mt-6 text-sm text-muted-foreground">
        {isLogin ? "New to QueryLab? " : "Already have an account? "}
        <Link
          href={`${isLogin ? "/register" : "/login"}${next ? `?next=${encodeURIComponent(next)}` : ""}`}
          className="text-brand hover:underline"
        >
          {isLogin ? "Create an account" : "Sign in"}
        </Link>
      </p>
    </div>
  );
}
