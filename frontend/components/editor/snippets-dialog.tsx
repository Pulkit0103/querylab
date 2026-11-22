"use client";

import { useMemo, useState } from "react";
import { ArrowLeft, Pencil, Plus, Puzzle, Search, Trash2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useActiveWorkspace, useDeleteSnippet, useSaveSnippet, useSnippets } from "@/hooks/use-workspaces";
import { fillSnippet, placeholders } from "@/lib/snippets";
import type { Snippet } from "@/types/workspaces";

type View = { kind: "list" } | { kind: "use"; snippet: Snippet } | { kind: "edit"; snippet: Snippet | null };

function SnippetForm({ snippet, initialBody, onDone }: { snippet: Snippet | null; initialBody: string; onDone: () => void }) {
  const save = useSaveSnippet();
  const [name, setName] = useState(snippet?.name ?? "");
  const [description, setDescription] = useState(snippet?.description ?? "");
  const [body, setBody] = useState(snippet?.body ?? initialBody);
  const [tags, setTags] = useState(snippet?.tags.join(", ") ?? "");
  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate(
          {
            id: snippet?.id,
            body: {
              name: name.trim(),
              description: description.trim() || null,
              body,
              tags: tags.split(",").map((t) => t.trim()).filter(Boolean),
            },
          },
          {
            onSuccess: (s) => {
              toast.success(`Saved snippet “${s.name}”`);
              onDone();
            },
            onError: (err) => toast.error(err.message),
          },
        );
      }}
    >
      <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Name" aria-label="Snippet name" maxLength={100} />
      <Input value={description} onChange={(e) => setDescription(e.target.value)} placeholder="Description (optional)" aria-label="Description" maxLength={500} />
      <Textarea
        value={body}
        onChange={(e) => setBody(e.target.value)}
        aria-label="Snippet SQL"
        rows={8}
        className="font-mono text-xs"
        placeholder="select * from {{table}} where created_at > now() - interval '{{days}} days'"
      />
      <p className="text-[11px] text-muted-foreground">
        Use <code className="font-mono">{"{{name}}"}</code> for values asked when the snippet is inserted
        {placeholders(body).length > 0 && `: ${placeholders(body).join(", ")}`}.
      </p>
      <Input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="Tags, comma separated" aria-label="Tags" />
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" disabled={!name.trim() || !body.trim() || save.isPending}>
          Save snippet
        </Button>
      </div>
    </form>
  );
}

function UseSnippet({ snippet, onInsert }: { snippet: Snippet; onInsert: (text: string) => void }) {
  const names = placeholders(snippet.body);
  const [values, setValues] = useState<Record<string, string>>({});
  const filled = fillSnippet(snippet.body, values);
  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault();
        onInsert(filled);
      }}
    >
      {names.map((n, i) => (
        <label key={n} className="grid grid-cols-[8rem_1fr] items-center gap-2 text-xs">
          <span className="truncate font-mono">{n}</span>
          <Input
            autoFocus={i === 0}
            value={values[n] ?? ""}
            onChange={(e) => setValues((v) => ({ ...v, [n]: e.target.value }))}
            aria-label={`Value for ${n}`}
            className="h-8"
          />
        </label>
      ))}
      <pre className="max-h-56 overflow-auto rounded-md border bg-panel p-2 font-mono text-[11px] whitespace-pre-wrap">{filled}</pre>
      <div className="flex justify-end">
        <Button type="submit">Insert into editor</Button>
      </div>
    </form>
  );
}

