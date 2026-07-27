/** Font settings every terminal in the panel is opened with.
 *
 *  Shared rather than inlined at each `new Terminal(...)` because the startup
 *  handshake measures a cell with a throwaway terminal and sends the result to
 *  the server. A measuring terminal with a different font measures a different
 *  cell, and the PTY would be born at a size no real pane has — the exact
 *  defect the handshake exists to remove. */
export function terminalFontOptions() {
  return {
    // Touch screens are read at arm's length; a desktop is not.
    fontSize:
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function" &&
      window.matchMedia("(pointer: coarse)").matches
        ? 13
        : 12,
    fontFamily: getComputedStyle(document.body).fontFamily,
  };
}
