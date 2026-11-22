import type { Metadata } from "next";

import { ForgotPassword } from "@/components/auth/password-reset";

export const metadata: Metadata = { title: "Reset password" };

export default function ForgotPasswordPage() {
  return <ForgotPassword />;
}
