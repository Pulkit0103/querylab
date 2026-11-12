"use client";

import { useDeferredValue, useState, type FormEvent } from "react";
import { useRouter } from "next/navigation";
import { Bookmark, Folder, FolderPlus, Library, Pencil, Search, SquareArrowOutUpRight, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { PageContainer } from "@/components/common/page-container";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import {
  useCollections,
  useCreateCollection,
  useDeleteCollection,
  useDeleteSavedQuery,
  useRenameCollection,
  useSavedQueries,
  useUpdateSavedQuery,
} from "@/hooks/use-library";
import { formatRelativeTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { Collection, SavedQuery } from "@/types/library";

const NONE = "__none__";

function EditQueryDialog({ query, onClose }: { query: SavedQuery; onClose: () => void }) {
  const collections = useCollections();
  const update = useUpdateSavedQuery();
  const [name, setName] = useState(query.name);
  const [description, setDescription] = useState(query.description ?? "");
  const [collection, setCollection] = useState(query.collection_id ?? NONE);
  const items = [{ value: NONE, label: "No collection" }, ...(collections.data ?? []).map((c) => ({ value: c.id, label: c.name }))];

  const submit = (e: FormEvent) => {
    e.preventDefault();
    update.mutate(
      {
        id: query.id,
        body: { name: name.trim(), description: description.trim() || null, collection_id: collection === NONE ? null : collection },
      },
      { onSuccess: () => (toast.success("Query updated"), onClose()) },
    );
  };

  return (
    <form onSubmit={submit} className="space-y-4">
      <div className="space-y-1.5">
        <Label htmlFor="eq-name">Name</Label>
        <Input id="eq-name" value={name} onChange={(e) => setName(e.target.value)} maxLength={200} />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="eq-desc">Description</Label>
        <Textarea id="eq-desc" value={description} onChange={(e) => setDescription(e.target.value)} rows={3} maxLength={2000} />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="eq-coll">Collection</Label>
        <Select items={items} value={collection} onValueChange={(v) => setCollection(String(v ?? NONE))}>
          <SelectTrigger id="eq-coll" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {items.map((i) => (
              <SelectItem key={i.value} value={i.value}>
                {i.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {update.error && <p className="text-sm text-destructive">{update.error.message}</p>}
      <DialogFooter>
        <Button type="submit" disabled={!name.trim() || update.isPending}>
          Save changes
        </Button>
      </DialogFooter>
    </form>
  );
}

function QueryCard({ query, collection, onEdit }: { query: SavedQuery; collection?: Collection; onEdit: () => void }) {
  const router = useRouter();
  const openSaved = useEditorStore((s) => s.openSavedQuery);
  const setActiveConnection = useWorkspaceStore((s) => s.setActiveConnection);
  const del = useDeleteSavedQuery();

  const open = () => {
    if (query.connection_id) setActiveConnection(query.connection_id);
    openSaved({ id: query.id, name: query.name, sql: query.sql_content });
    router.push("/editor");
  };

  return (
    <li className="group rounded-lg border bg-card p-4 transition-colors hover:border-brand/40">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <button type="button" onClick={open} className="truncate text-left text-sm font-medium hover:text-brand">
            {query.name}
          </button>
          {query.description && <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">{query.description}</p>}
        </div>
        <div className="flex shrink-0 gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
          <Button size="icon-sm" variant="ghost" aria-label={`Open ${query.name}`} onClick={open}>
            <SquareArrowOutUpRight className="size-3.5" />
          </Button>
          <Button size="icon-sm" variant="ghost" aria-label={`Edit ${query.name}`} onClick={onEdit}>
            <Pencil className="size-3.5" />
          </Button>
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label={`Delete ${query.name}`}
            className="hover:text-destructive"
            onClick={() => {
              if (window.confirm(`Delete “${query.name}”?`)) {
                del.mutate(query.id, { onSuccess: () => toast.success("Query deleted") });
              }
            }}
          >
            <Trash2 className="size-3.5" />
          </Button>
        </div>
      </div>
      <pre className="mt-3 line-clamp-4 rounded-md bg-muted/40 p-2.5 font-mono text-[11px] break-all whitespace-pre-wrap text-muted-foreground">
        {query.sql_content}
      </pre>
      <p className="mt-2 flex items-center gap-2 text-[11px] text-muted-foreground">
        {collection && (
          <span className="flex items-center gap-1">
            <Folder className="size-3" /> {collection.name}
          </span>
        )}
        <span>Updated {formatRelativeTime(query.updated_at)}</span>
      </p>
    </li>
  );
}

function CollectionsNav({ selected, onSelect }: { selected: string | null; onSelect: (id: string | null) => void }) {
  const collections = useCollections();
  const create = useCreateCollection();
  const rename = useRenameCollection();
  const del = useDeleteCollection();

  const add = () => {
    const name = window.prompt("Collection name")?.trim();
    if (name) create.mutate(name, { onError: (e) => toast.error(e.message), onSuccess: (c) => onSelect(c.id) });
  };

  return (
    <nav className="space-y-0.5" aria-label="Collections">
      <button
        type="button"
        onClick={() => onSelect(null)}
        className={cn(
          "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-accent/60",
          selected === null && "bg-accent font-medium",
        )}
      >
        <Library className="size-4 text-muted-foreground" /> All queries
      </button>
      {(collections.data ?? []).map((c) => (
        <div
          key={c.id}
          className={cn("group flex items-center rounded-md hover:bg-accent/60", selected === c.id && "bg-accent font-medium")}
        >
          <button type="button" onClick={() => onSelect(c.id)} className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-sm">
            <Folder className="size-4 shrink-0 text-muted-foreground" />
            <span className="truncate">{c.name}</span>
            <span className="ml-auto text-[11px] text-muted-foreground tabular-nums">{c.query_count}</span>
          </button>
          <div className="hidden shrink-0 pr-1 group-hover:flex">
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={`Rename ${c.name}`}
              onClick={() => {
                const name = window.prompt("Rename collection", c.name)?.trim();
                if (name && name !== c.name) rename.mutate({ id: c.id, name }, { onError: (e) => toast.error(e.message) });
              }}
            >
              <Pencil />
            </Button>
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={`Delete ${c.name}`}
              onClick={() => {
                if (window.confirm(`Delete collection “${c.name}”? Its queries are kept.`)) {
                  del.mutate(c.id, { onSuccess: () => selected === c.id && onSelect(null) });
                }
              }}
            >
              <Trash2 />
            </Button>
          </div>
        </div>
      ))}
      <Button variant="ghost" size="sm" className="mt-1 w-full justify-start gap-2 text-muted-foreground" onClick={add}>
        <FolderPlus className="size-4" /> New collection
      </Button>
    </nav>
  );
}

export default function SavedQueriesPage() {
  const [collectionId, setCollectionId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const q = useDeferredValue(search.trim());
  const [editing, setEditing] = useState<SavedQuery | null>(null);
  const collections = useCollections();
  const saved = useSavedQueries({ ...(q ? { q } : {}), ...(collectionId ? { collection_id: collectionId } : {}) });
  const byId = new Map((collections.data ?? []).map((c) => [c.id, c]));

  return (
    <PageContainer title="Saved Queries" description="Organise, search and reopen the queries you rely on.">
      <div className="grid gap-6 md:grid-cols-[14rem_1fr]">
        <CollectionsNav selected={collectionId} onSelect={setCollectionId} />
        <div className="min-w-0">
          <div className="relative mb-3">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search by name, description or SQL…"
              className="pl-8"
              aria-label="Search saved queries"
            />
          </div>
          {saved.isPending ? (
            <div className="grid gap-3 lg:grid-cols-2">
              {Array.from({ length: 4 }).map((_, i) => (
                <Skeleton key={i} className="h-36" />
              ))}
            </div>
          ) : saved.isError ? (
            <p className="text-sm text-destructive">{saved.error.message}</p>
          ) : saved.data.length === 0 ? (
            <div className="rounded-lg border bg-card">
              <EmptyState
                className="py-16"
                icon={Bookmark}
                title={q ? "No matching queries" : "No saved queries"}
                description={q ? "Try a different search." : "Press Ctrl + S in the editor to save the current query."}
              />
            </div>
          ) : (
            <ul className="grid gap-3 lg:grid-cols-2">
              {saved.data.map((query) => (
                <QueryCard
                  key={query.id}
                  query={query}
                  collection={query.collection_id ? byId.get(query.collection_id) : undefined}
                  onEdit={() => setEditing(query)}
                />
              ))}
            </ul>
          )}
        </div>
      </div>
      <Dialog open={editing !== null} onOpenChange={(o) => !o && setEditing(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Edit saved query</DialogTitle>
          </DialogHeader>
          {editing && <EditQueryDialog key={editing.id} query={editing} onClose={() => setEditing(null)} />}
        </DialogContent>
      </Dialog>
    </PageContainer>
  );
}
