import type { IDisposable, ITerminalAddon, Terminal } from "@xterm/xterm";

/** A renderer addon that can report losing its rendering context. */
interface WebglRendererAddon extends ITerminalAddon {
  readonly onContextLoss: (listener: () => void) => IDisposable;
}

/**
 * Put a pane on xterm's WebGL renderer, or leave it on the DOM one.
 *
 * WebGL is optional: a browser may refuse a context (a blocklisted driver, a
 * machine out of GPU memory, a context budget already spent), and the terminal
 * still has to work. `loadAddon` activates the addon immediately, so that call
 * is where an unavailable context surfaces; the pane falls back to the renderer
 * xterm opened with, which needs no action on our side.
 *
 * The addon restores the DOM renderer itself when disposed, so a lost context
 * is handled by disposing it: xterm rebuilds its default renderer, and the
 * screen is redrawn from the buffer the parser kept. Keeping the addon would
 * leave the pane painting into a dead context.
 */
export function enableWebglRenderer(
  term: Pick<Terminal, "loadAddon">,
  createAddon: () => WebglRendererAddon,
): boolean {
  let addon: WebglRendererAddon;
  try {
    addon = createAddon();
  } catch {
    return false;
  }
  // Only released with the addon: while it renders, its own emitter owns this
  // listener. The error path below releases it explicitly.
  const listener = addon.onContextLoss(() => addon.dispose());
  try {
    term.loadAddon(addon);
  } catch {
    listener.dispose();
    addon.dispose();
    return false;
  }
  return true;
}
