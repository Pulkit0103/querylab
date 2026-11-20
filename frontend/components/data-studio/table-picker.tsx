"use client";

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useTables } from "@/hooks/use-schema";

const encode = (schema: string, name: string) => JSON.stringify([schema, name]);

/** Chooses the table shown in the Table Editor. */
export function TablePicker({
  connectionId,
  value,
  onChange,
}: {
  connectionId: string;
  value: { schema: string; table: string } | null;
  onChange: (schema: string, table: string) => void;
}) {
  const tables = useTables(connectionId);
  const items = (tables.data ?? []).map((t) => ({ value: encode(t.schema, t.name), label: `${t.schema}.${t.name}`, kind: t.kind }));
  return (
    <Select
      items={items}
      value={value ? encode(value.schema, value.table) : null}
      onValueChange={(v) => {
        if (typeof v !== "string") return;
        const [schema, table] = JSON.parse(v) as [string, string];
        onChange(schema, table);
      }}
    >
      <SelectTrigger size="sm" className="h-7 w-64 text-xs" aria-label="Table">
        <SelectValue placeholder={tables.isPending ? "Loading tables…" : "Choose a table"} />
      </SelectTrigger>
      <SelectContent>
        {items.map((t) => (
          <SelectItem key={t.value} value={t.value} className="text-xs">
            {t.label}
            {t.kind !== "table" && <span className="ml-auto text-[10px] text-muted-foreground">{t.kind.replace("_", " ")}</span>}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
