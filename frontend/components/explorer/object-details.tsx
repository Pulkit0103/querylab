"use client";

import type { ReactNode } from "react";
import { Braces, FunctionSquare, ListOrdered, Shapes } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { useFunctionDetail, useSchemaObjects } from "@/hooks/use-schema";
import type { ExplorerObject } from "@/lib/explorer";
import { formatNumber } from "@/lib/format";

import { SqlBlock } from "./table-details";

function Header({ icon: Icon, schema, name, badges }: { icon: typeof Shapes; schema: string; name: string; badges: ReactNode }) {
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-3 border-b px-4 py-3">
      <Icon className="size-4 text-brand" />
      <div className="min-w-0">
        <p className="text-xs text-muted-foreground">{schema}</p>
        <h2 className="truncate font-mono text-sm font-medium">{name}</h2>
      </div>
      <div className="flex flex-wrap items-center gap-1.5">{badges}</div>
    </div>
  );
}

function Facts({ rows }: { rows: Array<[string, ReactNode]> }) {
  return (
    <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2 rounded-lg border p-3 text-xs">
      {rows.map(([k, v]) => (
        <div key={k} className="contents">
          <dt className="text-muted-foreground">{k}</dt>
          <dd className="min-w-0 font-mono break-words">{v}</dd>
        </div>
      ))}
    </dl>
  );
}

function Loading() {
  return (
    <div className="space-y-2 p-4">
      <Skeleton className="h-6 w-48" />
      <Skeleton className="h-32 w-full" />
    </div>
  );
}

function FunctionDetails({
  connectionId,
  oid,
  onOpenSql,
}: {
  connectionId: string;
  oid: number;
  onOpenSql?: (title: string, sql: string) => void;
}) {
  const { data, isPending, isError, error } = useFunctionDetail(connectionId, oid);
  if (isPending) return <Loading />;
  if (isError) return <p className="p-4 text-sm text-destructive">{error.message}</p>;
  const isProc = data.kind === "procedure";
  return (
    <div className="flex h-full min-h-0 flex-col">
      <Header
        icon={isProc ? Braces : FunctionSquare}
        schema={data.schema}
        name={`${data.name}(${data.arguments})`}
        badges={
          <>
            <Badge variant="outline" className="text-[10px]">
              {data.kind}
            </Badge>
            <Badge variant="outline" className="text-[10px]">
              {data.language}
            </Badge>
            {!isProc && (
              <Badge variant="outline" className="text-[10px]">
                {data.volatility}
              </Badge>
            )}
            {data.security_definer && (
              <Badge variant="outline" className="border-warning/40 text-[10px] text-warning">
                security definer
              </Badge>
            )}
          </>
        }
      />
      <div className="min-h-0 flex-1 space-y-4 overflow-auto p-4 scrollbar-thin">
        {data.comment && <p className="text-sm text-muted-foreground">{data.comment}</p>}
        <Facts
          rows={[
            ["Arguments", data.arguments || "—"],
            ["Returns", data.result ?? "—"],
          ]}
        />
        {data.definition ? (
          <SqlBlock sql={data.definition} title={`${data.name}.sql`} onOpenSql={onOpenSql} />
        ) : (
          <p className="text-xs text-muted-foreground">Definitions are not available for aggregates.</p>
        )}
      </div>
    </div>
  );
}

export function ObjectDetails({
  connectionId,
  object,
  onOpenSql,
}: {
  connectionId: string;
  object: ExplorerObject;
  onOpenSql?: (title: string, sql: string) => void;
}) {
  const objects = useSchemaObjects(connectionId, object.kind !== "function");
  if (object.kind === "function") return <FunctionDetails connectionId={connectionId} oid={object.oid} onOpenSql={onOpenSql} />;
  if (objects.isPending) return <Loading />;
  if (objects.isError) return <p className="p-4 text-sm text-destructive">{objects.error.message}</p>;

  if (object.kind === "sequence") {
    const s = objects.data.sequences.find((x) => x.schema === object.schema && x.name === object.name);
    if (!s) return <p className="p-4 text-sm text-muted-foreground">Sequence not found. It may have been dropped.</p>;
    return (
      <div className="flex h-full min-h-0 flex-col">
        <Header icon={ListOrdered} schema={s.schema} name={s.name} badges={<Badge variant="outline" className="text-[10px]">sequence</Badge>} />
        <div className="p-4">
          <Facts
            rows={[
              ["Data type", s.data_type],
              ["Start", formatNumber(s.start_value)],
              ["Increment", formatNumber(s.increment_by)],
              ["Last value", s.last_value === null ? "not used yet (or not visible)" : formatNumber(s.last_value)],
              ["Owned by", s.owned_by ?? "—"],
            ]}
          />
        </div>
      </div>
    );
  }

  const t = objects.data.types.find((x) => x.schema === object.schema && x.name === object.name);
  if (!t) return <p className="p-4 text-sm text-muted-foreground">Type not found. It may have been dropped.</p>;
  return (
    <div className="flex h-full min-h-0 flex-col">
      <Header icon={Shapes} schema={t.schema} name={t.name} badges={<Badge variant="outline" className="text-[10px]">{t.kind}</Badge>} />
      <div className="space-y-4 p-4">
        {t.comment && <p className="text-sm text-muted-foreground">{t.comment}</p>}
        {t.kind === "enum" ? (
          <div>
            <p className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">Values</p>
            <div className="flex flex-wrap gap-1.5">
              {t.values.map((v) => (
                <Badge key={v} variant="outline" className="font-mono">
                  {v}
                </Badge>
              ))}
            </div>
          </div>
        ) : (
          <Facts rows={[[t.kind === "domain" ? "Base type" : t.kind === "range" ? "Subtype" : "Attributes", t.detail ?? "—"]]} />
        )}
      </div>
    </div>
  );
}
