import { MARKER_ATTR } from "../markers";
import type { AgentPost, AgentState } from "./types";

export function createEditController(
  state: AgentState,
  post: AgentPost,
  hideBar: () => void,
) {
  const answerFlushes = (): void => {
    for (const seq of state.pendingFlush.splice(0)) post({ type: "flushed", seq });
  };

  const takeEdit = (): { id: number; el: HTMLElement | null; before: string | null } | null => {
    if (state.editingId === null) return null;
    // Clear first: removing contenteditable synchronously raises focusout and re-enters commit.
    const taken = { id: state.editingId, el: state.editingEl, before: state.snapshot };
    state.editingId = null;
    state.editingEl = null;
    state.snapshot = null;
    state.pendingCommit = false;
    return taken;
  };

  const commit = (): void => {
    if (state.editingId === null) return;
    if (state.composing) {
      state.pendingCommit = true;
      return;
    }
    const taken = takeEdit();
    if (!taken) return;
    const { id, el, before } = taken;
    if (el) {
      el.removeAttribute("contenteditable");
      el.removeAttribute("enterkeyhint");
      // Keep preview furniture out of the HTML patch if an artifact moved it into the block.
      if (state.bar && el.contains(state.bar)) document.documentElement.appendChild(state.bar);
      post({
        type: "edit",
        id,
        html: el.innerHTML,
        pristine: el.innerHTML === before,
      });
    }
    hideBar();
    post({ type: "select", id: null });
    answerFlushes();
  };

  const cancel = (): void => {
    const taken = takeEdit();
    if (!taken) return;
    const { el, before } = taken;
    if (el) {
      el.removeAttribute("contenteditable");
      el.removeAttribute("enterkeyhint");
      // Restore only after moving the toolbar; innerHTML replacement would detach it.
      if (state.bar && el.contains(state.bar)) document.documentElement.appendChild(state.bar);
      if (before !== null) el.innerHTML = before;
    }
    hideBar();
    post({ type: "select", id: null });
    answerFlushes();
  };

  const startEdit = (el: HTMLElement): void => {
    const id = Number(el.getAttribute(MARKER_ATTR));
    if (!state.verified) {
      post({ type: "notReady" });
      return;
    }
    if (state.locked.has(id)) {
      post({ type: "blocked", id });
      return;
    }
    if (state.editingId === id) return;
    commit();
    // Composition keeps the previous edit open until compositionend commits it.
    if (state.editingId !== null) return;
    state.editingId = id;
    state.editingEl = el;
    state.snapshot = el.innerHTML;
    el.setAttribute("contenteditable", "true");
    el.setAttribute("enterkeyhint", "done");
    el.focus();
    post({ type: "select", id });
  };

  return { commit, cancel, startEdit };
}
