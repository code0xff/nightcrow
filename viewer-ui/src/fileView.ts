import type { Span } from "./api";

const MARKDOWN_EXTENSIONS = [".md", ".markdown"];

export function isMarkdownPath(path: string): boolean {
  const lower = path.toLowerCase();
  return MARKDOWN_EXTENSIONS.some((ext) => lower.endsWith(ext));
}

/** Reuse highlighted spans to avoid a second fetch. */
export function fileViewSource(lines: Span[][]): string {
  return lines.map((line) => line.map((s) => s.t).join("")).join("\n");
}
