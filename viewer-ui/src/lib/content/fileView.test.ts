import { describe, expect, it } from "vitest";
import {
  fileViewSource,
  isHtmlPath,
  isMarkdownPath,
  isPreviewablePath,
} from "./fileView";

describe("isMarkdownPath", () => {
  it("returns true for common Markdown extensions", () => {
    expect(isMarkdownPath("README.md")).toBe(true);
    expect(isMarkdownPath("docs/architecture.markdown")).toBe(true);
  });

  it("is case-insensitive about extensions", () => {
    expect(isMarkdownPath("NOTES.MD")).toBe(true);
    expect(isMarkdownPath("Guide.Markdown")).toBe(true);
  });

  it("returns false for a non-Markdown path", () => {
    expect(isMarkdownPath("src/main.rs")).toBe(false);
    expect(isMarkdownPath("mdfile")).toBe(false);
    expect(isMarkdownPath("readme.md.bak")).toBe(false);
  });
});

describe("isHtmlPath", () => {
  it("returns true for an HTML extension", () => {
    expect(isHtmlPath("index.html")).toBe(true);
    expect(isHtmlPath("target/coverage/report.htm")).toBe(true);
  });

  it("is case-insensitive about extensions", () => {
    expect(isHtmlPath("INDEX.HTML")).toBe(true);
    expect(isHtmlPath("Report.Htm")).toBe(true);
  });

  it("returns false for a non-HTML path", () => {
    expect(isHtmlPath("src/main.rs")).toBe(false);
    expect(isHtmlPath("htmlfile")).toBe(false);
    // The preview renders what the extension claims, so a path that only
    // contains the extension must not opt into it.
    expect(isHtmlPath("index.html.bak")).toBe(false);
    expect(isHtmlPath("notes.md")).toBe(false);
  });
});

describe("isPreviewablePath", () => {
  it("Markdown and HTML are both previewable", () => {
    expect(isPreviewablePath("README.md")).toBe(true);
    expect(isPreviewablePath("index.html")).toBe(true);
  });

  it("other files have no preview", () => {
    expect(isPreviewablePath("src/main.rs")).toBe(false);
    expect(isPreviewablePath("Cargo.toml")).toBe(false);
  });
});

describe("fileViewSource", () => {
  it("reconstructs the original without loss by joining spans", () => {
    const lines = [
      [
        { t: "# ", c: "#fff" },
        { t: "Title", c: "#abc" },
      ],
      [{ t: "body", c: "#def" }],
    ];
    expect(fileViewSource(lines)).toBe("# Title\nbody");
  });

  it("preserves blank lines", () => {
    const lines = [[{ t: "a", c: "#fff" }], [], [{ t: "b", c: "#fff" }]];
    expect(fileViewSource(lines)).toBe("a\n\nb");
  });

  it("returns an empty string for an empty file", () => {
    expect(fileViewSource([])).toBe("");
  });
});
