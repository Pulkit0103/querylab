"use client";

import { useState, type FormEvent } from "react";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { useCollections, useCreateSavedQuery } from "@/hooks/use-library";
import type { SavedQuery } from "@/types/library";

const NONE = "__none__";

function SaveForm({
  defaultName,
  sql,
  connectionId,
  onSaved,
}: {
  defaultName: string;
  sql: string;
  connectionId: string | null;
  onSaved: (saved: SavedQuery) => void;
}) {
  const [name, setName] = useState(defaultName);
  const [description, setDescription] = useState("");
  const [collection, setCollection] = useState<string>(NONE);
  const collections = useCollections();
  const create = useCreateSavedQuery();

  const items = [{ value: NONE, label: "No collection" }, ...(collections.data ?? []).map((c) => ({ value: c.id, label: c.name }))];

  const submit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate(
      {
        name: name.trim(),
        description: description.trim() || null,
        sql_content: sql,
        connection_id: connectionId,
        collection_id: collection === NONE ? null : collection,
      },
      {
        onSuccess: (saved) => {
          toast.success(`Saved “${saved.name}”`);
          onSaved(saved);
        },
      },
    );
  };

  return (
    <form onSubmit={submit} className="space-y-4">
      <div className="space-y-1.5">
        <Label htmlFor="sq-name">Name</Label>
        <Input id="sq-name" value={name} onChange={(e) => setName(e.target.value)} autoFocus maxLength={200} />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="sq-desc">Description</Label>
        <Textarea
          id="sq-desc"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          rows={3}
          maxLength={2000}
          placeholder="What does this query answer?"
        />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="sq-collection">Collection</Label>
        <Select items={items} value={collection} onValueChange={(v) => setCollection(String(v ?? NONE))}>
          <SelectTrigger id="sq-collection" className="w-full">
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
      {create.error && <p className="text-sm text-destructive">{create.error.message}</p>}
      <DialogFooter>
        <Button
          type="submit"
          disabled={!name.trim() || create.isPending}
          className="gap-1.5 bg-brand text-brand-foreground hover:bg-brand/85"
        >
          {create.isPending && <Loader2 className="size-3.5 animate-spin" />} Save query
        </Button>
      </DialogFooter>
    </form>
  );
}

export function SaveQueryDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaultName: string;
  sql: string;
  connectionId: string | null;
  onSaved: (saved: SavedQuery) => void;
}) {
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Save query</DialogTitle>
          <DialogDescription>Saved queries are private to your account.</DialogDescription>
        </DialogHeader>
        {props.open && (
          <SaveForm
            defaultName={props.defaultName}
            sql={props.sql}
            connectionId={props.connectionId}
            onSaved={(s) => {
              props.onSaved(s);
              props.onOpenChange(false);
            }}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}
