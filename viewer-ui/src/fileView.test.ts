import { describe, expect, it } from "vitest";
import { fileViewSource, isMarkdownPath } from "./fileView";

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
