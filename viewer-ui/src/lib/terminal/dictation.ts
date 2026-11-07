/**
 * Voice input for the message form, through the browser's speech recognition.
 *
 * Only the parts that need no browser live here; `useDictation` drives the
 * recognizer itself.
 */

/** Whether this page can offer dictation. */
export type DictationAvailability =
  /** No speech recognition in this browser (Firefox, for one). */
  | "unsupported"
  /** Plain http on a name other than localhost: browsers keep the microphone
   *  closed there, without even asking. */
  | "insecure"
  | "ready";

/** The slice of the Web Speech API the form uses. */
export interface Recognizer {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  onresult: ((event: RecognizerResultEvent) => void) | null;
  onerror: ((event: { error: string }) => void) | null;
  onend: (() => void) | null;
  start: () => void;
  /** Stops listening but still delivers what was already heard. */
  stop: () => void;
  /** Stops and discards, for a form that is going away. */
  abort: () => void;
}

export interface RecognizerResultEvent {
  resultIndex: number;
  results: ArrayLike<{ isFinal: boolean; 0: { transcript: string } }>;
}

export type RecognizerFactory = () => Recognizer;

/** The browser's recognizer constructor, prefixed in Chrome and Safari. */
export function browserRecognizerFactory(): RecognizerFactory | null {
  const w = window as Window & {
    SpeechRecognition?: new () => Recognizer;
    webkitSpeechRecognition?: new () => Recognizer;
  };
  const Ctor = w.SpeechRecognition ?? w.webkitSpeechRecognition;
  return Ctor ? () => new Ctor() : null;
}

/** A browser without recognition is unsupported on any address, since HTTPS
 *  would not help it. Chrome and Safari keep the constructor on http and fail
 *  only once started, so for them the address is what to point at. */
export function dictationAvailability(
  secure: boolean,
  factory: RecognizerFactory | null,
): DictationAvailability {
  if (!factory) return "unsupported";
  return secure ? "ready" : "insecure";
}

/** The final text in a result event, from the first result it changed. */
export function finalTranscript(event: RecognizerResultEvent): string {
  let text = "";
  for (let i = event.resultIndex; i < event.results.length; i++) {
    const result = event.results[i];
    if (result.isFinal) text += result[0].transcript;
  }
  return text.trim();
}

/** The draft with dictated text added at its end, a space between them unless
 *  the draft already ends in whitespace. */
export function appendDictation(draft: string, text: string): string {
  if (text.length === 0) return draft;
  if (draft.length === 0 || /\s$/.test(draft)) return draft + text;
  return `${draft} ${text}`;
}

/**
 * What a recognizer error code means to the person, or null for one that
 * needs no message: `aborted` is our own stop, and `no-speech` is a silence
 * the person already noticed.
 */
export function dictationErrorMessage(code: string): string | null {
  switch (code) {
    case "aborted":
    case "no-speech":
      return null;
    case "not-allowed":
    case "service-not-allowed":
      return "Microphone access is blocked. Allow it in the browser's site settings.";
    case "audio-capture":
      return "No microphone was found.";
    case "network":
      return "The speech service could not be reached.";
    case "language-not-supported":
      return "This browser cannot recognize speech in this language.";
    default:
      return `Voice input failed (${code}).`;
  }
}

export const INSECURE_DICTATION_MESSAGE =
  "Voice input needs HTTPS or localhost. See the notifications guide for tailscale serve.";
