import { Mark } from "./Mark";

/** The mark and the name, wherever the page puts them: in the header, or at
 *  the head of the left tab strip so the tabs hang under the title. */
export function Brand() {
  return (
    <>
      <Mark className="h-[22px] w-[22px] shrink-0" />
      <span className="text-[16px] font-medium tracking-[0.04em] text-ink-50">nightcrow</span>
    </>
  );
}
