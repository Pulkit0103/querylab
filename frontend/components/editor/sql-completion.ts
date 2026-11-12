/**
 * Schema-aware SQL autocomplete for Monaco.
 *
 * The provider is registered once per Monaco instance and reads the current
 * catalog from a mutable holder, so switching connections never registers
 * duplicate providers.
 */
import type { CatalogEntry } from "@/hooks/use-schema";
import { quoteIdent } from "@/lib/sql-ident";
import { statementAt } from "@/lib/sql-statements";

import type { MonacoApi } from "./sql-editor";

type Monaco = MonacoApi;
type ITextModel = ReturnType<Monaco["editor"]["createModel"]>;
type Position = { lineNumber: number; column: number };

const KEYWORDS = [
  "SELECT", "FROM", "WHERE", "GROUP BY", "ORDER BY", "HAVING", "LIMIT", "OFFSET", "JOIN", "LEFT JOIN",
  "RIGHT JOIN", "FULL JOIN", "INNER JOIN", "CROSS JOIN", "LATERAL", "ON", "USING", "AS", "AND", "OR", "NOT",
  "IN", "EXISTS", "BETWEEN", "LIKE", "ILIKE", "IS NULL", "IS NOT NULL", "DISTINCT", "DISTINCT ON", "UNION",
  "UNION ALL", "INTERSECT", "EXCEPT", "WITH", "RECURSIVE", "CASE", "WHEN", "THEN", "ELSE", "END", "INSERT INTO",
  "VALUES", "UPDATE", "SET", "DELETE FROM", "RETURNING", "ON CONFLICT", "DO NOTHING", "DO UPDATE", "CREATE TABLE",
  "CREATE INDEX", "CREATE VIEW", "ALTER TABLE", "DROP TABLE", "TRUNCATE", "EXPLAIN", "EXPLAIN ANALYZE", "ASC",
  "DESC", "NULLS FIRST", "NULLS LAST", "FETCH FIRST", "ROWS ONLY", "OVER", "PARTITION BY", "WINDOW", "FILTER",
  "WITHIN GROUP", "TRUE", "FALSE", "NULL", "INTERVAL", "CAST",
];

const FUNCTIONS = [
  "count", "sum", "avg", "min", "max", "coalesce", "nullif", "greatest", "least", "now", "current_date",
  "current_timestamp", "date_trunc", "date_part", "extract", "to_char", "to_date", "to_timestamp", "age",
  "lower", "upper", "length", "substring", "replace", "trim", "concat", "concat_ws", "split_part", "string_agg",
  "array_agg", "jsonb_agg", "json_agg", "jsonb_build_object", "jsonb_extract_path_text", "row_number", "rank",
  "dense_rank", "lag", "lead", "first_value", "last_value", "ntile", "percentile_cont", "generate_series",
  "unnest", "round", "floor", "ceil", "abs", "random", "md5", "gen_random_uuid", "pg_size_pretty",
  "pg_total_relation_size",
];

interface Holder {
  catalog: CatalogEntry[];
}

const holders = new WeakMap<Monaco, Holder>();

/** Updates the catalog used by autocomplete for this Monaco instance. */
export function setCompletionCatalog(monaco: Monaco, catalog: CatalogEntry[]) {
  const holder = holders.get(monaco);
  if (holder) holder.catalog = catalog;
}

const IDENT = String.raw`(?:"(?:[^"]|"")+"|[A-Za-z_][A-Za-z0-9_$]*)`;
const unquote = (s: string) => (s.startsWith('"') ? s.slice(1, -1).replace(/""/g, '"') : s.toLowerCase());

/** Maps alias (and bare table name) → table for tables referenced in FROM/JOIN/UPDATE/INTO clauses. */
export function referencedTables(statement: string, catalog: CatalogEntry[]) {
  const re = new RegExp(
    String.raw`\b(?:from|join|update|into)\s+(${IDENT})(?:\s*\.\s*(${IDENT}))?(?:\s+(?:as\s+)?(${IDENT}))?`,
    "gi",
  );
  const reserved = new Set(["where", "on", "using", "join", "left", "right", "inner", "full", "cross", "group", "order", "limit", "set", "values", "natural", "lateral"]);
  const map = new Map<string, CatalogEntry>();
  for (const m of statement.matchAll(re)) {
    const [schema, table] = m[2] ? [unquote(m[1]), unquote(m[2])] : [null, unquote(m[1])];
    const entry = catalog.find((e) => e.table === table && (schema ? e.schema === schema : true));
    if (!entry) continue;
    map.set(entry.table.toLowerCase(), entry);
    const alias = m[3] ? unquote(m[3]) : null;
    if (alias && !reserved.has(alias.toLowerCase())) map.set(alias.toLowerCase(), entry);
  }
  return map;
}

