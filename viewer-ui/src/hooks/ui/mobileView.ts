// The narrow-screen section is a browser preference: a refresh should return
// to the screen the person was using without adding it to shared repo state.

import { useCallback, useState } from "react";
import type { MobileView } from "../../types";

const STORAGE_KEY = "nightcrow.mobileView";

function parse(raw: string | null): MobileView {
  return raw === "diff" || raw === "terminal" || raw === "files" ? raw : "files";
}

function load(): MobileView {
  try {
    return parse(localStorage.getItem(STORAGE_KEY));
  } catch {
    return "files";
  }
}

export function useMobileView() {
  const [mobileView, setView] = useState<MobileView>(load);
  const setMobileView = useCallback((view: MobileView) => {
    try {
      localStorage.setItem(STORAGE_KEY, view);
    } catch {
      // Storage is optional; navigation remains usable for this page lifetime.
    }
    setView(view);
  }, []);

  return { mobileView, setMobileView };
}
