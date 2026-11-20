"use client";

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { flattenTree, folderPath, folderTree, type FolderNode } from "@/lib/library";
import type { Collection } from "@/types/library";

const NONE = "__none__";

/** Chooses a folder (or none) from the nested folder tree. */
export function FolderSelect({
  id,
  folders,
  value,
  onChange,
  options,
  noneLabel = "No folder",
}: {
  id?: string;
  folders: Collection[];
  value: string | null;
  onChange: (id: string | null) => void;
  /** Restrict the choices (e.g. valid move targets); defaults to all folders. */
  options?: FolderNode[];
  noneLabel?: string;
}) {
  const nodes = options ?? flattenTree(folderTree(folders));
  const items = [
    { value: NONE, label: noneLabel },
    ...nodes.map((n) => ({ value: n.folder.id, label: folderPath(folders, n.folder.id) })),
  ];
  return (
    <Select items={items} value={value ?? NONE} onValueChange={(v) => onChange(v === NONE || v == null ? null : String(v))}>
      <SelectTrigger id={id} className="w-full">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={NONE}>{noneLabel}</SelectItem>
        {nodes.map((n) => (
          <SelectItem key={n.folder.id} value={n.folder.id}>
            <span style={{ paddingLeft: `${(n.depth - 1) * 12}px` }}>{n.folder.name}</span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
