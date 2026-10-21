/**
 * A button label that says "working" without changing its width. The text
 * stays the same; a small dot beside it pulses while `busy`. The dot's slot is
 * always reserved (invisible when idle), so starting or finishing never
 * resizes the button or shifts whatever sits next to it. Pair it with
 * `aria-busy` on the button: the dot is decoration only.
 */
export function BusyLabel({ busy, children }: { busy: boolean; children: React.ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      {children}
      <span
        aria-hidden="true"
        data-busy={busy}
        className={`h-1.5 w-1.5 shrink-0 rounded-full bg-current ${
          busy ? "nc-busy-dot" : "invisible"
        }`}
      />
    </span>
  );
}
