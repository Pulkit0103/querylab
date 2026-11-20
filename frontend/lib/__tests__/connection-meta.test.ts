import { describe, expect, it } from "vitest";

import { groupConnections, looksLikePemBundle, matchesConnection, statusTone } from "@/lib/connection-meta";
import type { ConnectionGroup } from "@/types/api";

import { conn } from "./fixtures";

const group = (id: string, name: string, position: number): ConnectionGroup => ({
  id,
  name,
  position,
  connection_count: 0,
  created_at: "",
  updated_at: "",
});

describe("groupConnections", () => {
  it("orders groups by position, keeps empty groups and appends ungrouped", () => {
    const groups = [group("g2", "Zeta", 1), group("g1", "Alpha", 0), group("g3", "Empty", 2)];
    const sections = groupConnections(
      [
        conn({ id: "a", group_id: "g2" }),
        conn({ id: "b", group_id: "g1" }),
        conn({ id: "c", group_id: null }),
        conn({ id: "d", group_id: "deleted-group" }),
      ],
      groups,
    );
    expect(sections.map((s) => s.group?.name ?? "ungrouped")).toEqual(["Alpha", "Zeta", "Empty", "ungrouped"]);
    expect(sections[2].connections).toEqual([]);
    expect(sections[3].connections.map((c) => c.id)).toEqual(["c", "d"]);
  });

  it("returns a single ungrouped section without groups and omits empty ungrouped", () => {
    expect(groupConnections([conn({ id: "a" })], [])).toHaveLength(1);
    expect(groupConnections([], [])).toEqual([{ group: null, connections: [] }]);
    const only = groupConnections([conn({ id: "a", group_id: "g" })], [group("g", "G", 0)]);
    expect(only.map((s) => s.group?.id)).toEqual(["g"]);
  });
});

describe("connection helpers", () => {
  it("matches on name, host, database and user", () => {
    const c = conn({ name: "Orders replica", host: "pg-eu.example.com", database: "shop", username: "analyst" });
    for (const q of ["replica", "PG-EU", "shop", "analyst", "  "]) expect(matchesConnection(c, q)).toBe(true);
    expect(matchesConnection(c, "billing")).toBe(false);
  });

  it("maps statuses to tones", () => {
    expect(statusTone(null)).toBe("muted");
    expect(statusTone("online")).toBe("success");
    expect(statusTone("auth_failed")).toBe("warning");
    expect(statusTone("tls_error")).toBe("warning");
    expect(statusTone("unreachable")).toBe("destructive");
  });

  it("checks PEM bundles structurally", () => {
    const cert = "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----";
    expect(looksLikePemBundle(cert)).toBe(true);
    expect(looksLikePemBundle(`${cert}\n${cert}\n`)).toBe(true);
    expect(looksLikePemBundle("-----BEGIN CERTIFICATE-----\nMIIB")).toBe(false);
    expect(looksLikePemBundle("-----BEGIN PRIVATE KEY-----\nx\n-----END PRIVATE KEY-----")).toBe(false);
  });
});
