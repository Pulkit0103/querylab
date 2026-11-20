"use client";

import { useDeferredValue, useState, type FormEvent, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import {
  Bookmark,
  ChevronRight,
  Folder,
  FolderInput,
  FolderPlus,
  Library,
  MoreHorizontal,
  Pencil,
  Pin,
  Search,
  SquareArrowOutUpRight,
  Tag,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";

import { EmptyState } from "@/components/common/empty-state";
import { PageContainer } from "@/components/common/page-container";
import { FolderSelect } from "@/components/library/folder-select";
import { PinButton } from "@/components/library/pin-button";
import { TagEditor } from "@/components/library/tag-editor";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import {
  useCollections,
  useCreateCollection,
  useDeleteCollection,
  useDeleteSavedQuery,
  useSavedQueries,
  useTags,
  useUpdateCollection,
  useUpdateSavedQuery,
} from "@/hooks/use-library";
import { formatRelativeTime } from "@/lib/format";
import { folderPath, folderTree, MAX_FOLDER_DEPTH, moveTargets, type FolderNode } from "@/lib/library";
import { cn } from "@/lib/utils";
import { useEditorStore } from "@/stores/editor-store";
import { useWorkspaceStore } from "@/stores/workspace-store";
import type { Collection, SavedQuery } from "@/types/library";

function EditQueryForm({ query, onClose }: { query: SavedQuery; onClose: () => void }) {
  const collections = useCollections();
  const allTags = useTags();
  const update = useUpdateSavedQuery();
  const [name, setName] = useState(query.name);
  const [description, setDescription] = useState(query.description ?? "");
  const [collection, setCollection] = useState(query.collection_id);
  const [tags, setTags] = useState(query.tags);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    update.mutate(
      {
        id: query.id,
        body: { name: name.trim(), description: description.trim() || null, collection_id: collection, tags },
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
        <Label htmlFor="eq-coll">Folder</Label>
        <FolderSelect id="eq-coll" folders={collections.data ?? []} value={collection} onChange={setCollection} />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="eq-tags">Tags</Label>
        <TagEditor id="eq-tags" value={tags} onChange={setTags} suggestions={(allTags.data ?? []).map((t) => t.tag)} />
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

function QueryCard({
  query,
  folder,
  onEdit,
  onTag,
}: {
  query: SavedQuery;
  folder?: string;
  onEdit: () => void;
  onTag: (tag: string) => void;
}) {
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
    <li
      className={cn(
        "group rounded-lg border bg-card p-4 transition-colors hover:border-brand/40",
        query.pinned && "border-warning/40",
      )}
    >
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <button type="button" onClick={open} className="flex max-w-full items-center gap-1.5 text-left text-sm font-medium hover:text-brand">
            {query.pinned && <Pin className="size-3.5 shrink-0 text-warning" aria-label="Pinned" />}
            <span className="truncate">{query.name}</span>
          </button>
          {query.description && <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">{query.description}</p>}
        </div>
        <div className="flex shrink-0 gap-0.5">
          <PinButton
            target="saved"
            id={query.id}
            pinned={query.pinned}
            label={query.name}
            className={cn(!query.pinned && "opacity-0 group-hover:opacity-100 focus-visible:opacity-100")}
          />
          <div className="flex gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
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
      </div>
      <pre className="mt-3 line-clamp-4 rounded-md bg-muted/40 p-2.5 font-mono text-[11px] break-all whitespace-pre-wrap text-muted-foreground">
        {query.sql_content}
      </pre>
      <div className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground">
        {query.tags.map((tag) => (
          <button
            key={tag}
            type="button"
            onClick={() => onTag(tag)}
            className="rounded bg-brand/10 px-1.5 py-0.5 text-brand hover:bg-brand/20"
          >
            #{tag}
          </button>
        ))}
        {folder && (
          <span className="flex items-center gap-1">
            <Folder className="size-3" /> {folder}
          </span>
        )}
        <span>Updated {formatRelativeTime(query.updated_at)}</span>
      </div>
    </li>
  );
}

function FolderRow({
  node,
  folders,
  selected,
  open,
  onToggle,
  onSelect,
  onMove,
}: {
  node: FolderNode;
  folders: Collection[];
  selected: string | null;
  open: boolean;
  onToggle: () => void;
  onSelect: (id: string | null) => void;
  onMove: (folder: Collection) => void;
}) {
  const create = useCreateCollection();
  const update = useUpdateCollection();
  const del = useDeleteCollection();
  const { folder, depth, children } = node;

  return (
    <div
      className={cn("group flex items-center rounded-md hover:bg-accent/60", selected === folder.id && "bg-accent font-medium")}
      style={{ paddingLeft: `${(depth - 1) * 14}px` }}
    >
      <button
        type="button"
        onClick={onToggle}
        aria-label={open ? `Collapse ${folder.name}` : `Expand ${folder.name}`}
        className={cn("p-1 text-muted-foreground", children.length === 0 && "invisible")}
      >
        <ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
      </button>
      <button type="button" onClick={() => onSelect(folder.id)} className="flex min-w-0 flex-1 items-center gap-2 py-1.5 pr-1 text-sm">
        <Folder className="size-4 shrink-0 text-muted-foreground" />
        <span className="truncate">{folder.name}</span>
        <span className="ml-auto text-[11px] text-muted-foreground tabular-nums">{folder.query_count}</span>
      </button>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              size="icon-xs"
              variant="ghost"
              aria-label={`Folder actions for ${folder.name}`}
              className="mr-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 data-[popup-open]:opacity-100"
            />
          }
        >
          <MoreHorizontal />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-48">
          {depth < MAX_FOLDER_DEPTH && (
            <DropdownMenuItem
              onClick={() => {
                const name = window.prompt(`New folder in “${folder.name}”`)?.trim();
                if (name) create.mutate({ name, parent_id: folder.id }, { onError: (e) => toast.error(e.message) });
              }}
            >
              <FolderPlus /> New subfolder
            </DropdownMenuItem>
          )}
          <DropdownMenuItem
            onClick={() => {
              const name = window.prompt("Rename folder", folder.name)?.trim();
              if (name && name !== folder.name) update.mutate({ id: folder.id, name }, { onError: (e) => toast.error(e.message) });
            }}
          >
            <Pencil /> Rename
          </DropdownMenuItem>
          <DropdownMenuItem onClick={() => onMove(folder)}>
            <FolderInput /> Move to…
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            onClick={() => {
              const message =
                children.length > 0
                  ? `Delete “${folderPath(folders, folder.id)}” and its subfolders? Queries are kept, without a folder.`
                  : `Delete “${folder.name}”? Its queries are kept, without a folder.`;
              if (window.confirm(message)) del.mutate(folder.id, { onSuccess: () => selected === folder.id && onSelect(null) });
            }}
          >
            <Trash2 /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function FolderTreeNav({
  selected,
  onSelect,
  onMove,
}: {
  selected: string | null;
  onSelect: (id: string | null) => void;
  onMove: (folder: Collection) => void;
}) {
  const collections = useCollections();
  const create = useCreateCollection();
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const folders = collections.data ?? [];

  const render = (nodes: FolderNode[]): ReactNode =>
    nodes.map((node) => {
      const open = !collapsed[node.folder.id];
      return (
        <div
          key={node.folder.id}
          role="treeitem"
          aria-selected={selected === node.folder.id}
          aria-expanded={node.children.length > 0 ? open : undefined}
        >
          <FolderRow
            node={node}
            folders={folders}
            selected={selected}
            open={open}
            onToggle={() => setCollapsed((c) => ({ ...c, [node.folder.id]: open }))}
            onSelect={onSelect}
            onMove={onMove}
          />
          {open && node.children.length > 0 && <div role="group">{render(node.children)}</div>}
        </div>
      );
    });

  return (
    <nav className="space-y-0.5" aria-label="Folders">
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
      <div role="tree" aria-label="Folder tree">
        {render(folderTree(folders))}
      </div>
      <Button
        variant="ghost"
        size="sm"
        className="mt-1 w-full justify-start gap-2 text-muted-foreground"
        onClick={() => {
          const name = window.prompt("Folder name")?.trim();
          if (name) create.mutate({ name }, { onError: (e) => toast.error(e.message), onSuccess: (c) => onSelect(c.id) });
        }}
      >
        <FolderPlus className="size-4" /> New folder
      </Button>
    </nav>
  );
}

function MoveFolderForm({ folder, onClose }: { folder: Collection; onClose: () => void }) {
  const collections = useCollections();
  const update = useUpdateCollection();
  const folders = collections.data ?? [];
  const [target, setTarget] = useState<string | null>(folder.parent_id);

  return (
    <form
      className="space-y-4"
      onSubmit={(e) => {
        e.preventDefault();
        update.mutate({ id: folder.id, parent_id: target }, { onSuccess: () => (toast.success("Folder moved"), onClose()) });
      }}
    >
      <div className="space-y-1.5">
        <Label htmlFor="move-target">New location</Label>
        <FolderSelect
          id="move-target"
          folders={folders}
          value={target}
          onChange={setTarget}
          options={moveTargets(folders, folder.id)}
          noneLabel="Top level"
        />
        <p className="text-xs text-muted-foreground">
          Folders nest up to {MAX_FOLDER_DEPTH} levels; a folder cannot move into its own subfolders.
        </p>
      </div>
      {update.error && <p className="text-sm text-destructive">{update.error.message}</p>}
      <DialogFooter>
        <Button type="submit" disabled={update.isPending || target === folder.parent_id}>
          Move
        </Button>
      </DialogFooter>
    </form>
  );
}

export default function SavedQueriesPage() {
  const [collectionId, setCollectionId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [activeTags, setActiveTags] = useState<string[]>([]);
  const [pinnedOnly, setPinnedOnly] = useState(false);
  const q = useDeferredValue(search.trim());
  const [editing, setEditing] = useState<SavedQuery | null>(null);
  const [moving, setMoving] = useState<Collection | null>(null);
  const collections = useCollections();
  const tags = useTags();
  const saved = useSavedQueries({
    ...(q ? { q } : {}),
    ...(collectionId ? { collection_id: collectionId } : {}),
    ...(activeTags.length > 0 ? { tags: activeTags.join(",") } : {}),
    ...(pinnedOnly ? { pinned: true } : {}),
  });
  const folders = collections.data ?? [];
  const toggleTag = (tag: string) =>
    setActiveTags((current) => (current.includes(tag) ? current.filter((t) => t !== tag) : [...current, tag]));
  const filtering = Boolean(q || activeTags.length || pinnedOnly);

  return (
    <PageContainer title="Saved Queries" description="Organise, tag, pin and reopen the queries you rely on.">
      <div className="grid gap-6 md:grid-cols-[15rem_1fr]">
        <FolderTreeNav selected={collectionId} onSelect={setCollectionId} onMove={setMoving} />
        <div className="min-w-0">
          <div className="mb-3 flex flex-wrap gap-2">
            <div className="relative min-w-56 flex-1">
              <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search by name, description or SQL…"
                className="pl-8"
                aria-label="Search saved queries"
              />
            </div>
            <Button
              variant={pinnedOnly ? "secondary" : "outline"}
              aria-pressed={pinnedOnly}
              className="gap-1.5"
              onClick={() => setPinnedOnly((p) => !p)}
            >
              <Pin className="size-3.5" /> Pinned
            </Button>
          </div>
          {(tags.data ?? []).length > 0 && (
            <div className="mb-3 flex flex-wrap items-center gap-1.5" aria-label="Filter by tag">
              <Tag className="size-3.5 text-muted-foreground" />
              {(tags.data ?? []).map((t) => {
                const active = activeTags.includes(t.tag);
                return (
                  <button
                    key={t.tag}
                    type="button"
                    aria-pressed={active}
                    onClick={() => toggleTag(t.tag)}
                    className={cn(
                      "rounded-full border px-2 py-0.5 text-xs",
                      active ? "border-brand bg-brand/15 text-brand" : "text-muted-foreground hover:bg-accent",
                    )}
                  >
                    {t.tag} <span className="tabular-nums opacity-70">{t.count}</span>
                  </button>
                );
              })}
              {activeTags.length > 0 && (
                <Button variant="ghost" size="xs" onClick={() => setActiveTags([])}>
                  Clear
                </Button>
              )}
            </div>
          )}
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
                title={filtering ? "No matching queries" : "No saved queries"}
                description={filtering ? "Try a different search, tag or filter." : "Press Ctrl + S in the editor to save the current query."}
              />
            </div>
          ) : (
            <ul className="grid gap-3 lg:grid-cols-2">
              {saved.data.map((query) => (
                <QueryCard
                  key={query.id}
                  query={query}
                  folder={query.collection_id ? folderPath(folders, query.collection_id) : undefined}
                  onEdit={() => setEditing(query)}
                  onTag={(tag) => !activeTags.includes(tag) && toggleTag(tag)}
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
          {editing && <EditQueryForm key={editing.id} query={editing} onClose={() => setEditing(null)} />}
        </DialogContent>
      </Dialog>
      <Dialog open={moving !== null} onOpenChange={(o) => !o && setMoving(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Move folder</DialogTitle>
            <DialogDescription>{moving && folderPath(folders, moving.id)}</DialogDescription>
          </DialogHeader>
          {moving && <MoveFolderForm key={moving.id} folder={moving} onClose={() => setMoving(null)} />}
        </DialogContent>
      </Dialog>
    </PageContainer>
  );
}
