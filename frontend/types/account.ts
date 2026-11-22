/** Account security and the audit log (mirrors `services::account`, `models::audit`). */

export interface SessionInfo {
  id: string;
  created_at: string;
  last_seen_at: string;
  expires_at: string;
  user_agent: string | null;
  ip: string | null;
  current: boolean;
}

export interface SecurityStatus {
  two_factor_enabled: boolean;
  recovery_codes_left: number;
  password_reset_available: boolean;
}

export interface TotpEnrollment {
  secret: string;
  otpauth_uri: string;
  qr_svg: string;
}

export interface RecoveryCodes {
  recovery_codes: string[];
}

export interface TwoFactorChallenge {
  two_factor_required: true;
  challenge: string;
}

export interface AuditEvent {
  id: number;
  workspace_id: string | null;
  actor_id: string | null;
  actor_email: string | null;
  action: string;
  target_type: string | null;
  target_id: string | null;
  connection_id: string | null;
  connection_name: string | null;
  detail: Record<string, unknown> | null;
  ip: string | null;
  request_id: string | null;
  created_at: string;
}

export interface AuditPage {
  events: AuditEvent[];
  next_before: number | null;
}

export interface AuditFilter {
  action?: string;
  connection_id?: string;
  from?: string;
  to?: string;
}
