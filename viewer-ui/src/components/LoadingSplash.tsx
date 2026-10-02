import { Mark } from "./Mark";

export function LoadingSplash() {
  return (
    <div className="flex h-full items-center justify-center p-6">
      <div className="flex flex-col items-center gap-3 text-ink-400">
        <Mark className="h-12 w-12 animate-pulse" />
        <span className="text-caption uppercase">
          Loading…
        </span>
      </div>
    </div>
  );
}
