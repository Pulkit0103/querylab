/** `{{placeholder}}` templates for snippets (mirrors `models::workspaces::placeholders`). */

const PLACEHOLDER = /\{\{\s*([\p{L}\p{N}_\- ]{1,50}?)\s*\}\}/gu;

/** Placeholder names in order of first appearance. */
export function placeholders(body: string): string[] {
  const names: string[] = [];
  for (const m of body.matchAll(PLACEHOLDER)) {
    const name = m[1].trim();
    if (name && !names.includes(name)) names.push(name);
  }
  return names;
}

/** Replaces every placeholder with its value (unknown ones are kept). */
export function fillSnippet(body: string, values: Record<string, string>): string {
  return body.replace(PLACEHOLDER, (whole, raw: string) => {
    const name = raw.trim();
    return Object.prototype.hasOwnProperty.call(values, name) ? values[name] : whole;
  });
}
