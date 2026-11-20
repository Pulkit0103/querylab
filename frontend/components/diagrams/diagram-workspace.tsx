"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getNodesBounds, useReactFlow } from "@xyflow/react";
import { AlertTriangle, Download, LoaderCircle, Maximize, Network, RotateCcw, Search, Table2, X } from "lucide-react";
import { toast } from "sonner";

import { DiagramCanvas } from "@/components/diagrams/diagram-canvas";
import { TableSelect } from "@/components/diagrams/table-select";
import { TableDetails } from "@/components/explorer/table-details";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { diagramKeys, useDiagramLayout, useResetDiagramLayout, useSaveDiagramLayout, useSchemaGraph } from "@/hooks/use-diagram";
import { useTables } from "@/hooks/use-schema";
import { tableEditorHref } from "@/lib/data-studio";
import { findTables, mergePositions, nodeSize, resolvePositions } from "@/lib/diagram";
import { exportDiagram, type ExportFormat } from "@/lib/diagram-export";
import { previewSql } from "@/lib/sql-ident";
import { useEditorStore } from "@/stores/editor-store";
import type { SchemaInfo } from "@/types/schema";
import type { LayoutResponse, Positions } from "@/types/diagram";

const SAVE_DELAY_MS = 700;

const nextFrame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

/**
 * ER diagram for one schema of the active connection. Must be rendered
 * inside a `ReactFlowProvider`.
 */
