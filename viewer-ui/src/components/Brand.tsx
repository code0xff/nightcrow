import { Mark } from "./Mark";

/** The mark and the name, in the header's left corner. Below `md` only the
 *  mark is drawn: the header there also carries the project menu, and a repo
 *  with a long name would otherwise push the controls off a phone. */
export function Brand() {
  return (
    <>
      <Mark className="h-[22px] w-[22px] shrink-0" />
      <span className="hidden text-[16px] font-medium tracking-[0.04em] text-ink-50 md:inline">
        nightcrow
      </span>
    </>
  );
}
