// @vitest-environment happy-dom

import { afterEach, describe, expect, it } from "vitest";
import agentSource from "virtual:nightcrow-preview-agent";
import { agentScriptTag } from "./markers";
import { buildPreviewDocument } from "./preview";
import { parseBlocks } from "./parse";

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const FRAME_HTML = '<!doctype html><html><head></head><body><p data-ne-id="0">Hello</p></body></html>';
const frames: HTMLIFrameElement[] = [];
const messages: Record<string, unknown>[] = [];
const receive = (event: MessageEvent) => {
  if (event.data && typeof event.data === "object") messages.push(event.data as Record<string, unknown>);
};

function makeFrame(): { frame: HTMLIFrameElement; view: Window & typeof globalThis } {
  const frame = document.createElement("iframe");
  frame.srcdoc = FRAME_HTML;
  document.body.append(frame);
  frames.push(frame);
  const view = frame.contentWindow?.window;
  if (!view) throw new Error("iframe has no window");
  expect(view).not.toBe(view.parent);
  expect(view.eval("document")).toBe(view.document);
  return { frame, view };
}

afterEach(() => {
  for (const frame of frames.splice(0)) frame.remove();
  messages.length = 0;
  window.removeEventListener("message", receive);
});

describe("bundled preview agent contract", () => {
  it("runs in an iframe, guards host messages, carries its token, edits, and detaches", async () => {
    window.addEventListener("message", receive);
    const { view } = makeFrame();
    const token = "frame-token";
    const detach = view.eval(`(${agentSource})(${JSON.stringify(token)})`) as () => void;
    expect(typeof detach).toBe("function");
    view.eval(
      'document.documentElement.dataset.artifactClicks = "0"; ' +
        'document.addEventListener("click", () => { ' +
        'document.documentElement.dataset.artifactClicks = String(Number(document.documentElement.dataset.artifactClicks) + 1); });',
    );

    const { frame: foreignFrame } = makeFrame();
    const foreign = foreignFrame.contentWindow;
    if (!foreign) throw new Error("foreign iframe has no window");
    expect(foreign).not.toBe(view.parent);
    view.dispatchEvent(
      new view.MessageEvent("message", {
        data: { type: "locked", ids: [], all: [0] },
        source: foreign,
      }),
    );
    view.document.querySelector("p")!.dispatchEvent(new view.MouseEvent("click", { bubbles: true }));
    await settle();
    expect(messages.some((message) => message.type === "notReady")).toBe(true);
    expect(view.document.documentElement.dataset.artifactClicks).toBe("0");

    view.dispatchEvent(
      new view.MessageEvent("message", {
        data: { type: "locked", ids: [], all: [0] },
        source: view.parent,
      }),
    );
    const block = view.document.querySelector("p")!;
    block.dispatchEvent(new view.MouseEvent("click", { bubbles: true }));
    expect(block.getAttribute("contenteditable")).toBe("true");
    block.textContent = "Changed";
    block.dispatchEvent(new view.FocusEvent("focusout", { bubbles: true }));
    await settle();

    const edit = messages.find((message) => message.type === "edit");
    expect(edit).toMatchObject({ token, id: 0, html: "Changed", pristine: false });
    expect(view.document.documentElement.dataset.artifactClicks).toBe("0");

    detach();
    block.dispatchEvent(new view.MouseEvent("click", { bubbles: true }));
    expect(view.document.documentElement.dataset.artifactClicks).toBe("1");
  });

  it("escapes hostile tokens and injects before artifact scripts", () => {
    const source =
      '<html><head></head><body><p>Hello</p><script>window.artifactRan = true;</script></body></html>';
    const blocks = parseBlocks(source);
    const token = '</script><script>window.pwned = true</script>';
    const html = buildPreviewDocument(source, blocks, token);
    const agentCall = html.indexOf("AgentBundle.previewAgent(token)");
    expect(agentCall).toBeGreaterThan(-1);
    expect(agentCall).toBeLessThan(html.indexOf("window.artifactRan = true"));
    expect(html).toContain("\\u003c/script>");
    expect(html).not.toContain(token);

    const tag = agentScriptTag(agentSource, token);
    expect(tag).toContain("\\u003c/script>");
    expect(tag).not.toContain(token);
  });

  it("cancels the delayed scan when the iframe agent detaches", async () => {
    window.addEventListener("message", receive);
    const { view } = makeFrame();
    const detach = view.eval(`(${agentSource})("short-lived")`) as () => void;
    detach();
    await settle();
    expect(messages.some((message) => message.type === "ready")).toBe(false);
  });
});
