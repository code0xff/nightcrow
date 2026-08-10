import type { ComponentType, SVGProps } from "react";
import type { MobileView } from "../types";
import { FileTextIcon, ListIcon, TerminalIcon } from "./icons/navigation";

interface MobileDestination {
  key: MobileView;
  label: string;
  icon: ComponentType<SVGProps<SVGSVGElement>>;
}

const DESTINATIONS: MobileDestination[] = [
  // Labels name the region, not one of the things it can hold: the left one is
  // `status`/`log`/`tree`, so it is not always files, and the right one is a
  // diff, a whole file, or nothing at all.
  { key: "files", label: "Repo", icon: ListIcon },
  { key: "diff", label: "Content", icon: FileTextIcon },
  { key: "terminal", label: "Terminal", icon: TerminalIcon },
];

export function RepoMobileNav({
  view,
  onSelect,
}: {
  view: MobileView;
  onSelect: (view: MobileView) => void;
}) {
  return (
    <nav
      aria-label="Switch view"
      className="flex shrink-0 items-stretch border-t border-ink-700 bg-ink-900 md:hidden"
    >
      {DESTINATIONS.map(({ key, label, icon: Icon }) => (
        <button
          key={key}
          onClick={() => onSelect(key)}
          aria-current={view === key ? "page" : undefined}
          className={`flex min-h-11 flex-1 flex-col items-center justify-center gap-0.5 py-1 text-[11px] ${
            view === key
              ? "text-accent shadow-[inset_0_2px_0_0_var(--color-accent)]"
              : "text-ink-400"
          }`}
        >
          <Icon className="h-5 w-5" />
          {label}
        </button>
      ))}
    </nav>
  );
}
