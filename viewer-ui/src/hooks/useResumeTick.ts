import { useEffect, useState } from "react";

// Bumped when the tab comes back after the device slept or the network
// returned. A mobile browser suspends the page and drops the in-flight poll,
// so an immediate re-poll on resume refreshes at once instead of waiting out
// the interval. The status stream reconnects on its own (EventSource retries).
export function useResumeTick() {
  const [resumeTick, setResumeTick] = useState(0);
  useEffect(() => {
    const wake = () => {
      if (document.visibilityState === "visible") setResumeTick((t) => t + 1);
    };
    document.addEventListener("visibilitychange", wake);
    window.addEventListener("online", wake);
    return () => {
      document.removeEventListener("visibilitychange", wake);
      window.removeEventListener("online", wake);
    };
  }, []);
  return resumeTick;
}