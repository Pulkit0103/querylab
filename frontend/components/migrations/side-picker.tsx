"use client";

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useConnections } from "@/hooks/use-connections";
import { useSchemas } from "@/hooks/use-schema";

/** A connection and a schema on it. The schema may be typed (it may not exist yet). */
export function SidePicker({
  label,
  connectionId,
  schema,
  onChange,
  allowNewSchema = false,
}: {
  label: string;
  connectionId: string | null;
  schema: string;
  onChange: (next: { connectionId: string | null; schema: string }) => void;
  allowNewSchema?: boolean;
}) {
  const { data: connections } = useConnections();
  const schemas = useSchemas(connectionId);
  const connectionItems = (connections ?? []).map((c) => ({ value: c.id, label: `${c.name} (${c.environment})` }));
  const schemaItems = (schemas.data ?? []).map((s) => ({ value: s.name, label: s.name }));

  return (
    <fieldset className="min-w-0 space-y-1.5">
      <legend className="mb-1 text-xs font-medium text-muted-foreground">{label}</legend>
      <div className="flex flex-wrap gap-2">
        <Select
          items={connectionItems}
          value={connectionId}
          onValueChange={(v) => typeof v === "string" && onChange({ connectionId: v, schema: "" })}
        >
          <SelectTrigger size="sm" className="h-8 w-56 text-xs" aria-label={`${label} connection`}>
            <SelectValue placeholder="Connection" />
          </SelectTrigger>
          <SelectContent>
            {connectionItems.map((c) => (
              <SelectItem key={c.value} value={c.value} className="text-xs">
                {c.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {allowNewSchema ? (
          <>
            <input
              list={`${label}-schemas`}
              value={schema}
              onChange={(e) => onChange({ connectionId, schema: e.target.value })}
              placeholder="Schema"
              aria-label={`${label} schema`}
              disabled={!connectionId}
              className="h-8 w-44 rounded-md border bg-background px-2 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
            />
            <datalist id={`${label}-schemas`}>
              {schemaItems.map((s) => (
                <option key={s.value} value={s.value} />
              ))}
            </datalist>
          </>
        ) : (
          <Select
            items={schemaItems}
            value={schema || null}
            onValueChange={(v) => typeof v === "string" && onChange({ connectionId, schema: v })}
          >
            <SelectTrigger size="sm" className="h-8 w-44 text-xs" aria-label={`${label} schema`} disabled={!connectionId}>
              <SelectValue placeholder={schemas.isPending && connectionId ? "Loading…" : "Schema"} />
            </SelectTrigger>
            <SelectContent>
              {schemaItems.map((s) => (
                <SelectItem key={s.value} value={s.value} className="text-xs">
                  {s.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>
    </fieldset>
  );
}
