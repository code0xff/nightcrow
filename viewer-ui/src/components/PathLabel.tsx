/**
 * A path rendered in full, reachable by scrolling the list sideways.
 *
 * Truncating instead would cut the tail, which is the one part that tells two
 * rows apart — `src/web/viewer/server.rs` and `src/web/viewer/terminal.rs` both
 * become `src/web/viewer/…` in a narrow sidebar. `title` still carries the
 * whole path so a hover answers without scrolling.
 */
export function PathLabel({
  path,
  from,
  className,
}: {
  path: string;
  from?: string;
  className?: string;
}) {
  return (
    <span
      className={`whitespace-nowrap ${className ?? ""}`}
      title={from ? `${from} → ${path}` : path}
    >
      {from ? `${from} → ${path}` : path}
    </span>
  );
}