export function registerSqlCompletion(monaco: Monaco) {
  if (holders.has(monaco)) return;
  const holder: Holder = { catalog: [] };
  holders.set(monaco, holder);
  const { CompletionItemKind, CompletionItemInsertTextRule } = monaco.languages;

  monaco.languages.registerCompletionItemProvider("sql", {
    triggerCharacters: [".", " "],
    provideCompletionItems(model: ITextModel, position: Position) {
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };
      const catalog = holder.catalog;
      const text = model.getValue();
      const offset = model.getOffsetAt(position);
      const statement = statementAt(text, offset)?.text ?? text;
      const before = model.getValueInRange({
        startLineNumber: position.lineNumber,
        startColumn: 1,
        endLineNumber: position.lineNumber,
        endColumn: word.startColumn,
      });

      // `qualifier.` → columns of a table/alias, or tables of a schema.
      const dot = new RegExp(String.raw`(${IDENT})\.\s*$`).exec(before);
      if (dot) {
        const qualifier = unquote(dot[1]).toLowerCase();
        const tables = referencedTables(statement, catalog);
        const entry = tables.get(qualifier) ?? catalog.find((e) => e.table.toLowerCase() === qualifier);
        if (entry) {
          return {
            suggestions: entry.columns.map((c, i) => ({
              label: { label: c, description: entry.column_types[i] },
              kind: CompletionItemKind.Field,
              insertText: quoteIdent(c),
              detail: `${entry.schema}.${entry.table}`,
              range,
              sortText: `0${String(i).padStart(4, "0")}`,
            })),
          };
        }
        const schemaTables = catalog.filter((e) => e.schema.toLowerCase() === qualifier);
        if (schemaTables.length > 0) {
          return {
            suggestions: schemaTables.map((e) => ({
              label: { label: e.table, description: e.kind.replace("_", " ") },
              kind: e.kind === "view" || e.kind === "materialized_view" ? CompletionItemKind.Interface : CompletionItemKind.Struct,
              insertText: quoteIdent(e.table),
              range,
            })),
          };
        }
        return { suggestions: [] };
      }

      const afterRelationKeyword = /\b(from|join|update|into|table)\s+$/i.test(before);
      const suggestions = [];

      // Tables (qualified when outside public).
      for (const e of catalog) {
        const insert = e.schema === "public" ? quoteIdent(e.table) : `${quoteIdent(e.schema)}.${quoteIdent(e.table)}`;
        suggestions.push({
          label: { label: e.schema === "public" ? e.table : `${e.schema}.${e.table}`, description: e.kind.replace("_", " ") },
          kind: e.kind === "view" || e.kind === "materialized_view" ? CompletionItemKind.Interface : CompletionItemKind.Struct,
          insertText: insert,
          detail: `${e.columns.length} columns`,
          range,
          sortText: afterRelationKeyword ? `0${e.table}` : `2${e.table}`,
        });
      }

      if (!afterRelationKeyword) {
        // Columns of tables referenced in the current statement.
        const seen = new Set<string>();
        for (const entry of new Set(referencedTables(statement, catalog).values())) {
          entry.columns.forEach((c, i) => {
            const key = `${entry.table}.${c}`;
            if (seen.has(key)) return;
            seen.add(key);
            suggestions.push({
              label: { label: c, description: `${entry.table} · ${entry.column_types[i]}` },
              kind: CompletionItemKind.Field,
              insertText: quoteIdent(c),
              range,
              sortText: `0${c}`,
            });
          });
        }
        for (const kw of KEYWORDS) {
          suggestions.push({ label: kw, kind: CompletionItemKind.Keyword, insertText: kw, range, sortText: `1${kw}` });
        }
        for (const fn of FUNCTIONS) {
          suggestions.push({
            label: { label: fn, description: "function" },
            kind: CompletionItemKind.Function,
            insertText: `${fn}($0)`,
            insertTextRules: CompletionItemInsertTextRule.InsertAsSnippet,
            range,
            sortText: `3${fn}`,
          });
        }
      }
      return { suggestions };
    },
  });
}
