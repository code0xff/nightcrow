import { useCallback, useEffect, useRef, useState } from "react";
import {
  dictationErrorMessage,
  finalTranscript,
  type Recognizer,
  type RecognizerFactory,
} from "../../lib/terminal/dictation";

/**
 * One utterance per press: the recognizer stops on its own after a pause, and
 * pressing again while it listens stops it early. Only final text is handed
 * over, so nothing half-recognized reaches the draft.
 *
 * The recognizer is aborted when the form closes, so a microphone is never
 * left open behind it.
 */
export function useDictation(
  factory: RecognizerFactory | null,
  onText: (text: string) => void,
) {
  const [listening, setListening] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const recognizerRef = useRef<Recognizer | null>(null);
  // The latest callback, since a result arrives renders after the press.
  const onTextRef = useRef(onText);
  onTextRef.current = onText;

  // `stop`, not `abort`: a phrase still being recognized when the button is
  // pressed would otherwise be thrown away.
  const stop = useCallback(() => {
    recognizerRef.current?.stop();
  }, []);

  const start = useCallback(() => {
    if (!factory || recognizerRef.current) return;
    let recognizer: Recognizer;
    try {
      recognizer = factory();
    } catch {
      setError(dictationErrorMessage("unavailable"));
      return;
    }
    recognizer.lang = navigator.language;
    recognizer.continuous = false;
    recognizer.interimResults = false;
    recognizer.onresult = (event) => {
      const text = finalTranscript(event);
      if (text) onTextRef.current(text);
    };
    recognizer.onerror = (event) => setError(dictationErrorMessage(event.error));
    recognizer.onend = () => {
      if (recognizerRef.current === recognizer) recognizerRef.current = null;
      setListening(false);
    };
    setError(null);
    try {
      recognizer.start();
    } catch {
      // Starting can throw synchronously, e.g. while another start is pending.
      setError(dictationErrorMessage("start-failed"));
      return;
    }
    recognizerRef.current = recognizer;
    setListening(true);
  }, [factory]);

  useEffect(
    () => () => {
      const recognizer = recognizerRef.current;
      recognizerRef.current = null;
      if (recognizer) {
        // Unmounted: no state to update, only the microphone to release.
        recognizer.onend = null;
        recognizer.onresult = null;
        recognizer.onerror = null;
        recognizer.abort();
      }
    },
    [],
  );

  return {
    listening,
    error,
    toggle: listening ? stop : start,
    dismissError: () => setError(null),
    /** Shows a message without starting, for a page that cannot dictate. */
    report: setError,
  };
}
