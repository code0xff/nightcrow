import { useEffect, useState } from "react";
import type { ShortcutSettings } from "../../hooks/useShortcutSettings";

/**
 * The leader, and the three things that can be done to it.
 *
 * It sits in the help sheet rather than in a settings screen of its own because
 * the sheet is where somebody is already reading what the keys are, and the
 * reason to rebind — `Ctrl+F` is the browser's Find — is the warning printed
 * right here.
 */
export function LeaderSettings({ settings }: { settings: ShortcutSettings }) {
  const { leaderText, conflict, setLeader, disable, reset } = settings;
  const [draft, setDraft] = useState(leaderText);
  const [rejected, setRejected] = useState<string | null>(null);

  // Follow the stored leader: `disable` and `reset` change it from outside this
  // field, and a field still showing what was there would say the chord is
  // something it is not.
  useEffect(() => {
    setDraft(leaderText);
    setRejected(null);
  }, [leaderText]);

  const apply = () => {
    // False means `parseChord` refused the text. Said out loud rather than
    // swallowed: silently keeping the old leader looks identical to a rebinding
    // that worked until the next keystroke fails.
    if (!setLeader(draft)) {
      setRejected(
        `"${draft}" is not a chord. Write it as Ctrl+F, Alt+Space or Ctrl+Shift+ArrowLeft.`,
      );
      return;
    }
    setRejected(null);
  };

  return (
    <section className="shrink-0 border-b border-ink-700 px-3 py-2">
      <p className="text-ink-400">
        Leader chord:{" "}
        {leaderText ? (
          <kbd className="rounded-sm border border-ink-600 bg-ink-850 px-1.5 py-0.5 font-mono text-ink-50">
            {leaderText}
          </kbd>
        ) : (
          <span className="text-ink-200">switched off</span>
        )}
      </p>
      <div className="mt-1.5 flex items-center gap-2">
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") apply();
          }}
          placeholder="Ctrl+F"
          aria-label="leader chord"
          spellCheck={false}
          autoCapitalize="none"
          autoCorrect="off"
          className="min-w-0 flex-1 rounded-sm border border-ink-700 bg-ink-950 px-2 py-1 font-mono text-ink-50 placeholder:text-ink-400 focus:border-ink-600 focus:outline-none"
        />
        <button
          type="button"
          onClick={apply}
          className="shrink-0 rounded-sm border border-ink-700 px-2 py-1 text-ink-200 hover:bg-ink-850"
        >
          Rebind
        </button>
        <button
          type="button"
          onClick={disable}
          title="Leave the leader unbound. Standalone chords keep working."
          className="shrink-0 rounded-sm border border-ink-700 px-2 py-1 text-ink-200 hover:bg-ink-850"
        >
          Switch off
        </button>
        <button
          type="button"
          onClick={reset}
          title="Back to the leader the TUI ships with"
          className="shrink-0 rounded-sm border border-ink-700 px-2 py-1 text-ink-200 hover:bg-ink-850"
        >
          Reset
        </button>
      </div>
      {rejected && (
        <p role="alert" className="mt-1 text-removed">
          {rejected}
        </p>
      )}
      {conflict && (
        <p role="status" className="mt-1 text-accent">
          {conflict} Rebind it above if you would rather keep that key.
        </p>
      )}
    </section>
  );
}
