import { Mark } from "./Mark";
import { ProjectMenu } from "./ProjectMenu";
import { PlusIcon, XIcon } from "../icons";
import type { Repo } from "../api";

export interface HeaderProps {
  repos: Repo[];
  repo: string | null;
  setRepo: (id: string) => void;
  setPane: () => void;
  closeRepo: (id: string) => void;
  setPickerOpen: (open: boolean) => void;
  accent: { name: string };
  next: { name: string };
  cycle: () => void;
}

export function Header({
  repos,
  repo,
  setRepo,
  setPane,
  closeRepo,
  setPickerOpen,
  accent,
  next,
  cycle,
}: HeaderProps) {
  return (
    <header className="flex items-center gap-2 border-b border-ink-700 bg-ink-900 px-[12.8px] py-[8.8px]">
      <Mark className="h-[22px] w-[22px] shrink-0" />
      <span className="text-[16px] font-medium tracking-[0.04em] text-ink-50">nightcrow</span>
      <span className="hidden font-sans text-[10px] uppercase tracking-[0.18em] text-ink-400 sm:inline">
        web viewer
      </span>
      <ProjectMenu
        className="md:hidden"
        repos={repos}
        currentId={repo}
        onSelect={(id) => {
          setRepo(id);
          setPane();
        }}
        onCloseProject={closeRepo}
        onOpenPicker={() => setPickerOpen(true)}
      />
      {/* Editor tabs, after VS Code's: square, touching, and stretched to the
          full height of the bar they sit in — a tab is a tab because it fills
          its strip, not because it is a labelled box. The negative margins eat
          the header's padding to reach that height, and the active one takes
          the body colour so it reads as the near edge of the content below.
          The accent marker is an inset shadow rather than a border, which
          would shift the label down by its own width.

          VS Code also lets the active tab overlap the rule under the bar, so
          the two areas merge outright. Not done here: this strip scrolls
          sideways when enough projects are open, and a scroll container clips
          both axes — the overlap would be cut off and could raise a vertical
          scrollbar besides. */}
      <nav className="-my-[8.8px] hidden items-stretch self-stretch overflow-x-auto pl-1 md:flex">
        {repos.map((r) => (
          <div
            key={r.id}
            className={`flex items-center border-r border-ink-700 whitespace-nowrap ${
              r.id === repo
                ? "bg-ink-950 text-ink-50 shadow-[inset_0_2px_0_0_var(--color-accent)]"
                : "text-ink-400 hover:bg-ink-850 hover:text-ink-200"
            }`}
            title={r.display_path}
          >
            <button
              onClick={() => {
                setRepo(r.id);
                setPane();
              }}
              className="self-stretch pl-3 pr-1"
            >
              {r.name}
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                closeRepo(r.id);
              }}
              title="Close project"
              aria-label={`close ${r.name}`}
              className="mr-1 flex h-5 w-5 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-removed"
            >
              <XIcon className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
      </nav>
      {/* The plus is the same drawn mark the terminal panel's add button uses,
          not the `+` character, so the app has one plus rather than two that
          disagree on weight. Sized to the label beside it — the convention the
          project tabs' close glyph already follows — rather than to the 16px
          of an icon-only control. */}
      <button
        onClick={() => setPickerOpen(true)}
        title="Open a project"
        className="hidden shrink-0 items-center gap-1 rounded-sm px-2 py-0.5 text-ink-400 hover:text-ink-200 md:inline-flex"
      >
        <PlusIcon className="h-3.5 w-3.5" />
        open
      </button>
      {/* Cycles rather than opening a picker, matching the TUI's
          `<prefix> p`. The swatch is the current accent, so the control
          doubles as the indicator. */}
      <button
        onClick={cycle}
        title={`Accent: ${accent.name} (click for ${next.name})`}
        aria-label={`accent colour: ${accent.name}, click for ${next.name}`}
        className="ml-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-sm"
      >
        <span
          aria-hidden="true"
          className="h-3 w-3 rounded-full bg-accent ring-1 ring-ink-600"
        />
      </button>
      <a href="/logout" className="pl-2 text-ink-400 hover:text-ink-200">
        sign out
      </a>
    </header>
  );
}