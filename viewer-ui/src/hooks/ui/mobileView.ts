// The narrow-screen section is a browser preference: a refresh should return
// to the screen the person was using without adding it to shared repo state.

import { useCallback, useState } from "react";
import type { MobileView } from "../../types";
import { readPreference, writePreference } from "../../lib/ui/preferenceStorage";

const STORAGE_KEY = "nightcrow.mobileView";

function parse(raw: string | null): MobileView {
  return raw === "diff" || raw === "terminal" || raw === "files" ? raw : "files";
}

function load(): MobileView {
  return parse(readPreference(STORAGE_KEY));
}

export function useMobileView() {
  const [mobileView, setView] = useState<MobileView>(load);
  const setMobileView = useCallback((view: MobileView) => {
    writePreference(STORAGE_KEY, view);
    setView(view);
  }, []);

  return { mobileView, setMobileView };
}
