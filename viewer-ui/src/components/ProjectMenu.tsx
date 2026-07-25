import { useEffect, useRef, useState } from "react";
import { ChevronIcon, PlusIcon, XIcon } from "./icons";
import type { Repo } from "../api";

export function ProjectMenu({
  repos,
  currentId,
  onSelect,
  onCloseProject,
  onOpenPicker,
  className = "",
}: {
  repos: Repo[];
  currentId: string | null;
  onSelect: (id: string) => void;
  onCloseProject: (id: string) => void;
  onOpenPicker: () => void;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const current = repos.find((r) => r.id === currentId);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open]);

  return (
    <div className={`relative ${className}`}>
      <button
        ref={triggerRef}
        onClick={() => setOpen((o) => !o)}
        aria-haspopup="menu"
        aria-expanded={open}
        title={current?.display_path ?? "Select a project"}
        className="flex max-w-[9rem] items-center gap-1 rounded-sm bg-ink-700 py-0.5 pl-2 pr-1 text-ink-50"
      >
        <span className="truncate">{current?.name ?? "No project"}</span>
        <ChevronIcon open={open} />
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div
            role="menu"
            className="absolute left-0 z-50 mt-1 max-h-[70vh] w-56 max-w-[80vw] overflow-y-auto rounded-md border border-ink-700 bg-ink-900 py-1 shadow-lg"
          >
            {repos.length === 0 && (
              <p className="px-3 py-1.5 text-ink-400">No projects open.</p>
            )}
            {repos.map((r) => (
              <div
                key={r.id}
                className={`flex items-center ${
                  r.id === currentId ? "bg-ink-700 text-ink-50" : "text-ink-200"
                }`}
              >
                <button
                  role="menuitem"
                  onClick={() => {
                    onSelect(r.id);
                    setOpen(false);
                  }}
                  title={r.display_path}
                  className="min-w-0 flex-1 truncate py-1.5 pl-3 pr-1 text-left hover:text-accent"
                >
                  {r.name}
                </button>
                <button
                  onClick={() => onCloseProject(r.id)}
                  aria-label={`close ${r.name}`}
                  title="Close project"
                  className="mr-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-removed"
                >
                  <XIcon className="h-3.5 w-3.5" />
                </button>
              </div>
            ))}
            <div className="my-1 border-t border-ink-800" />
            <button
              role="menuitem"
              onClick={() => {
                onOpenPicker();
                setOpen(false);
              }}
              className="flex w-full items-center gap-1 px-3 py-1.5 text-left text-ink-400 hover:text-ink-200"
            >
              <PlusIcon className="h-3.5 w-3.5" />
              open
            </button>
          </div>
        </>
      )}
    </div>
  );
}
