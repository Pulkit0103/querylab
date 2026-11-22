/** Roles and access levels, as the API enforces them. */
import type { GrantAccess, Role } from "@/types/workspaces";

export const ROLE_LABEL: Record<Role, string> = {
  owner: "Owner",
  admin: "Admin",
  editor: "Editor",
  viewer: "Viewer",
};

export const ROLE_HELP: Record<Role, string> = {
  owner: "Owners manage everything, including other owners and deleting the workspace.",
  admin: "Admins manage members, invitations and every connection.",
  editor: "Editors run writes, apply changes and edit queries and snippets.",
  viewer: "Viewers browse and run read-only queries.",
};

const ORDER: Role[] = ["viewer", "editor", "admin", "owner"];

export const canManageMembers = (role: Role) => role === "owner" || role === "admin";

/**
 * Roles an actor may assign. Admins cannot create owners or change an
 * owner's role; owners can do both.
 */
export function rolesAssignableBy(actor: Role, current?: Role): Role[] {
  if (actor === "owner") return [...ORDER].reverse();
  if (actor === "admin") return current === "owner" ? [] : ["admin", "editor", "viewer"];
  return [];
}

export const ACCESS_LABEL: Record<GrantAccess, string> = {
  none: "No access",
  read: "Read",
  write: "Read & write",
  admin: "Manage",
};

/** Grants that make sense for a member with `role` (viewers never write). */
export function grantOptions(role: Role): GrantAccess[] {
  if (role === "owner" || role === "admin") return [];
  return role === "viewer" ? ["none", "read"] : ["none", "read", "write", "admin"];
}
