import { describe, expect, it } from "vitest";
import { parseBlocks } from "./parse";
import {
  MARKER_ATTR,
  injectAgentScript,
  injectEditorStyle,
  injectMarkers,
} from "./markers";

describe("injectMarkers", () => {
  it("marks each block's opening tag and keeps the text between the marks", () => {
    const source = "<p>one</p><p>two</p>";
    const marked = injectMarkers(source, parseBlocks(source));
    expect(marked).toBe(
      `<p ${MARKER_ATTR}="0">one</p><p ${MARKER_ATTR}="1">two</p>`,
    );
  });

  it("marks a cell without disturbing the table around it", () => {
    const source = "<table><tr><td>cell</td></tr></table>";
    const marked = injectMarkers(source, parseBlocks(source));
    expect(marked).toContain(`<td ${MARKER_ATTR}=`);
    expect(marked).toContain(">cell</td>");
  });
});

describe("injectAgentScript", () => {
  it("runs the agent at the front of <head>, before the artifact's own scripts", () => {
    const out = injectAgentScript(
      "<html><head></head><body><script>run()</script></body></html>",
      "function agent(){}",
    );
    expect(out.indexOf("function agent(){}")).toBeLessThan(out.indexOf("run()"));
    expect(out).toContain("<head><script>");
  });

  it("neutralizes a </script> hidden in the agent source", () => {
    const out = injectAgentScript("<head></head>", "var s = '</script>';");
    expect(out).not.toContain("';</script>';");
    expect(out).toContain("<\\/script");
  });

  it("falls back to prepending when there is no head or html tag", () => {
    const out = injectAgentScript("<p>bare</p>", "function a(){}");
    expect(out.startsWith("<script>")).toBe(true);
  });
});

describe("injectEditorStyle", () => {
  it("adds a style at the front of <head> keyed on the marker attribute", () => {
    const out = injectEditorStyle("<html><head></head><body></body></html>");
    expect(out).toContain("<head><style>");
    expect(out).toContain(`[${MARKER_ATTR}]`);
  });
});
