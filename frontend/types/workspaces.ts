/** Team workspaces (mirrors `models::workspaces`). */
import type { ConnectionAccess } from "@/types/api";

export type Role = "owner" | "admin" | "editor" | "viewer";
export type GrantAccess = ConnectionAccess | "none";

export interface Workspace {
  id: string;
  name: string;
  personal: boolean;
  role: Role;
  member_count: number;
  created_at: string;
}

export interface Member {
  user_id: string;
  email: string;
  display_name: string | null;
  role: Role;
  joined_at: string;
}

export interface Invitation {
  id: string;
  workspace_id: string;
  email: string;
  role: Role;
  invited_by_email: string | null;
  created_at: string;
  expires_at: string;
}

export interface CreatedInvitation {
  invitation: Invitation;
  link: string;
  emailed: boolean;
}

export interface InvitationPreview {
  workspace_name: string;
  email: string;
  role: Role;
  invited_by_email: string | null;
  expires_at: string;
}

export interface MemberAccess {
  user_id: string;
  email: string;
  display_name: string | null;
  role: Role;
  grant: GrantAccess | null;
  effective: GrantAccess;
}

export interface Snippet {
  id: string;
  workspace_id: string;
  created_by: string | null;
  created_by_email: string | null;
  name: string;
  description: string | null;
  body: string;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface SnippetInput {
  name: string;
  description?: string | null;
  body: string;
  tags?: string[];
}
