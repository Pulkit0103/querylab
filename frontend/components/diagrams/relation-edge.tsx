"use client";

import { memo } from "react";
import { getSmoothStepPath, Position, useInternalNode, type Edge, type EdgeProps } from "@xyflow/react";

import { describeEdge, edgeEnds, edgeSides, endGlyph, nodeSize, rowAnchor, selfLoopPath, type Glyph } from "@/lib/diagram";
import { cn } from "@/lib/utils";
import type { GraphEdge, GraphTable } from "@/types/diagram";

export type RelationEdgeData = {
  edge: GraphEdge;
  from: GraphTable;
  to: GraphTable;
  emphasis: "normal" | "dim" | "focus";
};

export type RelationFlowEdge = Edge<RelationEdgeData, "relation">;

// Styles are set on every shape (not inherited from the group) so image
// exports, which copy each element's own styles, draw them the same way.
type Stroke = { stroke: string; strokeWidth: number };

function Glyphs({ glyphs, stroke }: { glyphs: Glyph[]; stroke: Stroke }) {
  return glyphs.map((g, i) =>
    g.type === "line" ? (
      <line key={i} x1={g.x1} y1={g.y1} x2={g.x2} y2={g.y2} style={{ ...stroke, fill: "none" }} />
    ) : (
      <circle key={i} cx={g.cx} cy={g.cy} r={g.r} style={{ ...stroke, fill: "var(--card)" }} />
    ),
  );
}

/**
 * A foreign key drawn from the child's key column to the parent's
 * referenced column, with crow's foot markers at both ends.
 */
function RelationEdgeView({ id, source, target, data }: EdgeProps<RelationFlowEdge>) {
  const sourceNode = useInternalNode(source);
  const targetNode = useInternalNode(target);
  if (!sourceNode || !targetNode || !data) return null;

  const { edge, from, to, emphasis } = data;
  const fromSize = nodeSize(from);
  const toSize = nodeSize(to);
  const s = { ...sourceNode.internals.positionAbsolute, width: fromSize.width, height: fromSize.height };
  const t = { ...targetNode.internals.positionAbsolute, width: toSize.width, height: toSize.height };
  const selfLoop = source === target;
  const sides = edgeSides(s, t, selfLoop);

  const sx = sides.source === "right" ? s.x + s.width : s.x;
  const sy = s.y + rowAnchor(from, edge.from_columns[0]);
  const tx = sides.target === "right" ? t.x + t.width : t.x;
  const ty = t.y + rowAnchor(to, edge.to_columns[0]);
  const sDir = sides.source === "right" ? 1 : -1;
  const tDir = sides.target === "right" ? 1 : -1;

  const path = selfLoop
    ? selfLoopPath(sx, sy, ty)
    : getSmoothStepPath({
        sourceX: sx,
        sourceY: sy,
        sourcePosition: sides.source === "right" ? Position.Right : Position.Left,
        targetX: tx,
        targetY: ty,
        targetPosition: sides.target === "right" ? Position.Right : Position.Left,
        borderRadius: 8,
        offset: 28,
      })[0];

  const ends = edgeEnds(edge);
  const label = describeEdge(edge);
  const stroke: Stroke = {
    stroke: emphasis === "focus" ? "var(--brand)" : "var(--muted-foreground)",
    strokeWidth: emphasis === "focus" ? 2 : 1.25,
  };
  return (
    <g className={cn(emphasis === "dim" && "opacity-15")} data-edge={id}>
      <title>{label}</title>
      <path d={path} style={{ ...stroke, fill: "none" }} />
      {/* Wide transparent stroke so the edge is easy to hover. */}
      <path d={path} style={{ stroke: "transparent", strokeWidth: 12, fill: "none" }} aria-label={label} />
      <Glyphs glyphs={endGlyph(ends.child, sx, sy, sDir)} stroke={stroke} />
      <Glyphs glyphs={endGlyph(ends.parent, tx, ty, tDir)} stroke={stroke} />
    </g>
  );
}

export const RelationEdge = memo(RelationEdgeView);
