/**
 * Splits SQL text into statements on top-level semicolons, respecting
 * PostgreSQL lexical rules: single quotes (with '' escapes), E'' strings
 * (backslash escapes), double-quoted identifiers, dollar-quoted strings
 * ($$...$$ / $tag$...$tag$), line comments and nested block comments.
 *
 * Used for "run statement under cursor". The server still enforces that a
 * single statement is executed.
 */

export interface SqlStatement {
  /** Statement text without the trailing semicolon. */
  text: string;
  /** Offset of the first character of the statement (including leading whitespace). */
  start: number;
  /** Offset just past the statement's terminating semicolon (or end of input). */
  end: number;
}

const DOLLAR_TAG = /^\$([A-Za-z_][A-Za-z0-9_]*)?\$/;

export function splitStatements(sql: string): SqlStatement[] {
  const out: SqlStatement[] = [];
  let start = 0;
  let i = 0;
  const n = sql.length;

  const push = (endExclusive: number, next: number) => {
    out.push({ text: sql.slice(start, endExclusive), start, end: next });
    start = next;
  };

  while (i < n) {
    const ch = sql[i];
    const next = sql[i + 1];

    if (ch === "-" && next === "-") {
      const nl = sql.indexOf("\n", i);
      i = nl === -1 ? n : nl + 1;
      continue;
    }
    if (ch === "/" && next === "*") {
      let depth = 1;
      i += 2;
      while (i < n && depth > 0) {
        if (sql[i] === "/" && sql[i + 1] === "*") {
          depth++;
          i += 2;
        } else if (sql[i] === "*" && sql[i + 1] === "/") {
          depth--;
          i += 2;
        } else i++;
      }
      continue;
    }
    if (ch === "'") {
      const escapes = i > 0 && (sql[i - 1] === "E" || sql[i - 1] === "e") && !/[A-Za-z0-9_]/.test(sql[i - 2] ?? "");
      i++;
      while (i < n) {
        if (escapes && sql[i] === "\\") {
          i += 2;
          continue;
        }
        if (sql[i] === "'") {
          if (sql[i + 1] === "'") {
            i += 2;
            continue;
          }
          break;
        }
        i++;
      }
      i++;
      continue;
    }
    if (ch === '"') {
      i++;
      while (i < n) {
        if (sql[i] === '"') {
          if (sql[i + 1] === '"') {
            i += 2;
            continue;
          }
          break;
        }
        i++;
      }
      i++;
      continue;
    }
    if (ch === "$" && !/[A-Za-z0-9_]/.test(sql[i - 1] ?? "")) {
      const m = DOLLAR_TAG.exec(sql.slice(i));
      if (m) {
        const tag = m[0];
        const close = sql.indexOf(tag, i + tag.length);
        i = close === -1 ? n : close + tag.length;
        continue;
      }
    }
    if (ch === ";") {
      push(i, i + 1);
      i++;
      continue;
    }
    i++;
  }
  if (start < n) push(n, n);
  return out;
}

/** Whether a statement contains anything other than whitespace and comments. */
export function hasContent(text: string): boolean {
  const stripped = text
    .replace(/--[^\n]*/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .trim();
  return stripped.length > 0;
}

/**
 * Finds the statement at `offset`. When the cursor sits in whitespace after a
 * statement's semicolon, the preceding statement is used (matches DataGrip).
 */
export function statementAt(sql: string, offset: number): SqlStatement | null {
  const statements = splitStatements(sql).filter((s) => hasContent(s.text));
  if (statements.length === 0) return null;
  for (let idx = 0; idx < statements.length; idx++) {
    const s = statements[idx];
    if (offset >= s.start && offset < s.end) {
      return s;
    }
    if (offset < s.start) {
      // Between statements: prefer the previous one if the gap is only whitespace.
      const prev = statements[idx - 1];
      if (prev && !sql.slice(prev.end, offset).trim()) return prev;
      return s;
    }
  }
  return statements[statements.length - 1];
}

/** Trims leading whitespace from a statement, returning the adjusted offset. */
export function trimStatement(s: SqlStatement): { text: string; offset: number } {
  const lead = s.text.length - s.text.trimStart().length;
  return { text: s.text.trim(), offset: s.start + lead };
}
