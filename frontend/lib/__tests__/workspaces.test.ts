import { describe, expect, it } from "vitest";

import { canManageMembers, grantOptions, rolesAssignableBy } from "@/lib/workspaces";

describe("workspace roles", () => {
  it("follows the server's rules for assigning roles", () => {
    expect(rolesAssignableBy("owner")).toEqual(["owner", "admin", "editor", "viewer"]);
    expect(rolesAssignableBy("admin", "editor")).toEqual(["admin", "editor", "viewer"]);
    expect(rolesAssignableBy("admin", "owner")).toEqual([]);
    expect(rolesAssignableBy("editor")).toEqual([]);
    expect(canManageMembers("admin")).toBe(true);
    expect(canManageMembers("editor")).toBe(false);
  });

  it("offers grants a role can hold", () => {
    expect(grantOptions("viewer")).toEqual(["none", "read"]);
    expect(grantOptions("editor")).toEqual(["none", "read", "write", "admin"]);
    expect(grantOptions("admin")).toEqual([]);
  });
});
