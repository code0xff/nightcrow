/** The nightcrow mark: a black crow on a rounded accent tile. Shared with the
 *  web mirror's login/header so the two services read as one product. The tile
 *  is `bg-accent` rather than a baked colour, so it follows the runtime accent
 *  as the TUI's splash logo does (`ui/splash.rs` colours it with `accent`); the
 *  crow itself is the shared `crow-mono.svg` silhouette (transparent
 *  background) overlaid on top. The favicon keeps its own off-white-tiled
 *  `crow.svg`, which reads better against an arbitrary browser tab bar. */
export function Mark({ className }: { className?: string }) {
  return (
    <span
      className={`block overflow-hidden rounded-[20.7%] bg-accent ${className ?? ""}`}
    >
      <img src="/crow-mono.svg" alt="" aria-hidden="true" className="h-full w-full" />
    </span>
  );
}