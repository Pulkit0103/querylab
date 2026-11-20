"use client";

import { useId, useState, type KeyboardEvent } from "react";
import { X } from "lucide-react";

import { MAX_TAGS, normalizeTags, tagError } from "@/lib/library";
import { cn } from "@/lib/utils";

/**
 * Tag chips with an input. Enter or comma adds a tag, Backspace on an
 * empty input removes the last one. Suggestions come from existing tags.
 */
export function TagEditor({
  value,
  onChange,
  suggestions = [],
  id,
}: {
  value: string[];
  onChange: (tags: string[]) => void;
  suggestions?: string[];
  id?: string;
}) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const listId = useId();

  const add = (raw: string) => {
    const problem = tagError(value, raw);
    if (problem) {
      if (raw.trim()) setError(problem);
      return false;
    }
    onChange(normalizeTags([...value, raw]));
    setDraft("");
    setError(null);
    return true;
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      add(draft);
    } else if (e.key === "Backspace" && !draft && value.length > 0) {
      onChange(value.slice(0, -1));
    }
  };

  return (
    <div className="space-y-1">
      <div
        className={cn(
          "flex min-h-9 flex-wrap items-center gap-1 rounded-md border bg-background px-2 py-1 focus-within:ring-2 focus-within:ring-ring",
          error && "border-destructive",
        )}
      >
        {value.map((tag) => (
          <span key={tag} className="flex items-center gap-0.5 rounded bg-brand/15 py-0.5 pr-0.5 pl-1.5 text-xs text-brand">
            {tag}
            <button
              type="button"
              aria-label={`Remove tag ${tag}`}
              onClick={() => onChange(value.filter((t) => t !== tag))}
              className="rounded p-0.5 hover:bg-brand/20"
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
        <input
          id={id}
          value={draft}
          list={listId}
          disabled={value.length >= MAX_TAGS}
          onChange={(e) => {
            setDraft(e.target.value);
            setError(null);
          }}
          onKeyDown={onKeyDown}
          onBlur={() => draft.trim() && add(draft)}
          placeholder={value.length === 0 ? "Add tags…" : ""}
          aria-label="Add tag"
          className="min-w-24 flex-1 bg-transparent py-0.5 text-sm outline-none"
        />
        <datalist id={listId}>
          {suggestions
            .filter((s) => !value.includes(s))
            .map((s) => (
              <option key={s} value={s} />
            ))}
        </datalist>
      </div>
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}
