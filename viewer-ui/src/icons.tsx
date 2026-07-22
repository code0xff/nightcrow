/**
 * Maximise/restore glyph, traced from Lucide's `maximize` and `minimize`
 * (https://lucide.dev — ISC, Copyright (c) 2026 Lucide Icons and Contributors).
 *
 * Inlined rather than added as a dependency: a couple of icons do not justify
 * an icon package, and the bundle has to stay self-contained for the viewer's
 * `default-src 'self'` CSP. `currentColor` lets the host button's text colour
 * drive it, hover states included.
 */
export function MaximizeIcon({ maximized }: { maximized: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className="h-4 w-4"
    >
      {maximized ? (
        <>
          <path d="M8 3v3a2 2 0 0 1-2 2H3" />
          <path d="M21 8h-3a2 2 0 0 1-2-2V3" />
          <path d="M3 16h3a2 2 0 0 1 2 2v3" />
          <path d="M16 21v-3a2 2 0 0 1 2-2h3" />
        </>
      ) : (
        <>
          <path d="M8 3H5a2 2 0 0 0-2 2v3" />
          <path d="M21 8V5a2 2 0 0 0-2-2h-3" />
          <path d="M3 16v3a2 2 0 0 0 2 2h3" />
          <path d="M16 21h3a2 2 0 0 0 2-2v-3" />
        </>
      )}
    </svg>
  );
}

/**
 * Expand/collapse chevron for the file tree (VS Code style), traced from
 * Lucide's `chevron-right`: points right when closed and rotates to point down
 * when open.
 */
export function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className={`h-3.5 w-3.5 shrink-0 transition-transform ${open ? "rotate-90" : ""}`}
    >
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

/**
 * Close glyph, traced from Lucide's `x` (same provenance as above).
 *
 * Replaces the `×` character these buttons used to render: U+00D7 is a maths
 * operator drawn near x-height, so at the viewer's ~12px mono it left only a
 * few pixels of ink and read as far lighter than the icons beside it.
 *
 * Sized by the caller — the terminal tab matches `MaximizeIcon` next to it,
 * while the header's project tabs take a smaller one to suit their label.
 */
export function XIcon({ className = "h-4 w-4" }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className={`shrink-0 ${className}`}
    >
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </svg>
  );
}

/**
 * Add glyph, traced from Lucide's `plus` (same provenance as above).
 *
 * Here for the reason `XIcon` is: the `+` character is drawn to the font's maths
 * metrics, well inside the em box, so beside a 16px stroked icon it reads as a
 * smaller, lighter mark rather than its equal. Same box, same stroke, same
 * weight as the maximise button it sits next to.
 */
export function PlusIcon({ className = "h-4 w-4" }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className={`shrink-0 ${className}`}
    >
      <path d="M5 12h14" />
      <path d="M12 5v14" />
    </svg>
  );
}

/**
 * Split-view glyph, traced from Lucide's `columns-2` (same provenance as
 * above): a framed pane bisected by a vertical rule. Static — the button
 * signals its on/off state through `aria-pressed` and an accent text colour,
 * the way the header's accent swatch leans on its tooltip rather than a
 * second glyph.
 */
export function SplitViewIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className="h-4 w-4"
    >
      <rect width="18" height="18" x="3" y="3" rx="2" />
      <path d="M12 3v18" />
    </svg>
  );
}

/**
 * Preview glyph, traced from Lucide's `eye` (same provenance as above). Toggles
 * the markdown file pane between its rendered view and raw source; the button
 * signals which is active through `aria-pressed` and an accent text colour.
 */
export function PreviewIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className="h-4 w-4"
    >
      <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7Z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}

/** Search glyph, traced from Lucide's `search` (same provenance as above). */
export function SearchIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className="h-4 w-4"
    >
      <circle cx="11" cy="11" r="8" />
      <path d="m21 21-4.3-4.3" />
    </svg>
  );
}
