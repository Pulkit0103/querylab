"use client";

import { memo } from "react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import { Eye, Fingerprint, KeyRound, Link2, Table2 } from "lucide-react";

import { formatNumber } from "@/lib/format";
import { HEADER_HEIGHT, NODE_PADDING, ROW_HEIGHT, nodeSize } from "@/lib/diagram";
import { cn } from "@/lib/utils";
import type { GraphColumn, GraphTable } from "@/types/diagram";

export type NodeEmphasis = "normal" | "dim" | "focus" | "neighbour";

export type TableNodeData = {
  table: GraphTable;
  emphasis: NodeEmphasis;
};

export type TableFlowNode = Node<TableNodeData, "table">;

function KeyIcon({ column }: { column: GraphColumn }) {
  if (column.primary_key)
    return <KeyRound className="size-3 shrink-0 text-warning" aria-label="Primary key" role="img" />;
  if (column.foreign_key) return <Link2 className="size-3 shrink-0 text-chart-2" aria-label="Foreign key" role="img" />;
  if (column.unique)
    return <Fingerprint className="size-3 shrink-0 text-muted-foreground" aria-label="Unique" role="img" />;
  return <span className="size-3 shrink-0" />;
}

// Edges attach to computed row positions; these handles only satisfy the
// canvas, which requires one per connected node.
const hiddenHandle = "!size-px !min-h-0 !min-w-0 !border-0 !bg-transparent opacity-0";

function TableNodeView({ data, selected }: NodeProps<TableFlowNode>) {
  const { table, emphasis } = data;
  const { width, height, visible, hidden } = nodeSize(table);
  const isView = table.kind === "view" || table.kind === "materialized_view";
  return (
    <div
      style={{ width, height }}
      className={cn(
        "overflow-hidden rounded-lg border bg-card text-card-foreground",
        emphasis === "dim" && "opacity-30",
        emphasis === "focus" && "border-brand ring-2 ring-brand/40",
        emphasis === "neighbour" && "border-brand/60",
        selected && emphasis !== "focus" && "border-brand",
      )}
      data-table={table.name}
    >
      <Handle type="target" position={Position.Left} isConnectable={false} className={hiddenHandle} />
      <Handle type="source" position={Position.Right} isConnectable={false} className={hiddenHandle} />
      <div
        style={{ height: HEADER_HEIGHT }}
        className={cn("flex items-center gap-2 border-b px-3", isView ? "bg-chart-5/10" : "bg-brand/10")}
        title={table.comment ?? undefined}
      >
        {isView ? (
          <Eye className="size-3.5 shrink-0 text-chart-5" aria-hidden />
        ) : (
          <Table2 className="size-3.5 shrink-0 text-brand" aria-hidden />
        )}
        <span className="min-w-0 flex-1 truncate font-mono text-xs font-semibold">{table.name}</span>
        {table.estimated_rows !== null && (
          <span className="shrink-0 text-[10px] text-muted-foreground tabular-nums" title="Estimated rows">
            ~{formatNumber(table.estimated_rows)}
          </span>
        )}
      </div>
      <ul style={{ paddingBottom: NODE_PADDING }}>
        {table.columns.slice(0, visible).map((c) => (
          <li
            key={c.name}
            style={{ height: ROW_HEIGHT }}
            className="flex items-center gap-1.5 px-3 font-mono text-[11px]"
            title={`${c.name} ${c.data_type}${c.nullable ? "" : " not null"}`}
          >
            <KeyIcon column={c} />
            <span className={cn("min-w-0 truncate", c.primary_key && "font-semibold")}>{c.name}</span>
            <span className="ml-auto shrink-0 truncate pl-2 text-muted-foreground">{c.data_type}</span>
          </li>
        ))}
        {visible === 0 && (
          <li style={{ height: ROW_HEIGHT }} className="flex items-center px-3 text-[11px] text-muted-foreground italic">
            No columns
          </li>
        )}
        {hidden > 0 && (
          <li style={{ height: ROW_HEIGHT }} className="flex items-center px-3 text-[11px] text-muted-foreground">
            +{hidden} more columns
          </li>
        )}
      </ul>
    </div>
  );
}

export const TableNode = memo(TableNodeView);
