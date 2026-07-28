import type { Span } from "../api";

const MARKDOWN_EXTENSIONS = [".md", ".markdown"];
const HTML_EXTENSIONS = [".html", ".htm"];

export function isMarkdownPath(path: string): boolean {
  const lower = path.toLowerCase();
  return MARKDOWN_EXTENSIONS.some((ext) => lower.endsWith(ext));
}

export function isHtmlPath(path: string): boolean {
  const lower = path.toLowerCase();
  return HTML_EXTENSIONS.some((ext) => lower.endsWith(ext));
}

/** Whether this path has a preview to toggle to at all. */
export function isPreviewablePath(path: string): boolean {
  return isMarkdownPath(path) || isHtmlPath(path);
}

/** Reuse highlighted spans to avoid a second fetch. */
export function fileViewSource(lines: Span[][]): string {
  return lines.map((line) => line.map((s) => s.t).join("")).join("\n");
}
