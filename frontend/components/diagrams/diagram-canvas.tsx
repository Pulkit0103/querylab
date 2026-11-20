"use client";

import { useMemo, type CSSProperties } from "react";
import { Background, Controls, MiniMap, Panel, ReactFlow, useNodesState, type NodeMouseHandler } from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import { DiagramLegend } from "@/components/diagrams/diagram-legend";
import { RelationEdge, type RelationFlowEdge } from "@/components/diagrams/relation-edge";
import { TableNode, type NodeEmphasis, type TableFlowNode } from "@/components/diagrams/table-node";
import { neighbourhood, nodeSize } from "@/lib/diagram";
import type { Positions, SchemaGraph } from "@/types/diagram";

const nodeTypes = { table: TableNode };
const edgeTypes = { relation: RelationEdge };

// Canvas chrome follows the app's theme tokens.
const canvasStyle = {
  "--xy-background-color": "var(--background)",
  "--xy-background-pattern-dots-color": "color-mix(in oklch, var(--muted-foreground) 35%, transparent)",
  "--xy-minimap-background-color": "var(--panel)",
  "--xy-minimap-node-background-color": "var(--muted)",
  "--xy-minimap-mask-background-color": "color-mix(in oklch, var(--background) 70%, transparent)",
  "--xy-controls-button-background-color": "var(--card)",
  "--xy-controls-button-background-color-hover": "var(--accent)",
  "--xy-controls-button-color": "var(--foreground)",
  "--xy-controls-button-color-hover": "var(--foreground)",
  "--xy-controls-button-border-color": "var(--border)",
} as CSSProperties;

function buildNodes(graph: SchemaGraph, positions: Positions): TableFlowNode[] {
  return graph.tables.map((table) => {
    const { width, height } = nodeSize(table);
    return {
      id: table.name,
      type: "table",
      position: positions[table.name] ?? { x: 0, y: 0 },
      width,
      height,
      data: { table, emphasis: "normal" },
    };
  });
}

/**
 * The diagram canvas. Positions are taken once on mount; remount (with a
 * new `key`) to apply a new layout. Drags are reported through
 * `onPositionsChange`.
 */
export function DiagramCanvas({
  graph,
  initialPositions,
  focus,
  onSelect,
  onPositionsChange,
}: {
  graph: SchemaGraph;
  initialPositions: Positions;
  /** Table to highlight with its neighbours (search target or selection). */
  focus: string | null;
  onSelect: (table: string | null) => void;
  onPositionsChange: (positions: Positions) => void;
}) {
  const [nodes, , onNodesChange] = useNodesState<TableFlowNode>(buildNodes(graph, initialPositions));
  const byName = useMemo(() => new Map(graph.tables.map((t) => [t.name, t])), [graph.tables]);
  const near = useMemo(
    () => (focus && byName.has(focus) ? neighbourhood(graph.edges, focus) : null),
    [focus, byName, graph.edges],
  );

  const shownNodes = useMemo(
    () =>
      nodes.map((n) => {
        const emphasis: NodeEmphasis = !near ? "normal" : n.id === focus ? "focus" : near.has(n.id) ? "neighbour" : "dim";
        return emphasis === n.data.emphasis ? n : { ...n, data: { ...n.data, emphasis } };
      }),
    [nodes, near, focus],
  );

  const edges = useMemo<RelationFlowEdge[]>(
    () =>
      graph.edges.flatMap((edge) => {
        const from = byName.get(edge.from_table);
        const to = byName.get(edge.to_table);
        if (!from || !to) return [];
        const touches = focus !== null && (edge.from_table === focus || edge.to_table === focus);
        return [
          {
            id: `${edge.from_table}:${edge.constraint}`,
            source: edge.from_table,
            target: edge.to_table,
            type: "relation",
            selectable: false,
            focusable: false,
            zIndex: touches ? 1 : 0,
            data: { edge, from, to, emphasis: !near ? "normal" : touches ? "focus" : "dim" },
          },
        ];
      }),
    [graph.edges, byName, focus, near],
  );

  const onNodeClick: NodeMouseHandler<TableFlowNode> = (_, node) => onSelect(node.id);

  return (
    <ReactFlow<TableFlowNode, RelationFlowEdge>
      nodes={shownNodes}
      edges={edges}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      onNodesChange={onNodesChange}
      onNodeClick={onNodeClick}
      onPaneClick={() => onSelect(null)}
      onNodeDragStop={(_, __, dragged) => {
        // Report every node so the saved layout stays complete.
        const out: Positions = {};
        const moved = new Map(dragged.map((n) => [n.id, n.position]));
        for (const n of nodes) out[n.id] = moved.get(n.id) ?? n.position;
        onPositionsChange(out);
      }}
      nodesConnectable={false}
      edgesFocusable={false}
      deleteKeyCode={null}
      minZoom={0.05}
      maxZoom={2}
      fitView
      fitViewOptions={{ padding: 0.12, maxZoom: 1 }}
      proOptions={{ hideAttribution: true }}
      style={canvasStyle}
      aria-label={`ER diagram of schema ${graph.schema}`}
    >
      <Background gap={20} size={1} />
      <Controls showInteractive={false} position="bottom-right" />
      <MiniMap position="top-right" pannable zoomable ariaLabel="Diagram overview" className="hidden md:block" />
      <Panel position="bottom-left">
        <DiagramLegend />
      </Panel>
    </ReactFlow>
  );
}
