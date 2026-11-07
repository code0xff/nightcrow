import { useDictation } from "../../hooks/terminal/useDictation";
import {
  INSECURE_DICTATION_MESSAGE,
  type DictationAvailability,
  type RecognizerFactory,
} from "../../lib/terminal/dictation";
import { MicIcon } from "../icons/actions";

/** The message form's icon buttons: the toolbar's 16px icon and colours, in a
 *  square big enough to tap. */
export const FORM_ICON_BUTTON =
  "flex h-11 w-11 shrink-0 items-center justify-center rounded-sm hover:text-accent";

/**
 * The message form's voice input. Absent where the browser has no speech
 * recognition; on plain http it stays visible but only explains, since hiding
 * it would leave no clue why a phone never offers it.
 */
export function DictationButton({
  availability,
  factory,
  onText,
}: {
  availability: DictationAvailability;
  factory: RecognizerFactory | null;
  onText: (text: string) => void;
}) {
  const { listening, error, toggle, dismissError, report } = useDictation(
    availability === "ready" ? factory : null,
    onText,
  );
  if (availability === "unsupported") return null;
  const ready = availability === "ready";

  return (
    <>
      <button
        type="button"
        onClick={ready ? toggle : () => report(INSECURE_DICTATION_MESSAGE)}
        aria-pressed={ready ? listening : undefined}
        aria-label={
          !ready ? "Voice input needs HTTPS" : listening ? "Stop voice input" : "Voice input"
        }
        title={ready ? (listening ? "Stop voice input" : "Voice input") : INSECURE_DICTATION_MESSAGE}
        className={`${FORM_ICON_BUTTON} ${
          listening ? "animate-pulse text-accent" : ready ? "text-ink-400" : "text-ink-600"
        }`}
      >
        <MicIcon />
      </button>
      {error && (
        <span role="alert" className="flex basis-full items-center gap-2 text-removed">
          <span className="min-w-0">{error}</span>
          <button
            type="button"
            onClick={dismissError}
            className="min-h-11 shrink-0 rounded-sm px-2 underline"
          >
            Dismiss
          </button>
        </span>
      )}
    </>
  );
}
