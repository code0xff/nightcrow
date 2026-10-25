/**
 * A button label that says "working" without changing its width. The text
 * stays the same; a small dot pulses at its top right corner while `busy`.
 * The dot is positioned over the label rather than laid out beside it, so it
 * takes no room: an idle button is as wide as its text, and starting or
 * finishing never resizes it. Pair it with `aria-busy` on the button: the dot
 * is decoration only.
 */
export function BusyLabel({ busy, children }: { busy: boolean; children: React.ReactNode }) {
  return (
    <span className="relative inline-block">
      {children}
      {busy && (
        <span
          aria-hidden="true"
          data-busy=""
          className="nc-busy-dot absolute -right-2 -top-0.5 h-1.5 w-1.5 rounded-full bg-current"
        />
      )}
    </span>
  );
}
