/**
 * The editing agent that runs inside the preview iframe, ported from
 * nighteditor. It must be a self-contained function: no imports, no references
 * to outer scope. The host stringifies it with `previewAgent.toString()` and
 * injects it at the front of the document, so constants like `data-ne-id` are
 * written out again here and must match `markers.ts`. The function is kept as
 * one self-contained unit because splitting it would break that contract.
 *
 * @param token This preview document's token. The iframe is reused, so
 *   `contentWindow` identity cannot tell documents apart — every message carries
 *   this token so the host can drop messages from an old preview.
 * @returns A function that removes every listener the agent attached, called
 *   when another file is opened.
 */
export function previewAgent(token = ""): () => void {
  const MARKER = "data-ne-id";
  const LOCKED = "data-ne-locked";
  const DARK = "data-ne-dark";
  const REVEALED = "data-ne-revealed";
  const BAR = "data-ne-bar";

  /** Cap the color swatches here. Any more and picking becomes work of its own. */
  const MAX_COLORS = 10;

  // Must be removable. A previous agent lingering when another file opens would
  // intercept events with stale state and disrupt editing in the new document.
  const bound: { target: EventTarget; type: string; fn: EventListener }[] = [];
  const on = (target: EventTarget, type: string, fn: EventListener): void => {
    target.addEventListener(type, fn);
    bound.push({ target, type, fn });
  };
  const post = (msg: Record<string, unknown>): void => {
    // Attach the document's token to every message. Calling without a token
    // happens only where none is needed (the test harness).
    parent.postMessage(token ? { ...msg, token } : msg, "*");
  };

  let editingId: number | null = null;
  /**
   * The element being edited. Commit and restore never look it up again by id —
   * if the document (or an artifact script) mimics a `data-ne-id` with the same
   * id, querySelector returns the impostor earlier in the document, and the
   * impostor's content gets saved as this block's edit.
   */
  let editingEl: HTMLElement | null = null;
  /** innerHTML at the moment editing opened. For Escape restore and the pristine check. */
  let snapshot: string | null = null;
  let composing = false;
  /** When to clear the reveal highlight. Picking again cancels the previous one. */
  let revealTimer: ReturnType<typeof setTimeout> | null = null;
  let revealed: HTMLElement | null = null;
  /** The formatting bar, and the selection range kept alive while pressing it. */
  let bar: HTMLElement | null = null;
  let saved: Range | null = null;
  /**
   * Whether the bar is being pressed. A selection that collapses during the
   * press is not the user leaving, so keep `saved`; if it collapses anywhere
   * else, drop `saved` too — keeping it would make the next Ctrl+B land on the
   * old text instead of the current pick.
   */
  let barHeld = false;
  /** Whether a commit was deferred because composition is in progress. */
  let pendingCommit = false;
  /**
   * Sequence numbers of flush requests that arrived while a commit was deferred
   * and were deferred along with it. Replying before the deferred commit would
   * make the host assume it is current and swap the document, losing the
   * characters being composed.
   */
  const pendingFlush: number[] = [];
  /** Send the deferred flush replies now — the deferred commit went out or was dropped. */
  const answerFlushes = (): void => {
    for (const seq of pendingFlush.splice(0)) post({ type: "flushed", seq });
  };
  const locked = new Set<number>();
  /**
   * Every real block id the host announced. The document can mimic `data-ne-id`,
   * so a marker not on this roster does not count as a block. null means the
   * roster has not arrived yet.
   */
  let known: Set<number> | null = null;
  /**
   * id → the elements captured at verification. The roster screens numbers only,
   * so a script that inserts another element with the same number after
   * verification passes the number check — an element that is not the one
   * remembered here does not count as a block even when the number matches. When
   * several elements were scanned under one id, keep all of them.
   */
  let verifiedEl: Map<number, HTMLElement[]> | null = null;
  /**
   * Whether verification finished and the lock list arrived. No edit opens
   * before that — an edit opened earlier is erased from the save the moment
   * verification locks that block.
   */
  let verified = false;

  /**
   * Record the document's real markers, id → elements. Duplicate ids are not
   * discarded — the host locks the clash as MARKER_CLASH, but the lock notice
   * and click blocking must reach every clashing element.
   */
  const snapshotMarkers = (): Map<number, HTMLElement[]> => {
    const map = new Map<number, HTMLElement[]>();
    for (const el of document.querySelectorAll<HTMLElement>("[" + MARKER + "]")) {
      if (mimicked(el)) continue;
      const id = Number(el.getAttribute(MARKER));
      const seen = map.get(id);
      if (seen) seen.push(el);
      else map.set(id, [el]);
    }
    return map;
  };

  /**
   * Never re-query the document by id — if a mimic sits earlier in the document,
   * querySelector returns the impostor first, and restore/reveal touch it.
   */
  const elementFor = (id: number): HTMLElement | null =>
    verifiedEl
      ? (verifiedEl.get(id)?.[0] ?? null)
      : document.querySelector<HTMLElement>("[" + MARKER + '="' + id + '"]');

  /** Clear the reveal highlight. */
  const clearReveal = (): void => {
    if (revealTimer !== null) clearTimeout(revealTimer);
    revealTimer = null;
    revealed?.removeAttribute(REVEALED);
    revealed = null;
  };

  const mimicked = (el: Element): boolean =>
    !!el.parentElement?.closest("[" + MARKER + "]");

  /**
   * Walk up from the event target to find an ancestor carrying a marker.
   *
   * Markers not on the roster and markers nested inside other markers are
   * mimics — keep climbing past them so the click flows to the real block
   * outside, or to the document. Before the roster arrives, treat any marker as
   * a block and route it to the "not ready yet" notice.
   */
  const blockOf = (target: EventTarget | null): HTMLElement | null => {
    let el = target instanceof Element ? target : null;
    while (el) {
      if (el.hasAttribute(MARKER) && !mimicked(el)) {
        const id = Number(el.getAttribute(MARKER));
        // The number alone is not enough — an element inserted after
        // verification with the same number passes the roster check. Only the
        // element captured at verification is a block. Elements scanned under a
        // clashing id all count, so whichever is clicked goes to the lock notice.
        if (
          (verifiedEl === null || (verifiedEl.get(id)?.includes(el as HTMLElement) ?? false)) &&
          (known === null || known.has(id))
        ) {
          return el as HTMLElement;
        }
      }
      el = el.parentElement;
    }
    return null;
  };

  const idOf = (el: HTMLElement): number => Number(el.getAttribute(MARKER));

  /** Commit the edit and send the result to the host. */
  const commit = (): void => {
    if (editingId === null) return;
    // Never commit mid-composition. Defer it and finish in compositionend.
    if (composing) {
      pendingCommit = true;
      return;
    }
    // Never re-find by id — a mimic earlier in the document would be caught first.
    const el = editingEl;
    if (el) {
      el.removeAttribute("contenteditable");
      el.removeAttribute("enterkeyhint");
      // Last line of defense: an artifact script may have moved the bar inside
      // the block. Preview furniture must not leak into the save via innerHTML,
      // so move it back outside before reading.
      if (bar && el.contains(bar)) document.documentElement.appendChild(bar);
      // Compare against what the browser serialized. Comparing against the
      // source string would patch untouched blocks over normalization
      // differences like <br/> → <br>.
      post({
        type: "edit",
        id: editingId,
        html: el.innerHTML,
        pristine: el.innerHTML === snapshot,
      });
    }
    editingId = null;
    editingEl = null;
    snapshot = null;
    pendingCommit = false;
    hideBar();
    post({ type: "select", id: null });
    // Deferred flush replies can go now — the commit went out first above.
    answerFlushes();
  };

  /** Drop the edit and restore the content from before it opened. */
  const cancel = (): void => {
    if (editingId === null) return;
    const el = editingEl;
    if (el) {
      el.removeAttribute("contenteditable");
      el.removeAttribute("enterkeyhint");
      // Restoring innerHTML with the bar inside would detach it while the
      // reference survives, so the next selection neither rebuilds nor
      // re-attaches it. Spirit it out of the block before restoring.
      if (bar && el.contains(bar)) document.documentElement.appendChild(bar);
      if (snapshot !== null) el.innerHTML = snapshot;
    }
    editingId = null;
    editingEl = null;
    snapshot = null;
    pendingCommit = false;
    hideBar();
    post({ type: "select", id: null });
    // A dropped edit has no commit to send — no reason to hold the flush replies.
    answerFlushes();
  };

  const startEdit = (el: HTMLElement): void => {
    const id = idOf(el);
    // A click before verification finishes opens nothing; only report it.
    if (!verified) {
      post({ type: "notReady" });
      return;
    }
    if (locked.has(id)) {
      post({ type: "blocked", id });
      return;
    }
    if (editingId === id) return;
    commit();
    // If composition deferred the commit, the previous edit is still open.
    if (editingId !== null) return;
    editingId = id;
    editingEl = el;
    snapshot = el.innerHTML;
    el.setAttribute("contenteditable", "true");
    // Name the return key on a virtual keyboard. It commits and closes the
    // block, so a key labelled "new line" would promise the one thing it does
    // not do. Sits on the element, never on the innerHTML the commit reads.
    el.setAttribute("enterkeyhint", "done");
    el.focus();
    post({ type: "select", id });
  };


  /**
   * Apply a formatting command.
   *
   * Set `styleWithCSS` per command. Off: bold/italic/underline come out as
   * `<b>`/`<i>`/`<u>`, better for a human-readable diff. On: color and size come
   * out as `<span style>` rather than `<font>`, which would make the paragraph
   * uneditable next time.
   */
  const CSS_COMMANDS = new Set(["foreColor", "fontSize", "backColor", "hiliteColor"]);

  const format = (command: string, value?: string): void => {
    if (editingId === null) return;
    const el = editingEl;
    if (!el) return;

    // The selection may have collapsed while the bar was pressed. Revive the
    // held range. Move it to a local first — if removeAllRanges fires
    // selectionchange synchronously, placeBar clears saved inside it.
    const restore = saved;
    if (restore) {
      const sel = getSelection();
      sel?.removeAllRanges();
      sel?.addRange(restore);
    }
    el.focus();
    // Do not apply when the selection reaches beyond this block into a neighbor.
    // execCommand changes the neighbor too, but the commit message covers only
    // this block, so the neighbor's change would go untracked.
    const range = getSelection()?.rangeCount ? getSelection()?.getRangeAt(0) : null;
    if (!range || !el.contains(range.startContainer) || !el.contains(range.endContainer)) return;
    document.execCommand("styleWithCSS", false, String(CSS_COMMANDS.has(command)));
    document.execCommand(command, false, value);
    saved = getSelection()?.rangeCount ? (getSelection()?.getRangeAt(0) ?? null) : null;
    placeBar();
  };

  /** Is this the keyword value the `fontSize` command makes (7 = xxx-large). */
  const isBig = (node: HTMLElement): boolean =>
    node.style.fontSize === "xxx-large" || node.style.fontSize === "-webkit-xxx-large";

  /**
   * Does the range cover every non-empty character of the element? Measured by
   * character positions, not element boundaries — by boundary points, (span,0)
   * and (first char,0) look like the same spot yet compare differently.
   */
  const coveredBy = (range: Range, node: HTMLElement): boolean => {
    const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
    let found = false;
    for (let t = walker.nextNode(); t; t = walker.nextNode()) {
      if ((t.nodeValue ?? "").length === 0) continue;
      found = true;
      const tr = document.createRange();
      tr.selectNodeContents(t);
      if (
        range.compareBoundaryPoints(Range.START_TO_START, tr) > 0 ||
        range.compareBoundaryPoints(Range.END_TO_END, tr) < 0
      ) {
        return false;
      }
    }
    return found;
  };

  /** An ancestor containing the range that already carries the value. */
  const bigHostOf = (range: Range, el: HTMLElement): HTMLElement | null => {
    let node: Node | null = range.commonAncestorContainer;
    while (node && node !== el) {
      if (node instanceof HTMLElement && isBig(node) && !node.hasAttribute(MARKER)) return node;
      node = node.parentNode;
    }
    return null;
  };

  /**
   * When only part of an already-sized run is selected, the command makes
   * nothing. Split the host at the selected range and write the new multiplier
   * on just the selected part; the unselected parts keep the original value.
   */
  const splitResize = (host: HTMLElement, range: Range, times: string): void => {
    const picked = range.extractContents();
    const tail = document.createRange();
    tail.selectNodeContents(host);
    tail.setStart(range.startContainer, range.startOffset);
    const rest = tail.extractContents();

    // The look rides in shells cloned from host. The id is not inherited — the
    // document would end up with two of the same id.
    const shell = (frag: DocumentFragment): HTMLElement => {
      const s = host.cloneNode(false) as HTMLElement;
      s.removeAttribute("id");
      s.appendChild(frag);
      return s;
    };
    const mid = shell(picked);
    mid.style.fontSize = times;
    host.parentNode?.insertBefore(mid, host.nextSibling);
    // A hollow fragment holds nothing but empty text nodes; a comment the user
    // never selected must not vanish, so judge by node type, not textContent.
    const hollow = (node: Node): boolean => {
      for (let child = node.firstChild; child; child = child.nextSibling) {
        if (child.nodeType !== Node.TEXT_NODE || (child.nodeValue ?? "").length > 0) return false;
      }
      return true;
    };
    if (!hollow(rest)) mid.parentNode?.insertBefore(shell(rest), mid.nextSibling);
    if (hollow(host)) host.remove();

    const sel = getSelection();
    const r = document.createRange();
    r.selectNodeContents(mid);
    sel?.removeAllRanges();
    sel?.addRange(r);
    placeBar();
  };

  /**
   * Sizes are written as multipliers. `fontSize` makes an absolute keyword like
   * `x-large`; artifacts differ in base size, so after the command runs, rewrite
   * the value as a multiple of the original. What to rewrite is chosen by the
   * selected range, not by value.
   */
  const resize = (times: string): void => {
    if (editingId === null) return;
    const el = editingEl;
    if (!el) return;

    format("fontSize", "7");

    const sel = getSelection();
    const range = sel && sel.rangeCount > 0 ? sel.getRangeAt(0) : null;
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

  /** Formatting is a command, not typing, so the input event comes late. */
  const commitLater = (): void => {
    saved = getSelection()?.rangeCount ? (getSelection()?.getRangeAt(0) ?? null) : null;
  };

  /**
   * Collect the colors this document uses for text, most used first. Handing out
   * colors of our own would override the document's system; the usable colors
   * are already inside it. Count only elements that carry text, and fold colors
   * that look the same into one.
   */
  const paletteOf = (): string[] => {
    const used = new Map<string, number>();
    const body = document.body;
    const scope: HTMLElement[] = body ? [body, ...body.querySelectorAll<HTMLElement>("*")] : [];
    for (const el of scope) {
      const text = [...el.childNodes].some(
        (node) => node.nodeType === 3 && (node.nodeValue ?? "").trim().length > 0,
      );
      if (!text) continue;
      const color = getComputedStyle(el).color;
      if (/^rgba?\(/.test(color)) used.set(color, (used.get(color) ?? 0) + 1);
    }

    const palette: string[] = [];
    for (const [color] of [...used.entries()].sort((a, b) => b[1] - a[1])) {
      if (palette.length >= MAX_COLORS) break;
      if (!palette.some((kept) => alike(kept, color))) palette.push(color);
    }
    return palette;
  };

  /**
   * Do two colors look the same? Measured with the common weighted distance
   * (the eye is most sensitive to green, least to blue). Alpha is ignored.
   */
  const alike = (a: string, b: string): boolean => {
    const one = colorOf(a);
    const two = colorOf(b);
    if (!one || !two) return a === b;
    const [r1, g1, b1] = one;
    const [r2, g2, b2] = two;
    const distance = Math.sqrt(2 * (r1 - r2) ** 2 + 4 * (g1 - g2) ** 2 + 3 * (b1 - b2) ** 2);
    return distance < 24;
  };

  /** Labels for the bar. The host hands them over; until they arrive, empty. */
  let labels: Record<string, string> = {};

  const button = (label: string, name: string, run: () => void): HTMLElement => {
    const el = document.createElement("button");
    el.type = "button";
    el.textContent = label;
    el.setAttribute("data-ne-label", name);
    el.title = labels[name] ?? "";
    const touch = matchMedia("(pointer: coarse)").matches;
    el.style.cssText =
      `all:unset;cursor:pointer;padding:${touch ? "6px 10px" : "2px 6px"};` +
      "border-radius:4px;font:600 12px/1.4 system-ui;";
    // Focus moving at the moment of the press would collapse the selection.
    el.addEventListener("mousedown", (e) => e.preventDefault());
    el.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      run();
    });
    return el;
  };

  const buildBar = (): HTMLElement => {
    const box = document.createElement("div");
    box.setAttribute(BAR, "");
    box.addEventListener("mousedown", () => {
      barHeld = true;
    });
    box.addEventListener("touchstart", () => {
      barHeld = true;
    });
    box.style.cssText =
      "position:absolute;z-index:2147483647;display:none;gap:2px;align-items:center;" +
      "flex-wrap:wrap;max-width:calc(100vw - 16px);box-sizing:border-box;" +
      "padding:4px;border-radius:8px;background:#101014;color:#e9e9ec;" +
      "box-shadow:0 6px 20px rgba(0,0,0,.35);font:12px system-ui;";

    box.append(
      button("B", "format.bold", () => format("bold")),
      button("I", "format.italic", () => format("italic")),
      button("U", "format.underline", () => format("underline")),
      button("A-", "format.smaller", () => resize("0.85em")),
      button("A+", "format.bigger", () => resize("1.35em")),
    );

    for (const color of paletteOf()) {
      const dot = button(" ", "format.color", () => format("foreColor", color));
      const size = matchMedia("(pointer: coarse)").matches ? 20 : 12;
      dot.style.cssText +=
        `display:block;padding:0;width:${size}px;height:${size}px;border-radius:50%;` +
        `background:${color};box-shadow:inset 0 0 0 1px rgba(255,255,255,.35);`;
      box.append(dot);
    }
    box.append(button("✕", "format.clear", () => format("removeFormat")));
    return box;
  };

  const hideBar = (): void => {
    saved = null;
    if (bar) bar.style.display = "none";
  };

  /** Place the bar over the selected text. Hide it when nothing is selected. */
  const placeBar = (): void => {
    const sel = getSelection();
    const el = editingEl;
    const inside =
      el &&
      sel &&
      sel.rangeCount > 0 &&
      !sel.isCollapsed &&
      el.contains(sel.anchorNode) &&
      el.contains(sel.focusNode);
    if (!inside) {
      if (!barHeld) saved = null;
      if (bar) bar.style.display = "none";
      return;
    }

    if (!bar) {
      bar = buildBar();
      // Keep it outside the block. In a document where <body> itself is a block,
      // attaching to body would carry the buttons into the save. <html> holds
      // head and body, so it can never be a block.
      document.documentElement.appendChild(bar);
    }
    saved = sel.getRangeAt(0).cloneRange();
    const rect = sel.getRangeAt(0).getBoundingClientRect();
    bar.style.display = "flex";
    const top = rect.top + scrollY - bar.offsetHeight - 8;
    bar.style.top = `${Math.max(scrollY + 4, top)}px`;
    const room = document.documentElement.clientWidth - bar.offsetWidth - 8;
    bar.style.left = `${scrollX + Math.max(4, Math.min(rect.left, room))}px`;
  };

  on(document, "selectionchange", () => placeBar());
  on(document, "mouseup", () => {
    barHeld = false;
  });

  // Block at the bubble phase. This script runs at the front of the document,
  // so it registers before the artifact and runs first.

  const consume = (e: Event): void => {
    e.preventDefault();
    e.stopImmediatePropagation();
  };

  on(document, "click", ((e: MouseEvent) => {
    // The formatting bar is ours. Compare against the object we built, not the
    // attribute — a mimicked data-ne-bar must not swallow clicks on blocks.
    if (bar && e.target instanceof Node && bar.contains(e.target)) {
      e.stopImmediatePropagation();
      return;
    }
    const el = blockOf(e.target);
    if (el) {
      startEdit(el);
      // Keep it from reaching the artifact's global click handlers.
      consume(e);
      return;
    }
    // A click outside any block. If editing was open, consume it as "end
    // editing"; if not, let it through so artifact navigation still works.
    const wasEditing = editingId !== null;
    commit();
    if (wasEditing) consume(e);
  }) as EventListener);

  on(document, "keydown", ((e: KeyboardEvent) => {
    // Ctrl/⌘+S — block "save page" and ask the host to save.
    if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === "s" || e.key === "S")) {
      consume(e);
      if (composing || e.isComposing) return;
      commit();
      post({ type: "save" });
      return;
    }
    // Ctrl/⌘+Z outside editing — undo the last change for the host.
    if (
      (e.ctrlKey || e.metaKey) &&
      !e.altKey &&
      !e.shiftKey &&
      (e.key === "z" || e.key === "Z") &&
      editingId === null
    ) {
      consume(e);
      post({ type: "undo" });
      return;
    }
    if (editingId === null) return;
    // Ctrl/⌘+B · I · U — bold, italic, underline.
    if ((e.ctrlKey || e.metaKey) && !e.altKey) {
      const key = e.key.toLowerCase();
      const command =
        key === "b" ? "bold" : key === "i" ? "italic" : key === "u" ? "underline" : "";
      if (command) {
        consume(e);
        format(command);
        return;
      }
    }
    // Enter commits and closes. A block is one unit; line breaks are Shift+Enter.
    if (e.key === "Enter" && !e.shiftKey && !composing && !e.isComposing) {
      commit();
      consume(e);
      return;
    }
    if (e.key === "Escape") cancel();
    // While editing, keep arrow keys and space from leaking into artifact nav.
    e.stopImmediatePropagation();
  }) as EventListener);

  // Block the line break a mid-composition Enter would leave behind, and make
  // the input stage the one reliable commit path on a phone (virtual keyboards
  // do not reliably raise a usable keydown).
  on(document, "beforeinput", ((e: InputEvent) => {
    if (editingId === null) return;
    if (e.inputType !== "insertParagraph") return;
    e.preventDefault();
    if (!composing && !e.isComposing) commit();
  }) as EventListener);

  on(document, "touchend", (e) => {
    const target = (e as TouchEvent).target;
    if (!(bar && target instanceof Node && bar.contains(target))) barHeld = false;
    if (editingId !== null) e.stopImmediatePropagation();
  });

  on(document, "compositionstart", () => {
    composing = true;
  });
  on(document, "compositionend", () => {
    composing = false;
    if (pendingCommit) commit();
  });

  on(document, "focusout", (e) => {
    const to = (e as FocusEvent).relatedTarget;
    if (bar && to instanceof Node && bar.contains(to)) return;
    commit();
  });

  // Paste drops formatting and inserts plain text only.
  on(document, "paste", ((e: ClipboardEvent) => {
    if (editingId === null) return;
    e.preventDefault();
    const text = e.clipboardData?.getData("text/plain") ?? "";
    document.execCommand("insertText", false, text);
  }) as EventListener);

  on(window, "message", ((e: MessageEvent) => {
    // Accept only what the host sent.
    if (e.source !== parent) return;
    const msg = e.data as {
      type?: string;
      ids?: number[];
      all?: number[];
      id?: number;
      html?: string;
      labels?: Record<string, string>;
      seq?: number;
    } | null;
    if (!msg || typeof msg !== "object") return;
    if (msg.type === "locked" && msg.ids) {
      // The lock list only comes after verification ends — editing is accepted
      // from this signal on.
      verified = true;
      locked.clear();
      for (const id of msg.ids) locked.add(id);
      if (msg.all) known = new Set(msg.all);
      if (verifiedEl === null) verifiedEl = snapshotMarkers();
      // Mirror the lock marks onto the DOM; the injected style watches them.
      // Paint only where text is visible, and only the elements captured at
      // verification, so painting never diverges from the roster.
      for (const [id, els] of verifiedEl) {
        for (const el of els) {
          const show = locked.has(id) && (el.textContent ?? "").trim();
          if (show) el.setAttribute(LOCKED, "");
          else el.removeAttribute(LOCKED);
        }
      }
    } else if (msg.type === "labels" && msg.labels) {
      labels = msg.labels;
      for (const el of bar?.querySelectorAll("[data-ne-label]") ?? []) {
        el.setAttribute("title", labels[el.getAttribute("data-ne-label") ?? ""] ?? "");
      }
    } else if (msg.type === "reveal" && typeof msg.id === "number") {
      const el = elementFor(msg.id);
      if (el) {
        el.scrollIntoView({ block: "center", inline: "nearest" });
        clearReveal();
        el.setAttribute(REVEALED, "");
        revealed = el;
        revealTimer = setTimeout(clearReveal, 1200);
      }
    } else if (msg.type === "revert" && typeof msg.id === "number") {
      const el = elementFor(msg.id);
      if (el) el.innerHTML = msg.html ?? "";
    } else if (msg.type === "flush" && typeof msg.seq === "number") {
      // Commit the open edit now — the host asks before anything that would
      // lose edits. The commit goes out first on the same channel.
      commit();
      // Mid-composition, commit defers; defer the reply too so the host does not
      // swap the document and lose the characters being composed.
      if (pendingCommit) pendingFlush.push(msg.seq);
      else post({ type: "flushed", seq: msg.seq });
    }
  }) as EventListener);

  /** Pick `rgb()`/`rgba()` apart into [r, g, b, a]. null if not a color. */
  const colorOf = (value: string): [number, number, number, number] | null => {
    const m = /^rgba?\(([^)]+)\)/.exec(value);
    if (!m?.[1]) return null;
    const [r, g, b, a = 1] = m[1].split(",").map(Number);
    if (r === undefined || g === undefined || b === undefined || Number.isNaN(a)) return null;
    return [r, g, b, a];
  };

  /**
   * Measure the brightness of the background actually behind each block and mark
   * the dark ones; the injected style picks a white or black outline. Per block,
   * not per document. A translucent background is composited over the layers
   * below until an opaque one appears.
   */
  const paintContrast = (): void => {
    for (const el of document.querySelectorAll<HTMLElement>("[" + MARKER + "]")) {
      if (mimicked(el)) continue;
      const layers: [number, number, number, number][] = [];
      let node: HTMLElement | null = el;
      while (node) {
        const color = colorOf(getComputedStyle(node).backgroundColor);
        if (color && color[3] > 0) {
          layers.push(color);
          if (color[3] >= 1) break;
        }
        node = node.parentElement;
      }
      let r = 255;
      let g = 255;
      let b = 255;
      for (let i = layers.length - 1; i >= 0; i--) {
        const [lr, lg, lb, a] = layers[i] as [number, number, number, number];
        r = a * lr + (1 - a) * r;
        g = a * lg + (1 - a) * g;
        b = a * lb + (1 - a) * b;
      }
      if (0.2126 * r + 0.7152 * g + 0.0722 * b < 128) el.setAttribute(DARK, "");
      else el.removeAttribute(DARK);
    }
  };

  /** Collect the rendered result's actual text and send it to the host for verification. */
  const scan = (): void => {
    const blocks: { id: number; text: string }[] = [];
    for (const el of document.querySelectorAll<HTMLElement>("[" + MARKER + "]")) {
      if (mimicked(el)) continue;
      blocks.push({ id: idOf(el), text: el.textContent ?? "" });
    }
    verifiedEl = snapshotMarkers();
    paintContrast();
    post({ type: "ready", blocks });
  };

  // The sweep must run after the artifact's load handlers finish. We register
  // first, so push it one tick with setTimeout.
  on(window, "load", () => setTimeout(scan, 0));
  if (document.readyState === "complete") setTimeout(scan, 0);

  return () => {
    clearReveal();
    bar?.remove();
    bar = null;
    for (const { target, type, fn } of bound) target.removeEventListener(type, fn);
  };
}