/** The workspace's snippet library: insert with placeholders, save, edit. */
export function SnippetsButton({ getSelection, onInsert }: { getSelection: () => string; onInsert: (text: string) => void }) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<View>({ kind: "list" });
  const [search, setSearch] = useState("");
  const [initialBody, setInitialBody] = useState("");
  const snippets = useSnippets();
  const remove = useDeleteSnippet();
  const { workspace } = useActiveWorkspace();
  const canWrite = workspace?.role !== "viewer";

  const shown = useMemo(() => {
    const q = search.trim().toLowerCase();
    return (snippets.data ?? []).filter(
      (s) =>
        !q ||
        s.name.toLowerCase().includes(q) ||
        (s.description ?? "").toLowerCase().includes(q) ||
        s.tags.some((t) => t.toLowerCase().includes(q)),
    );
  }, [snippets.data, search]);

  return (
    <>
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              variant="ghost"
              size="sm"
              aria-label="Snippets"
              onClick={() => {
                setView({ kind: "list" });
                setOpen(true);
              }}
            />
          }
        >
          <Puzzle className="size-3.5" />
        </TooltipTrigger>
        <TooltipContent side="bottom">Snippets</TooltipContent>
      </Tooltip>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              {view.kind !== "list" && (
                <Button size="icon-xs" variant="ghost" aria-label="Back" onClick={() => setView({ kind: "list" })}>
                  <ArrowLeft className="size-3.5" />
                </Button>
              )}
              {view.kind === "use" ? view.snippet.name : view.kind === "edit" ? (view.snippet ? "Edit snippet" : "New snippet") : "Snippets"}
            </DialogTitle>
            <DialogDescription>
              {view.kind === "use"
                ? (view.snippet.description ?? "Fill in the values, then insert.")
                : `Reusable SQL shared in ${workspace?.personal ? "your personal workspace" : (workspace?.name ?? "this workspace")}.`}
            </DialogDescription>
          </DialogHeader>
          {view.kind === "use" ? (
            <UseSnippet
              snippet={view.snippet}
              onInsert={(text) => {
                onInsert(text);
                setOpen(false);
              }}
            />
          ) : view.kind === "edit" ? (
            <SnippetForm snippet={view.snippet} initialBody={initialBody} onDone={() => setView({ kind: "list" })} />
          ) : (
            <div className="space-y-3">
              <div className="flex gap-2">
                <div className="relative flex-1">
                  <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                  <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Search snippets" aria-label="Search snippets" className="h-8 pl-7" />
                </div>
                {canWrite && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => {
                      setInitialBody(getSelection());
                      setView({ kind: "edit", snippet: null });
                    }}
                  >
                    <Plus className="size-3.5" /> From selection
                  </Button>
                )}
              </div>
              {snippets.isPending ? (
                <Skeleton className="h-32" />
              ) : shown.length === 0 ? (
                <p className="py-6 text-center text-xs text-muted-foreground">
                  {snippets.data?.length ? "No snippet matches." : "No snippets yet. Select some SQL and save it as a snippet."}
                </p>
              ) : (
                <ul className="max-h-80 divide-y overflow-auto rounded-md border">
                  {shown.map((s) => (
                    <li key={s.id} className="group flex items-start gap-2 px-3 py-2 text-xs hover:bg-muted/40">
                      <button type="button" className="min-w-0 flex-1 text-left" onClick={() => setView({ kind: "use", snippet: s })}>
                        <p className="truncate font-medium">{s.name}</p>
                        <p className="truncate font-mono text-[11px] text-muted-foreground">{s.body.replace(/\s+/g, " ")}</p>
                        <p className="mt-0.5 text-[10px] text-muted-foreground">
                          {s.tags.map((t) => `#${t}`).join(" ")} {s.created_by_email && `· by ${s.created_by_email}`}
                        </p>
                      </button>
                      {canWrite && (
                        <span className="flex shrink-0 gap-0.5 opacity-0 group-hover:opacity-100 focus-within:opacity-100">
                          <Button size="icon-xs" variant="ghost" aria-label={`Edit ${s.name}`} onClick={() => setView({ kind: "edit", snippet: s })}>
                            <Pencil className="size-3" />
                          </Button>
                          <Button
                            size="icon-xs"
                            variant="ghost"
                            aria-label={`Delete ${s.name}`}
                            onClick={() => remove.mutate(s.id, { onError: (e) => toast.error(e.message) })}
                          >
                            <Trash2 className="size-3" />
                          </Button>
                        </span>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
