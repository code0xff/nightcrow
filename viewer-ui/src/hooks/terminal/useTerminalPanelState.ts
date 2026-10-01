import { useState } from "react";
import type { LinkState } from "../../lib/attachStatus";

export function useTerminalPanelState() {
  const [pending, setPending] = useState<number | null>(null);
  const [link, setLink] = useState<LinkState>("connecting");
  const [replayLeft, setReplayLeft] = useState(0);
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const [zoomed, setZoomed] = useState<number | null>(null);
  const [titles, setTitles] = useState<Record<number, string>>({});
  const [closing, setClosing] = useState<number | null>(null);
  const [ownsSize, setOwnsSize] = useState(true);

  return {
    pending,
    setPending,
    link,
    setLink,
    replayLeft,
    setReplayLeft,
    panes,
    setPanes,
    active,
    setActive,
    zoomed,
    setZoomed,
    titles,
    setTitles,
    closing,
    setClosing,
    ownsSize,
    setOwnsSize,
  };
}
