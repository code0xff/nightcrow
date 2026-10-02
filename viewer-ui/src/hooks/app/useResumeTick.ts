import { useEffect, useState } from "react";

// Wake events trigger immediate polling after suspension or network loss.
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