export function DiagramWorkspace({
  connectionId,
  schemas,
  schema,
  selection,
  includeViews,
  onChange,
}: {
  connectionId: string;
  schemas: SchemaInfo[];
  schema: string;
  /** Selected tables, or null for the whole schema. */
  selection: string[] | null;
  includeViews: boolean;
  onChange: (next: { schema?: string; tables?: string[] | null; views?: boolean }) => void;
}) {
  const router = useRouter();
  const qc = useQueryClient();
  const flow = useReactFlow();
  const newTab = useEditorStore((s) => s.newTab);

  const graph = useSchemaGraph(connectionId, schema, selection, includeViews);
  const layout = useDiagramLayout(connectionId, schema);
  const tables = useTables(connectionId);
  const saveLayout = useSaveDiagramLayout(connectionId, schema);
  const resetLayout = useResetDiagramLayout(connectionId, schema);

  const [resetCount, setResetCount] = useState(0);
  const [selected, setSelected] = useState<{ schema: string; name: string } | null>(null);
  const [search, setSearch] = useState("");
  const [exporting, setExporting] = useState(false);
  const [lightExport, setLightExport] = useState(false);
  const canvasRef = useRef<HTMLDivElement>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pending = useRef<Positions | null>(null);

  // Positions: saved ones where available, automatic layout for the rest.
  // Recomputed when the graph changes or the layout is reset, not on saves.
  const positions = useQuery<Positions>({
    queryKey: ["diagram-positions", connectionId, schema, graph.dataUpdatedAt, resetCount],
    queryFn: () => resolvePositions(graph.data!.tables, graph.data!.edges, layout.data!.positions),
    enabled: graph.isSuccess && layout.isSuccess,
    staleTime: Infinity,
    gcTime: 0,
  });

  const flush = () => {
    clearTimeout(saveTimer.current);
    const current = pending.current;
    pending.current = null;
    if (!current) return;
    const saved = qc.getQueryData<LayoutResponse>(diagramKeys.layout(connectionId, schema))?.positions ?? {};
    saveLayout.mutate(mergePositions(saved, current), {
      onError: (e) => toast.error(`The layout was not saved: ${e.message}`),
    });
  };
  const flushRef = useRef(flush);
  useEffect(() => {
    flushRef.current = flush;
  });
  // Save a pending drag when leaving the page or switching schema.
  useEffect(() => () => flushRef.current(), []);

  const onPositionsChange = (next: Positions) => {
    pending.current = next;
    clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => flushRef.current(), SAVE_DELAY_MS);
  };

  const data = graph.data;
  const matches = useMemo(() => (data ? findTables(data.tables, search) : []), [data, search]);
  const inGraph = (name: string) => data?.tables.some((t) => t.name === name) ?? false;
  const focus = search.trim() ? (matches[0] ?? null) : selected?.schema === schema && inGraph(selected.name) ? selected.name : null;

  const available = useMemo(
    () =>
      (tables.data ?? [])
        .filter((t) => t.schema === schema && (includeViews || (t.kind !== "view" && t.kind !== "materialized_view")))
        .map((t) => t.name),
    [tables.data, schema, includeViews],
  );

  const centerOn = (name: string) => {
    const node = flow.getNode(name);
    const table = data?.tables.find((t) => t.name === name);
    if (!node || !table) return;
    const { width, height } = nodeSize(table);
    void flow.setCenter(node.position.x + width / 2, node.position.y + height / 2, {
      zoom: Math.max(flow.getZoom(), 0.9),
      duration: 400,
    });
  };

  const select = (name: string | null) => setSelected(name ? { schema, name } : null);

  const reset = () => {
    clearTimeout(saveTimer.current);
    pending.current = null;
    resetLayout.mutate(undefined, {
      onSuccess: () => {
        setResetCount((n) => n + 1);
        toast.success("Layout reset");
      },
      onError: (e) => toast.error(e.message),
    });
  };

  const runExport = async (format: ExportFormat) => {
    const wrapper = canvasRef.current;
    const viewport = wrapper?.querySelector<HTMLElement>(".react-flow__viewport");
    const nodes = flow.getNodes();
    if (!wrapper || !viewport || nodes.length === 0) return;
    setExporting(true);
    if (lightExport) wrapper.classList.add("theme-light");
    try {
      await nextFrame();
      await exportDiagram({
        viewport,
        bounds: getNodesBounds(nodes),
        format,
        schema,
        background: getComputedStyle(wrapper).backgroundColor,
      });
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "Export failed");
    } finally {
      wrapper.classList.remove("theme-light");
      setExporting(false);
    }
  };

  const openSql = (title: string, sql: string) => {
    newTab({ title, sql });
    router.push("/editor");
  };

  const schemaItems = schemas.map((s) => ({ value: s.name, label: s.name }));

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Toolbar */}
      <div className="flex shrink-0 flex-wrap items-center gap-2 border-b bg-panel px-3 py-2">
        <Network className="size-4 text-brand" aria-hidden />
        <Select
          items={schemaItems}
          value={schema}
          onValueChange={(v) => typeof v === "string" && onChange({ schema: v, tables: null })}
        >
          <SelectTrigger size="sm" className="h-7 w-40 text-xs" aria-label="Schema">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {schemaItems.map((s) => (
              <SelectItem key={s.value} value={s.value} className="text-xs">
                {s.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <TableSelect
          available={available}
          shown={data?.tables.map((t) => t.name) ?? []}
          selection={selection}
          onChange={(t) => onChange({ tables: t })}
        />
        <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Switch checked={includeViews} onCheckedChange={(v) => onChange({ views: v, tables: null })} aria-label="Include views" />
          Views
        </label>
        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && matches[0]) centerOn(matches[0]);
              if (e.key === "Escape") setSearch("");
            }}
            placeholder="Find table…"
            aria-label="Find table"
            className="h-7 w-44 pl-7 text-xs"
          />
        </div>
        {search.trim() && (
          <span className="text-xs text-muted-foreground tabular-nums" aria-live="polite">
            {matches.length === 0 ? "No match" : `${matches.length} match${matches.length === 1 ? "" : "es"} · Enter to go`}
          </span>
        )}
        <div className="ml-auto flex items-center gap-1">
          {saveLayout.isPending && <span className="text-xs text-muted-foreground">Saving layout…</span>}
          <Button variant="ghost" size="sm" className="h-7 gap-1.5 text-xs" onClick={() => void flow.fitView({ padding: 0.12, duration: 300 })}>
            <Maximize className="size-3.5" /> Fit
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1.5 text-xs"
            onClick={reset}
            disabled={resetLayout.isPending || !data}
            title="Forget saved positions and lay the diagram out again"
          >
            <RotateCcw className="size-3.5" /> Reset layout
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger
              render={<Button variant="outline" size="sm" className="h-7 gap-1.5 text-xs" disabled={exporting || !data} />}
            >
              {exporting ? <LoaderCircle className="size-3.5 animate-spin" /> : <Download className="size-3.5" />} Export
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-48">
              <DropdownMenuItem onClick={() => void runExport("png")}>PNG image</DropdownMenuItem>
              <DropdownMenuItem onClick={() => void runExport("svg")}>SVG image</DropdownMenuItem>
              <DropdownMenuItem onClick={() => void runExport("pdf")}>PDF (print)</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuCheckboxItem checked={lightExport} onCheckedChange={setLightExport}>
                Light background
              </DropdownMenuCheckboxItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      {data && (data.truncated || data.hidden_edges > 0) && (
        <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b bg-warning/10 px-3 py-1.5 text-xs">
          {data.truncated && (
            <span className="flex items-center gap-1.5">
              <AlertTriangle className="size-3.5 text-warning" aria-hidden />
              Large schema: showing the {data.tables.length} most connected of {data.total_tables} tables. Use Tables to
              choose others.
            </span>
          )}
          {data.hidden_edges > 0 && (
            <span className="text-muted-foreground">
              {data.hidden_edges} relationship{data.hidden_edges === 1 ? "" : "s"} to tables outside this diagram (other
              schemas or not selected) {data.hidden_edges === 1 ? "is" : "are"} not drawn.
            </span>
          )}
        </div>
      )}

      {/* Canvas and details */}
      <div ref={canvasRef} className="relative min-h-0 flex-1 bg-background">
        {graph.isError ? (
          <div className="p-6 text-sm text-destructive">{graph.error.message}</div>
        ) : layout.isError ? (
          <div className="p-6 text-sm text-destructive">{layout.error.message}</div>
        ) : !data || !positions.data ? (
          <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
            <LoaderCircle className="size-4 animate-spin" /> {data ? "Laying out tables…" : "Loading schema…"}
          </div>
        ) : data.tables.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center text-sm text-muted-foreground">
            <Table2 className="size-6" />
            {includeViews ? "No tables or views in this schema." : "No tables in this schema."}
          </div>
        ) : (
          <DiagramCanvas
            key={positions.dataUpdatedAt}
            graph={data}
            initialPositions={positions.data}
            focus={focus}
            onSelect={select}
            onPositionsChange={onPositionsChange}
          />
        )}

        {selected && (
          <aside
            className="absolute inset-y-0 right-0 z-10 flex w-full flex-col border-l bg-background shadow-xl sm:w-[440px]"
            aria-label={`Details of ${selected.name}`}
          >
            <div className="flex shrink-0 items-center justify-end gap-1 border-b px-2 py-1">
              <Button
                variant="ghost"
                size="xs"
                onClick={() => router.push(tableEditorHref(selected.schema, selected.name))}
              >
                Open in Table Editor
              </Button>
              <Button variant="ghost" size="icon-xs" aria-label="Close details" onClick={() => setSelected(null)}>
                <X className="size-3.5" />
              </Button>
            </div>
            <div className="min-h-0 flex-1">
              <TableDetails
                connectionId={connectionId}
                table={selected}
                onPreview={(s, t) => openSql(t, previewSql(s, t))}
                onOpenSql={openSql}
                onSelectTable={(s, t) => {
                  setSelected({ schema: s, name: t });
                  if (s === schema && inGraph(t)) centerOn(t);
                }}
              />
            </div>
          </aside>
        )}
      </div>
    </div>
  );
}
