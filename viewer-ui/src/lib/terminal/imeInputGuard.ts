// Keep in sync with xterm 6 CompositionHelper.keydown's continue-composing cases.
const COMPOSITION_KEY_CODES = new Set([20, 229, 16, 17, 18]);

export interface ImeInputGuard {
  dispose(): void;
  noteKeyPassedToXterm(event: KeyboardEvent): void;
}

/**
 * xterm 6 sends insertText immediately and CompositionHelper sends the same
 * textarea content after compositionend. The parent capture listener runs
 * before xterm's textarea input listener; keep native textarea mutation intact.
 */
export function installImeInputGuard(
  textarea: HTMLTextAreaElement,
): ImeInputGuard {
  const parent = textarea.parentElement;
  if (!parent) return { dispose() {}, noteKeyPassedToXterm() {} };

  let composing = false;
  let generation = 0;
  let cleanupTimer: ReturnType<typeof setTimeout> | undefined;

  const clearCleanup = () => {
    if (cleanupTimer !== undefined) clearTimeout(cleanupTimer);
    cleanupTimer = undefined;
  };
  const onCompositionStart = (event: Event) => {
    if (event.target !== textarea) return;
    generation++;
    composing = true;
    clearCleanup();
  };
  const onCompositionEnd = (event: Event) => {
    if (event.target !== textarea) return;
    if (!composing) return;
    const endingGeneration = generation;
    // This parent bubble listener runs after xterm queues its finalize timer.
    cleanupTimer = setTimeout(() => {
      cleanupTimer = undefined;
      if (generation === endingGeneration) composing = false;
    }, 0);
  };
  const onInput = (event: Event) => {
    const input = event as InputEvent;
    if (
      composing &&
      event.target === textarea &&
      input.inputType === "insertText"
    ) {
      event.stopImmediatePropagation();
    }
  };

  parent.addEventListener("compositionstart", onCompositionStart);
  parent.addEventListener("compositionend", onCompositionEnd);
  parent.addEventListener("input", onInput, true);

  return {
    noteKeyPassedToXterm(event) {
      if (
        event.type !== "keydown" ||
        COMPOSITION_KEY_CODES.has(event.keyCode) ||
        !composing
      ) {
        return;
      }
      // Other keys make xterm finalize synchronously in its keydown handler.
      generation++;
      composing = false;
      clearCleanup();
    },
    dispose() {
      composing = false;
      generation++;
      clearCleanup();
      parent.removeEventListener("compositionstart", onCompositionStart);
      parent.removeEventListener("compositionend", onCompositionEnd);
      parent.removeEventListener("input", onInput, true);
    },
  };
}
