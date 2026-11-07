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
 * recognition; on plain http it stays visible but inert, its name and tooltip
 * saying why, since hiding it would leave no clue why a phone never offers it.
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
  const { listening, toggle } = useDictation(
    availability === "ready" ? factory : null,
    onText,
  );
  if (availability === "unsupported") return null;
  const ready = availability === "ready";

  return (
    <button
      type="button"
      // Inert on plain http: the reason is in its name and tooltip, not a
      // line that would push the form's buttons around.
      onClick={ready ? toggle : undefined}
      aria-disabled={!ready || undefined}
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
  );
}
