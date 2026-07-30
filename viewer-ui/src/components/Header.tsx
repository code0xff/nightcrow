import { Mark } from "./Mark";
import { ProjectMenu } from "./ProjectMenu";
import { LogOutIcon, PlusIcon, XIcon } from "./icons";
import type { Repo } from "../api";

export interface HeaderProps {
  repos: Repo[];
  repo: string | null;
  setRepo: (id: string) => void;
  setPane: () => void;
  closeRepo: (id: string) => void;
  setPickerOpen: (open: boolean) => void;
  /** A clone is running on the server. Shown here rather than in the folder
   *  picker because the job outlives that dialog. */
  cloning: boolean;
  accent: { name: string };
  next: { name: string };
  cycle: () => void;
  draggingRepo: string | null;
  dragOverRepo: string | null;
  onRepoDragStart: (event: React.PointerEvent, id: string) => void;
  onRepoDragMove: (event: React.PointerEvent) => void;
  onRepoDragEnd: () => void;
}

export function Header({
  repos,
  repo,
  setRepo,
  setPane,
  closeRepo,
  setPickerOpen,
  cloning,
  accent,
  next,
  cycle,
  draggingRepo,
  dragOverRepo,
  onRepoDragStart,
  onRepoDragMove,
  onRepoDragEnd,
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
      <nav className="-my-[8.8px] hidden items-stretch self-stretch overflow-x-auto pl-1 md:flex">
        {repos.map((r) => (
          <div
            key={r.id}
            data-repo-id={r.id}
            onPointerDown={(event) => onRepoDragStart(event, r.id)}
            onPointerMove={onRepoDragMove}
            onPointerUp={onRepoDragEnd}
            onPointerCancel={onRepoDragEnd}
            onLostPointerCapture={onRepoDragEnd}
            className={`flex items-center border-r border-ink-700 whitespace-nowrap ${
              repos.length > 1 ? "touch-none" : ""
            } ${draggingRepo === r.id ? "opacity-60" : ""} ${
              dragOverRepo === r.id ? "bg-ink-800 ring-1 ring-inset ring-accent" : ""
            } ${
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
              data-tab-close
              title="Close project"
              aria-label={`close ${r.name}`}
              className="mr-1 flex h-5 w-5 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-removed"
            >
              <XIcon className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
      </nav>
      <button
        onClick={() => setPickerOpen(true)}
        title="Open a project"
        className="hidden shrink-0 items-center gap-1 rounded-sm px-2 py-0.5 text-ink-400 hover:text-ink-200 md:inline-flex"
      >
        <PlusIcon className="h-3.5 w-3.5" />
        open
      </button>
      {cloning && (
        <span
          role="status"
          title="A clone is running on the server"
          className="flex shrink-0 items-center gap-1.5 px-2 py-0.5 text-ink-400"
        >
          <span
            aria-hidden="true"
            className="h-1.5 w-1.5 animate-pulse rounded-full bg-accent"
          />
          Cloning…
        </span>
      )}
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
      <a
        href="/logout"
        title="Sign out"
        aria-label="sign out"
        className="ml-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-ink-200"
      >
        <LogOutIcon className="h-3.5 w-3.5" />
      </a>
    </header>
  );
}
