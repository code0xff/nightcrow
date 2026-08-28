/// One gutter cell: the line numbers for a row, in as many columns as the view
/// shows (two for a unified diff, one for a split half or a file).
///
/// The cell is `sticky` rather than an ordinary leading span because the panes
/// scroll horizontally, and a number that scrolls with the code slides off the
/// left edge — the web counterpart of the TUI keeping its gutter in a paragraph
/// of its own (`ui/diff_viewer/gutter.rs`). Sticky in turn forces the opaque
/// base: the kind tints are translucent, so without it the code passing
/// underneath shows through the numbers. `select-none` keeps them out of
/// copied code, the way the `+`/`-` markers already are.
///
/// The `1ch` padding and gap are the TUI's `LINENO_PAD`/`LINENO_GAP` — one
/// space around the numbers and between the two columns.
export function LineNos({
  nos,
  digits,
  tint = "",
  stickyClass = "sticky left-0",
}: {
  /// One entry per column; `undefined` leaves that column blank, which is what
  /// an added line's old side (and a removed line's new side) has to show.
  nos: (number | undefined)[];
  digits: number;
  /// Row background to repeat over the opaque base, so the cell reads as part
  /// of its row rather than a notch of pane colour.
  tint?: string;
  /** Split virtualization pins the new-side gutter at the second half. */
  stickyClass?: string;
}) {
  return (
    <span className={`${stickyClass} shrink-0 select-none bg-ink-950`}>
      <span className={`flex gap-[1ch] px-[1ch] text-ink-400 ${tint}`}>
        {nos.map((no, i) => (
          <span key={i} className="text-right" style={{ width: `${digits}ch` }}>
            {no ?? ""}
          </span>
        ))}
      </span>
    </span>
  );
}
