import { SkeletonBar, SkeletonRows } from "../feedback/Skeleton";

/**
 * The screen before the session answers: the app's own outline in skeleton —
 * header, list and content over the terminal, and the view switcher on a
 * phone — so what follows fills a shape that is already there rather than
 * replacing a centred logo.
 */
export function LoadingSplash() {
  return (
    <div aria-busy="true" aria-label="Loading" className="flex h-full flex-col">
      <div className="flex h-(--nc-header-h) shrink-0 items-center gap-2 border-b border-chrome-line bg-ink-900 px-[12.8px]">
        {/* Mark, then the name on wide screens or the project menu on a phone,
            as `Brand` and `Header` draw them. */}
        <SkeletonBar className="h-[22px] w-[22px] shrink-0 rounded-sm" />
        <SkeletonBar className="hidden w-24 md:block" />
        <SkeletonBar className="w-28 md:hidden" />
        <SkeletonBar className="ml-auto h-4 w-4 shrink-0" />
        <SkeletonBar className="ml-1 h-4 w-4 shrink-0" />
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-[minmax(0,1fr)] md:grid-cols-[18rem_minmax(0,1fr)] md:grid-rows-[minmax(0,3fr)_minmax(0,2fr)]">
        <div className="overflow-hidden border-chrome-line py-2 md:border-r">
          <SkeletonRows rows={10} />
        </div>
        <div className="hidden overflow-hidden py-2 md:block">
          <SkeletonRows rows={14} />
        </div>
        <div className="hidden overflow-hidden border-t border-chrome-line md:col-span-2 md:block">
          <div className="flex h-8 items-center gap-2 border-b border-chrome-line px-3">
            <SkeletonBar className="w-30" />
            <SkeletonBar className="w-30" />
          </div>
          <div className="py-2">
            <SkeletonRows rows={5} />
          </div>
        </div>
      </div>
      <div className="flex shrink-0 border-t border-chrome-line bg-ink-900 md:hidden">
        {[0, 1, 2].map((index) => (
          <div key={index} className="flex min-h-11 flex-1 flex-col items-center justify-center gap-1 py-1">
            <SkeletonBar className="h-5 w-5" />
            <SkeletonBar className="h-2 w-10" />
          </div>
        ))}
      </div>
    </div>
  );
}
