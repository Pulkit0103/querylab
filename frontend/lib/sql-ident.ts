// A small set of reserved words that always require quoting as identifiers.
const RESERVED = new Set([
  "all", "analyse", "analyze", "and", "any", "array", "as", "asc", "asymmetric", "both", "case", "cast",
  "check", "collate", "column", "constraint", "create", "current_catalog", "current_date", "current_role",
  "current_time", "current_timestamp", "current_user", "default", "deferrable", "desc", "distinct", "do",
  "else", "end", "except", "false", "fetch", "for", "foreign", "from", "grant", "group", "having", "in",
  "initially", "intersect", "into", "lateral", "leading", "limit", "localtime", "localtimestamp", "not",
  "null", "offset", "on", "only", "or", "order", "placing", "primary", "references", "returning", "select",
  "session_user", "some", "symmetric", "table", "then", "to", "trailing", "true", "union", "unique", "user",
  "using", "variadic", "when", "where", "window", "with",
]);

/** Quotes a PostgreSQL identifier only when necessary. */
export function quoteIdent(name: string): string {
  if (/^[a-z_][a-z0-9_$]*$/.test(name) && !RESERVED.has(name)) return name;
  return `"${name.replace(/"/g, '""')}"`;
}

/** `schema.table`, omitting `public` for brevity. */
export function qualifiedName(schema: string, table: string): string {
  return schema === "public" ? quoteIdent(table) : `${quoteIdent(schema)}.${quoteIdent(table)}`;
}

export function previewSql(schema: string, table: string, limit = 100): string {
  return `SELECT *\nFROM ${qualifiedName(schema, table)}\nLIMIT ${limit};\n`;
}

/** `SELECT` with an explicit column list. */
export function selectSql(schema: string, table: string, columns: string[], limit = 100): string {
  const cols = columns.length ? columns.map(quoteIdent).join(",\n       ") : "*";
  return `SELECT ${cols}\nFROM ${qualifiedName(schema, table)}\nLIMIT ${limit};\n`;
}

/**
 * `INSERT` template with one placeholder per column. Placeholders are
 * `NULL` annotated with the column type so the statement is valid SQL
 * that can be edited in place. Columns with defaults are listed last and
 * marked so they can be removed.
 */
export function insertTemplate(
  schema: string,
  table: string,
  columns: { name: string; data_type: string; default_value: string | null }[],
): string {
  const ordered = [...columns.filter((c) => !c.default_value), ...columns.filter((c) => c.default_value)];
  const names = ordered.map((c) => quoteIdent(c.name)).join(", ");
  const values = ordered
    .map((c) => `  NULL /* ${c.name} ${c.data_type}${c.default_value ? ", has default" : ""} */`)
    .join(",\n");
  return `INSERT INTO ${qualifiedName(schema, table)} (${names})\nVALUES (\n${values}\n);\n`;
}
