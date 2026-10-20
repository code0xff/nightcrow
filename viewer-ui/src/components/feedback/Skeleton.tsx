/**
 * Placeholder bars standing where content is about to be, so the layout is
 * already the shape it will have and the first paint is not a blank panel or
 * a lone "Loading…". Widths vary by row so a list reads as a list; the shimmer
 * stops under `prefers-reduced-motion`. Hidden from assistive tech: the
 * surrounding region announces itself with `aria-busy`.
 */
const WIDTHS = ["w-3/5", "w-2/5", "w-4/5", "w-1/2", "w-3/4", "w-1/3", "w-2/3"];

export function SkeletonBar({ className = "" }: { className?: string }) {
  return <span aria-hidden="true" className={`nc-skeleton block h-2.5 rounded-xs ${className}`} />;
}

/** `rows` bars at list rhythm, as `<li>`s when it stands inside a list. */
export function SkeletonRows({
  rows = 6,
  as = "div",
  className = "",
}: {
  rows?: number;
  as?: "div" | "li";
  className?: string;
}) {
  const Row = as;
  return (
    <>
      {Array.from({ length: rows }, (_, index) => (
        <Row key={index} aria-hidden="true" className={`flex items-center gap-2 px-3 py-1.5 ${className}`}>
          <SkeletonBar className="w-3 shrink-0" />
          <SkeletonBar className={WIDTHS[index % WIDTHS.length]} />
        </Row>
      ))}
    </>
  );
}
