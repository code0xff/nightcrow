import { MARKER_ATTR } from "../markers";
import type { AgentState } from "./types";

const CSS_COMMANDS = new Set(["foreColor", "fontSize", "backColor", "hiliteColor"]);

export function createFormatting(state: AgentState, placeBar: () => void) {
  const format = (command: string, value?: string): void => {
    if (state.editingId === null) return;
    const el = state.editingEl;
    if (!el) return;

    const restore = state.saved;
    if (restore) {
      const sel = getSelection();
      sel?.removeAllRanges();
      sel?.addRange(restore);
    }
    el.focus();
    // execCommand affects every selected block, while a patch can name only this one.
    const range = getSelection()?.rangeCount ? getSelection()?.getRangeAt(0) : null;
    if (!range || !el.contains(range.startContainer) || !el.contains(range.endContainer)) return;
    document.execCommand("styleWithCSS", false, String(CSS_COMMANDS.has(command)));
    document.execCommand(command, false, value);
    state.saved = getSelection()?.rangeCount ? (getSelection()?.getRangeAt(0) ?? null) : null;
    placeBar();
  };

  const isBig = (node: HTMLElement): boolean =>
    node.style.fontSize === "xxx-large" || node.style.fontSize === "-webkit-xxx-large";

  const coveredBy = (range: Range, node: HTMLElement): boolean => {
    const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
    let found = false;
    for (let text = walker.nextNode(); text; text = walker.nextNode()) {
      if ((text.nodeValue ?? "").length === 0) continue;
      found = true;
      const textRange = document.createRange();
      textRange.selectNodeContents(text);
      if (
        range.compareBoundaryPoints(Range.START_TO_START, textRange) > 0 ||
        range.compareBoundaryPoints(Range.END_TO_END, textRange) < 0
      ) {
        return false;
      }
    }
    return found;
  };

  const bigHostOf = (range: Range, el: HTMLElement): HTMLElement | null => {
    let node: Node | null = range.commonAncestorContainer;
    while (node && node !== el) {
      if (node instanceof HTMLElement && isBig(node) && !node.hasAttribute(MARKER_ATTR)) return node;
      node = node.parentNode;
    }
    return null;
  };

  const splitResize = (host: HTMLElement, range: Range, times: string): void => {
    const picked = range.extractContents();
    const tail = document.createRange();
    tail.selectNodeContents(host);
    tail.setStart(range.startContainer, range.startOffset);
    const rest = tail.extractContents();

    const shell = (fragment: DocumentFragment): HTMLElement => {
      const element = host.cloneNode(false) as HTMLElement;
      // Cloned shells must not duplicate a document id.
      element.removeAttribute("id");
      element.appendChild(fragment);
      return element;
    };
    const mid = shell(picked);
    mid.style.fontSize = times;
    host.parentNode?.insertBefore(mid, host.nextSibling);
    const hollow = (node: Node): boolean => {
      for (let child = node.firstChild; child; child = child.nextSibling) {
        if (child.nodeType !== Node.TEXT_NODE || (child.nodeValue ?? "").length > 0) return false;
      }
      return true;
    };
    if (!hollow(rest)) mid.parentNode?.insertBefore(shell(rest), mid.nextSibling);
    if (hollow(host)) host.remove();

    const selection = getSelection();
    const result = document.createRange();
    result.selectNodeContents(mid);
    selection?.removeAllRanges();
    selection?.addRange(result);
    placeBar();
  };

  const commitLater = (): void => {
    state.saved = getSelection()?.rangeCount ? (getSelection()?.getRangeAt(0) ?? null) : null;
  };

  const resize = (times: string): void => {
    if (state.editingId === null) return;
    const el = state.editingEl;
    if (!el) return;
    format("fontSize", "7");

    const selection = getSelection();
    const range = selection && selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
    if (
      !range ||
      range.collapsed ||
      !el.contains(range.startContainer) ||
      !el.contains(range.endContainer)
    ) {
      return;
    }

    let touched = false;
    for (const node of el.querySelectorAll<HTMLElement>('[style*="font-size"]')) {
      if (isBig(node) && coveredBy(range, node)) {
        node.style.fontSize = times;
        touched = true;
      }
    }
    if (!touched) {
      const host = bigHostOf(range, el);
      if (host) splitResize(host, range, times);
    }
    commitLater();
  };

  return { format, resize };
}
