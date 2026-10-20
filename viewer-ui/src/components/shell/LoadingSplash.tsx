import { SkeletonBar, SkeletonRows } from "../feedback/Skeleton";

/**
 * The screen before the session answers: the app's own outline in skeleton —
 * header, list, content — so what follows fills a shape that is already there
 * rather than replacing a centred logo.
 */
export function LoadingSplash() {
  return (
    <div aria-busy="true" aria-label="Loading" className="flex h-full flex-col">
      <div className="flex h-[var(--nc-header-h)] shrink-0 items-center gap-3 border-b border-chrome-line bg-ink-900 px-3">
        <SkeletonBar className="h-6 w-6 rounded-sm" />
        <SkeletonBar className="w-24" />
        <SkeletonBar className="ml-auto w-5" />
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[18rem_minmax(0,1fr)]">
        <div className="border-chrome-line py-2 md:border-r">
          <SkeletonRows rows={10} />
        </div>
        <div className="hidden py-2 md:block">
          <SkeletonRows rows={14} />
        </div>
      </div>
    </div>
  );
}
