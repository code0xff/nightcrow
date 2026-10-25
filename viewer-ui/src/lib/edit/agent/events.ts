import { LOCKED_ATTR } from "../markers";
import type { AgentPost, AgentState, AddListener, Schedule } from "./types";

type DomTools = {
  snapshotMarkers: () => Map<number, HTMLElement[]>;
  blockOf: (target: EventTarget | null) => HTMLElement | null;
  idOf: (el: HTMLElement) => number;
  elementFor: (id: number) => HTMLElement | null;
};

type EditController = {
  commit: () => void;
  cancel: () => void;
  startEdit: (el: HTMLElement) => void;
};

type PreviewController = {
  scan: () => void;
  reveal: (id: number) => void;
};

type Toolbar = {
  placeBar: () => void;
  updateLabels: (labels: Record<string, string>) => void;
};

type Formatting = {
  format: (command: string, value?: string) => void;
};

type HostMessage = {
  type?: string;
  ids?: number[];
  all?: number[];
  id?: number;
  html?: string;
  labels?: Record<string, string>;
  seq?: number;
};

export function bindAgentEvents(
  state: AgentState,
  dom: DomTools,
  edit: EditController,
  preview: PreviewController,
  toolbar: Toolbar,
  on: AddListener,
  post: AgentPost,
  schedule: Schedule,
  formatting: Formatting,
): void {
  const consume = (event: Event): void => {
    event.preventDefault();
    event.stopImmediatePropagation();
  };

  on(document, "selectionchange", toolbar.placeBar as EventListener);
  on(document, "mouseup", () => {
    state.barHeld = false;
  });

  on(document, "click", ((event: MouseEvent) => {
    if (state.bar && event.target instanceof Node && state.bar.contains(event.target)) {
      event.stopImmediatePropagation();
      return;
    }
    const el = dom.blockOf(event.target);
    if (el) {
      edit.startEdit(el);
      consume(event);
      return;
    }
    const wasEditing = state.editingId !== null;
    edit.commit();
    if (wasEditing) consume(event);
  }) as EventListener);

  on(document, "keydown", ((event: KeyboardEvent) => {
    if ((event.ctrlKey || event.metaKey) && !event.altKey && (event.key === "s" || event.key === "S")) {
      consume(event);
      if (state.composing || event.isComposing) return;
      edit.commit();
      post({ type: "save" });
      return;
    }
    if (
      (event.ctrlKey || event.metaKey) &&
      !event.altKey &&
      !event.shiftKey &&
      (event.key === "z" || event.key === "Z") &&
      state.editingId === null
    ) {
      consume(event);
      post({ type: "undo" });
      return;
    }
    if (state.editingId === null) return;
    if ((event.ctrlKey || event.metaKey) && !event.altKey) {
      const key = event.key.toLowerCase();
      const command =
        key === "b" ? "bold" : key === "i" ? "italic" : key === "u" ? "underline" : "";
      if (command) {
        consume(event);
        formatting.format(command);
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey && !state.composing && !event.isComposing) {
      edit.commit();
      consume(event);
      return;
    }
    if (event.key === "Escape") edit.cancel();
    event.stopImmediatePropagation();
  }) as EventListener);

  on(document, "beforeinput", ((event: InputEvent) => {
    if (state.editingId === null || event.inputType !== "insertParagraph") return;
    event.preventDefault();
    if (!state.composing && !event.isComposing) edit.commit();
  }) as EventListener);

  on(document, "touchend", (event) => {
    const target = (event as TouchEvent).target;
    if (!(state.bar && target instanceof Node && state.bar.contains(target))) state.barHeld = false;
    if (state.editingId !== null) event.stopImmediatePropagation();
  });

  on(document, "compositionstart", () => {
    state.composing = true;
  });
  on(document, "compositionend", () => {
    state.composing = false;
    if (state.pendingCommit) edit.commit();
  });

  on(document, "focusout", (event) => {
    const to = (event as FocusEvent).relatedTarget;
    if (state.bar && to instanceof Node && state.bar.contains(to)) return;
    edit.commit();
  });

  on(document, "paste", ((event: ClipboardEvent) => {
    if (state.editingId === null) return;
    event.preventDefault();
    const text = event.clipboardData?.getData("text/plain") ?? "";
    document.execCommand("insertText", false, text);
  }) as EventListener);

  on(window, "message", ((event: MessageEvent) => {
    // Inbound messages have no token in the protocol; source identity is their guard.
    if (event.source !== parent) return;
    const message = event.data as HostMessage | null;
    if (!message || typeof message !== "object") return;
    if (message.type === "locked" && message.ids) {
      state.verified = true;
      state.locked.clear();
      for (const id of message.ids) state.locked.add(id);
      if (message.all) state.known = new Set(message.all);
      if (state.verifiedEl === null) state.verifiedEl = dom.snapshotMarkers();
      for (const [id, elements] of state.verifiedEl) {
        for (const el of elements) {
          const show = state.locked.has(id) && (el.textContent ?? "").trim();
          if (show) el.setAttribute(LOCKED_ATTR, "");
          else el.removeAttribute(LOCKED_ATTR);
        }
      }
    } else if (message.type === "labels" && message.labels) {
      toolbar.updateLabels(message.labels);
    } else if (message.type === "reveal" && typeof message.id === "number") {
      preview.reveal(message.id);
    } else if (message.type === "revert" && typeof message.id === "number") {
      const el = dom.elementFor(message.id);
      if (el) el.innerHTML = message.html ?? "";
    } else if (message.type === "flush" && typeof message.seq === "number") {
      edit.commit();
      if (state.pendingCommit) state.pendingFlush.push(message.seq);
      else post({ type: "flushed", seq: message.seq });
    }
  }) as EventListener);

  on(window, "load", () => schedule(preview.scan, 0));
  if (document.readyState === "complete") schedule(preview.scan, 0);
}
