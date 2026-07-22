import type { Span } from "./api";

const MARKDOWN_EXTENSIONS = [".md", ".markdown"];

/** Whether a repo-relative path names a markdown file (case-insensitive). */
export function isMarkdownPath(path: string): boolean {
  const lower = path.toLowerCase();
  return MARKDOWN_EXTENSIONS.some((ext) => lower.endsWith(ext));
}

/**
 * Reconstruct a file's raw text from its highlighted view (`/api/file`). The
 * spans only carry colour, never rewrite characters, so joining them per line
 * and by newline is lossless — letting the markdown renderer work off the same
 * payload the syntax view uses, without a second fetch.
 */
export function fileViewSource(lines: Span[][]): string {
  return lines.map((line) => line.map((s) => s.t).join("")).join("\n");
}
