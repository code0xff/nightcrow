import type { ReactNode } from "react";
import { SpinnerIcon } from "../icons/actions";

/**
 * A button's label that shows a spinner while its action is in flight, without
 * either changing the words or moving the button's edges.
 *
 * The label stays rendered and only fades (`opacity-0`), so it keeps holding
 * the width it had — swapping in "Signing in…" made the button jump, which is
 * what this replaces. The spinner sits over that reserved space rather than
 * beside it, so busy and idle are the same size. The faded label stays in the
 * accessibility tree; `aria-busy` on the button (its own concern) names the
 * state for a screen reader.
 */
export function BusyLabel({
  busy,
  children,
}: {
  busy: boolean;
  children: ReactNode;
}) {
  return (
    <span className="relative inline-flex items-center justify-center">
      <span className={busy ? "opacity-0" : undefined}>{children}</span>
      {busy && (
        <span className="absolute inset-0 flex items-center justify-center">
          <SpinnerIcon className="h-4 w-4" />
        </span>
      )}
    </span>
  );
}
