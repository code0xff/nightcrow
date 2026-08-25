import { useState } from "react";
import { api } from "../api";
import { Mark } from "./Mark";

export function Login({ onSuccess }: { onSuccess: () => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.login(password);
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : "login failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center p-6">
      <form onSubmit={submit} className="w-[17rem] max-w-[86vw]">
        <Mark className="mx-auto mb-3 block h-10 w-10" />
        <h1 className="text-center text-lg font-medium tracking-wide text-ink-50">
          nightcrow
        </h1>
        <p className="mt-1 mb-5 text-center text-[0.62rem] tracking-[0.18em] text-ink-400 uppercase">
          web viewer
        </p>
        {error && <p className="mb-2.5 text-center text-removed">{error}</p>}
        <input
          type="password"
          autoFocus
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="password"
          className="mb-2 w-full rounded-md border border-ink-700 bg-ink-900 px-2.5 py-1.5 outline-none placeholder:text-ink-400 focus:border-accent focus:ring-[3px] focus:ring-accent/15"
        />
        <button
          type="submit"
          disabled={busy}
          className="w-full rounded-md bg-ink-50 py-1.5 font-semibold text-ink-950 hover:bg-white disabled:opacity-50"
        >
          {busy ? "Signing in…" : "Sign in"}
        </button>
      </form>
    </div>
  );
}