"use client";

import Link from "next/link";
import { ArrowRight, Bookmark, Database, History, SquareTerminal } from "lucide-react";

import { PageContainer } from "@/components/common/page-container";
import { ConnectionList } from "@/components/connections/connection-list";

const quickLinks = [
  { href: "/editor", icon: SquareTerminal, title: "Open SQL editor", description: "Write and run queries with autocomplete." },
  { href: "/explorer", icon: Database, title: "Explore schema", description: "Browse schemas, tables, keys and relationships." },
  { href: "/saved", icon: Bookmark, title: "Saved queries", description: "Reuse and organise the queries you rely on." },
  { href: "/history", icon: History, title: "Query history", description: "Review and re-run previous executions." },
];

export default function DashboardPage() {
  return (
    <PageContainer title="Dashboard" description="Your SQL workspace at a glance.">
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {quickLinks.map((l) => (
          <Link
            key={l.href}
            href={l.href}
            className="group rounded-lg border bg-card p-4 transition-colors hover:border-brand/40 hover:bg-accent/30"
          >
            <l.icon className="size-4 text-brand" />
            <p className="mt-3 flex items-center gap-1 text-sm font-medium">
              {l.title}
              <ArrowRight className="size-3.5 opacity-0 transition-all group-hover:translate-x-0.5 group-hover:opacity-100" />
            </p>
            <p className="mt-1 text-xs text-muted-foreground">{l.description}</p>
          </Link>
        ))}
      </div>

      <section className="mt-6 rounded-lg border bg-card">
        <div className="border-b px-4 py-3">
          <h3 className="text-sm font-medium">Connections</h3>
        </div>
        <ConnectionList />
      </section>
    </PageContainer>
  );
}
