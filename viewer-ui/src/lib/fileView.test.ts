import { describe, expect, it } from "vitest";
import {
  fileViewSource,
  isHtmlPath,
  isMarkdownPath,
  isPreviewablePath,
} from "./fileView";

describe("isMarkdownPath", () => {
  it("일반적인_마크다운_확장자면_참을_반환한다", () => {
    expect(isMarkdownPath("README.md")).toBe(true);
    expect(isMarkdownPath("docs/architecture.markdown")).toBe(true);
  });

  it("확장자_대소문자를_구분하지_않는다", () => {
    expect(isMarkdownPath("NOTES.MD")).toBe(true);
    expect(isMarkdownPath("Guide.Markdown")).toBe(true);
  });

  it("마크다운이_아닌_경로면_거짓을_반환한다", () => {
    expect(isMarkdownPath("src/main.rs")).toBe(false);
    expect(isMarkdownPath("mdfile")).toBe(false);
    expect(isMarkdownPath("readme.md.bak")).toBe(false);
  });
});

describe("isHtmlPath", () => {
  it("html_확장자면_참을_반환한다", () => {
    expect(isHtmlPath("index.html")).toBe(true);
    expect(isHtmlPath("target/coverage/report.htm")).toBe(true);
  });

  it("확장자_대소문자를_구분하지_않는다", () => {
    expect(isHtmlPath("INDEX.HTML")).toBe(true);
    expect(isHtmlPath("Report.Htm")).toBe(true);
  });

  it("html이_아닌_경로면_거짓을_반환한다", () => {
    expect(isHtmlPath("src/main.rs")).toBe(false);
    expect(isHtmlPath("htmlfile")).toBe(false);
    // The preview renders what the extension claims, so a path that only
    // contains the extension must not opt into it.
    expect(isHtmlPath("index.html.bak")).toBe(false);
    expect(isHtmlPath("notes.md")).toBe(false);
  });
});

describe("isPreviewablePath", () => {
  it("마크다운과_html_모두_미리보기_대상이다", () => {
    expect(isPreviewablePath("README.md")).toBe(true);
    expect(isPreviewablePath("index.html")).toBe(true);
  });

  it("그_외_파일은_미리보기가_없다", () => {
    expect(isPreviewablePath("src/main.rs")).toBe(false);
    expect(isPreviewablePath("Cargo.toml")).toBe(false);
  });
});

describe("fileViewSource", () => {
  it("스팬을_이어붙여_원문을_무손실로_복원한다", () => {
    const lines = [
      [
        { t: "# ", c: "#fff" },
        { t: "Title", c: "#abc" },
      ],
      [{ t: "body", c: "#def" }],
    ];
    expect(fileViewSource(lines)).toBe("# Title\nbody");
  });

  it("빈_줄을_보존한다", () => {
    const lines = [[{ t: "a", c: "#fff" }], [], [{ t: "b", c: "#fff" }]];
    expect(fileViewSource(lines)).toBe("a\n\nb");
  });

  it("빈_파일이면_빈_문자열을_반환한다", () => {
    expect(fileViewSource([])).toBe("");
  });
});
