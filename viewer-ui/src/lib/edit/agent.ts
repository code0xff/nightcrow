/** The iframe agent is bundled with its imported helpers into one injected function. */
import {
  createAgentState,
  type AgentPost,
} from "./agent/types";
import { createDomTools } from "./agent/dom";
import { createEditController } from "./agent/editing";
import { createFormatting } from "./agent/formatting";
import { bindAgentEvents } from "./agent/events";
import { createScanner } from "./agent/scan";
import { createToolbar } from "./agent/toolbar";

export function previewAgent(token = ""): () => void {
  const state = createAgentState();
  const listeners: { target: EventTarget; type: string; fn: EventListener }[] = [];
  const timers = new Set<ReturnType<typeof setTimeout>>();
  let detached = false;

  const on = (target: EventTarget, type: string, fn: EventListener): void => {
    target.addEventListener(type, fn);
    listeners.push({ target, type, fn });
  };
  const post: AgentPost = (message) => {
    parent.postMessage(token ? { ...message, token } : message, "*");
  };
  const schedule = (callback: () => void, delay: number): ReturnType<typeof setTimeout> => {
    const timer = setTimeout(() => {
      timers.delete(timer);
      if (!detached) callback();
    }, delay);
    timers.add(timer);
    return timer;
  };
  const cancel = (timer: ReturnType<typeof setTimeout>): void => {
    clearTimeout(timer);
    timers.delete(timer);
  };
  const dom = createDomTools(state);
  let placeBar = (): void => {};
  const formatting = createFormatting(state, () => placeBar());
  const toolbar = createToolbar(state, formatting);
  placeBar = toolbar.placeBar;
  const edit = createEditController(state, post, toolbar.hideBar);
  const scanner = createScanner(state, dom, post, schedule, cancel);

  bindAgentEvents(state, dom, edit, scanner, toolbar, on, post, schedule, formatting);

  return () => {
    if (detached) return;
    detached = true;
    scanner.clearReveal();
    // A delayed load scan belongs to this document and must not message after detach.
    for (const timer of timers) clearTimeout(timer);
    timers.clear();
    toolbar.remove();
    for (const { target, type, fn } of listeners) target.removeEventListener(type, fn);
  };
}
