import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useState,
  type ReactNode,
} from "react";

const STORAGE_KEY = "nightcrow.viewer.scale";
const BASE_FONT_SIZE_PX = 14;
export const SCREEN_SCALES = [80, 90, 100, 110, 120, 130, 140, 150] as const;
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
    try {
      localStorage.setItem(STORAGE_KEY, String(normalized));
    } catch {
      // The setting still applies to this page when storage is unavailable.
    }
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
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw !== null) return normalizeScreenScale(Number(raw));
  } catch {
    // Storage is a preference, not a requirement for opening the viewer.
  }
  return 100;
}

export function applyScreenScale(scale: ScreenScale): void {
  if (typeof document === "undefined") return;
  document.documentElement.style.fontSize = `${BASE_FONT_SIZE_PX * scale / 100}px`;
}

function normalizeScreenScale(value: number): ScreenScale {
  if (!Number.isFinite(value)) return 100;
  return SCREEN_SCALES.reduce((closest, candidate) =>
    Math.abs(candidate - value) < Math.abs(closest - value) ? candidate : closest,
  );
}
