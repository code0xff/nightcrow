import { parse, type DefaultTreeAdapterTypes } from "parse5";
import type { Block, LockReason } from "./types";
import { normalizeText } from "./entities";
import { hasCodeBlockClass, isInline, RAW_TEXT_TAGS, RCDATA_TAGS } from "./blocks";

type Node = DefaultTreeAdapterTypes.Node;
type Element = DefaultTreeAdapterTypes.Element;
type ParentNode = DefaultTreeAdapterTypes.ParentNode;

function isElement(node: Node): node is Element {
  return "tagName" in node;
}

function childrenOf(node: Node): Node[] {
  return "childNodes" in node ? (node as ParentNode).childNodes : [];
}

function attr(el: Element, name: string): string | undefined {
  return el.attrs.find((a) => a.name === name)?.value;
}

/** Text the node holds directly — text in inline descendants does not count. */
function hasDirectText(node: Node): boolean {
  return childrenOf(node).some(
    (c) => c.nodeName === "#text" && "value" in c && c.value.trim().length > 0,
  );
}

/**
 * Collected descendant text. Content inside RAW_TEXT does not count as text.
 *
 * parse5 hands over `#text` nodes with character references already decoded.
 * Decoding again here resolves `&amp;amp;` all the way to `&`, diverging from
 * the live textContent, and the comparison then false-positive-locks a healthy
 * block.
 */
function textOf(node: Node): string {
  if (node.nodeName === "#text") {
    return "value" in node ? node.value : "";
  }
  if (isElement(node) && RAW_TEXT_TAGS.has(node.tagName)) return "";
  return childrenOf(node).map(textOf).join("");
}

function ownLockReason(el: Element): LockReason | null {
  return hasCodeBlockClass(attr(el, "class")) ? "CODE_BLOCK" : null;
}

/**
 * Extracts editing blocks from the source HTML string.
 *
 * Every returned offset is relative to `source`, and
 * `source.slice(innerStart, innerEnd)` always matches that block's original
 * innerHTML exactly.
 *
 * Known limitation — in an element mixing direct text with block children
 * (`<div>text<p>para</p></div>`), that direct text never becomes editable. The
 * parent cannot be a block because of its block children, and there is no
 * boundary that could carve out the text on its own.
 */
export function parseBlocks(source: string): Block[] {
  const doc = parse(source, { sourceCodeLocationInfo: true });
  const blocks: Block[] = [];
  let nextId = 0;

  const visit = (node: Node, inheritedLock: LockReason | null): void => {
    if (isElement(node) && RAW_TEXT_TAGS.has(node.tagName)) return;

    const elementChildren = childrenOf(node).filter(isElement);
    // Locks are inherited by descendants. Nested elements inside .code must not
    // become editable.
    const lock = isElement(node)
      ? (inheritedLock ?? ownLockReason(node))
      : inheritedLock;

    if (isElement(node)) {
      const loc = node.sourceCodeLocation;
      const startTag = loc?.startTag;
      const endTag = loc?.endTag;
      const hasBlockChild = elementChildren.some((c) => !isInline(c.tagName));

      // parse5 inserts nodes that are not in the source (a table's <tbody> etc.).
      // Without location info a node cannot be a block. Pass through and descend
      // into children.
      if (startTag && !hasBlockChild && textOf(node).trim().length > 0) {
        const innerStart = startTag.endOffset;
        // With the closing tag omitted (`<li>a<li>b`), the inner range cannot be
        // pinned down. Instead of dropping it silently, lock it as AMBIGUOUS and
        // keep the reason.
        const innerEnd = endTag ? endTag.startOffset : (loc?.endOffset ?? innerStart);
        blocks.push({
          id: nextId++,
          tag: node.tagName,
          innerStart,
          innerEnd,
          sourceInner: source.slice(innerStart, innerEnd),
          sourceText: normalizeText(textOf(node)),
          rcdata: RCDATA_TAGS.has(node.tagName),
          locked: endTag ? lock : "AMBIGUOUS",
        });
        return;
      }

      // A leaf element that is empty in the source. When a script fills it,
      // characters show on screen but exist nowhere in the source, so they
      // cannot be traced back. Dropping it for having no text means clicks do
      // nothing with no way to learn why — so capture it and lock it.
      //
      // Requiring a closing tag filters out void elements (`<br>`, `<img>`) — no
      // content can go inside them. An element with element children is a
      // container, so instead of stopping here we descend into it.
      if (startTag && endTag && elementChildren.length === 0) {
        const innerStart = startTag.endOffset;
        const innerEnd = endTag.startOffset;
        const sourceInner = source.slice(innerStart, innerEnd);
        // An element holding only a comment is not empty — erasing it destroys
        // something the user wrote.
        if (sourceInner.trim().length === 0) {
          // RCDATA like <title> is the exception — it does not render in the
          // preview, is edited through a separate input, and its value comes
          // from the source, not the live DOM. Locking it here leaves a document
          // with an empty title no way to get one. A script-filled title is
          // locked separately by the post-render comparison.
          const emptyLock = RCDATA_TAGS.has(node.tagName) ? null : "EMPTY_IN_SOURCE";
          blocks.push({
            id: nextId++,
            tag: node.tagName,
            innerStart,
            innerEnd,
            sourceInner,
            sourceText: "",
            rcdata: RCDATA_TAGS.has(node.tagName),
            locked: lock ?? emptyLock,
          });
          return;
        }
      }
    }

    // Decide whether to descend into inline children.
    //
    // If the parent has direct text, the inline is part of that sentence.
    // Promoting it would split one sentence, so do not descend. If the parent
    // has no direct text, the inline is a standalone label
    // (`<div><span class="codelabel">title</span><ul>…</ul></div>`); not
    // descending there makes perfectly visible text uneditable.
    const descendIntoInline = !hasDirectText(node);
    for (const child of elementChildren) {
      if (!isInline(child.tagName) || descendIntoInline) visit(child, lock);
    }
  };

  visit(doc, null);
  return blocks;
}
