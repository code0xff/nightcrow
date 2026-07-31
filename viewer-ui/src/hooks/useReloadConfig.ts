import { useCallback, useRef, useState } from "react";
import { api } from "../api";
import { toast } from "../lib/toast";

/**
 * Ask the server to re-read `config.toml`.
 *
 * This is not a page reload, and nothing on screen changes as a result: the
 * plugins it re-applies are child processes the page never sees, and the startup
 * panes it replaces only reach projects opened afterwards. So the toast is the
 * whole of the feedback — without it the button would look inert.
 *
 * One request at a time. The button is disabled while `pending`, and the ref
 * guards the case a disabled button cannot: a second call arriving from a
 * keyboard activation in the same tick.
 */
export function useReloadConfig() {
  const [pending, setPending] = useState(false);
  const inFlight = useRef(false);

  const reload = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    try {
      // The server writes this sentence, so a browser toast and a TUI notice say
      // the same thing about the same reload.
      toast.success(await api.reloadConfig());
    } catch (err) {
      // Shown as written: a refused reload names the key that was wrong in the
      // operator's own config file, which is the whole value of surfacing it.
      toast.error(
        err instanceof Error ? err.message : "could not reload the config",
      );
    } finally {
      inFlight.current = false;
      setPending(false);
    }
  }, []);

  return { reload, pending };
}
