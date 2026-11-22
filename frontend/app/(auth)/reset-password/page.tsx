import type { Metadata } from "next";

import { ResetPassword } from "@/components/auth/password-reset";

export const metadata: Metadata = { title: "Choose a new password", referrer: "no-referrer" };

export default async function ResetPasswordPage({ searchParams }: PageProps<"/reset-password">) {
  const { token } = await searchParams;
  return <ResetPassword token={typeof token === "string" ? token : null} />;
}
