import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useState,
  type ReactNode,
} from "react";
import { readPreference, writePreference } from "../../lib/ui/preferenceStorage";

const STORAGE_KEY = "nightcrow.viewer.scale";
export const SCREEN_SCALES = [50, 60, 70, 80, 90, 100, 110, 120, 130] as const;
export type ScreenScale = (typeof SCREEN_SCALES)[number];

interface ScreenScaleContextValue {
  scale: ScreenScale;
  setScale: (scale: ScreenScale) => void;
}

const DEFAULT_VALUE: ScreenScaleContextValue = {
  scale: 100,
  setScale: () => undefined,
};
const ScreenScaleContext = createContext(DEFAULT_VALUE);

export function ScreenScaleProvider({ children }: { children: ReactNode }) {
  const [scale, setCurrentScale] = useState(readScreenScale);

  useLayoutEffect(() => applyScreenScale(scale), [scale]);

  const setScale = useCallback((next: ScreenScale) => {
    const normalized = normalizeScreenScale(next);
    setCurrentScale(normalized);
    writePreference(STORAGE_KEY, String(normalized));
  }, []);

  return (
    <ScreenScaleContext.Provider value={{ scale, setScale }}>
      {children}
    </ScreenScaleContext.Provider>
  );
}

export function useScreenScale(): ScreenScaleContextValue {
  return useContext(ScreenScaleContext);
}

export function readScreenScale(): ScreenScale {
  const raw = readPreference(STORAGE_KEY);
  if (raw !== null) return normalizeScreenScale(Number(raw));
  return 100;
}

/** Publish the scale as `--nc-content-zoom`, which only the content regions
 *  (`.nc-content`: the repository lists and the file pane body) read. The
 *  header, tabs, toolbars, menus and dialogs stay at their designed size, so
 *  shrinking the reading text does not shrink the controls around it. */
export function applyScreenScale(scale: ScreenScale): void {
  if (typeof document === "undefined") return;
  document.documentElement.style.setProperty("--nc-content-zoom", String(scale / 100));
}

function normalizeScreenScale(value: number): ScreenScale {
  if (!Number.isFinite(value)) return 100;
  return SCREEN_SCALES.reduce((closest, candidate) =>
    Math.abs(candidate - value) < Math.abs(closest - value) ? candidate : closest,
  );
}
