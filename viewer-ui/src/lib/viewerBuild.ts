/**
 * Telling the page that the server it is talking to is no longer the server it
 * came from.
 *
 * The bundle is split, and the chunks it loads on demand are named by content
 * hash — so a rebuild does not replace what this page is running, it deletes
 * the names this page would ask for next. What that produces is a failure at
 * the worst moment (`lib/chunkError.ts`), or nothing at all: a tab that opens
 * no preview and no new panel keeps running the old code indefinitely, with no
 * sign that a fix has already shipped.
 *
 * So the server stamps the build into the document it serves and names the same
 * build in every poll (`ViewerBootstrap.viewer_build`). Two facts about two
 * different moments — what this page is, and what the server has now —
 * which is what makes the comparison mean anything. Held in the module rather
 * than in state because it belongs to the document, not to a component: what it
 * answers is "is this page out of date", and only a reload can change that.
 *
 * The page is never reloaded for the reader. A tab that reloads itself takes
 * away whatever was being typed into a terminal, and being one build behind is
 * not urgent enough to interrupt anyone.
 */

import { dismissToast, toast } from "./toast";

const MESSAGE = "The viewer was updated on the server.";

/** The build this document was served as, or null when nothing said — an
 *  unstamped shell, which is how `npm run dev` serves it. */
let page: string | null = null;
/** The build already announced, so one poll every few seconds does not announce
 *  the same news repeatedly — and a second update after this one still does. */
let announced: string | null = null;
/** The notice standing, so it can be taken down if the condition it reports
 *  goes away. */
let notice: number | null = null;

/**
 * Whether `served` says the server has been replaced under this page.
 *
 * False while either side is unknown: an unstamped page has nothing to compare,
 * and a server that cannot name its own build says `null` on every poll —
 * reading that as a change would ask every page to reload forever.
 */
export function buildChanged(
  pageBuild: string | null,
  served: string | null,
): boolean {
  if (pageBuild === null || served === null) return false;
  return pageBuild !== served;
}

/** Take the build this document was served as, from the shell's stamp. */
export function notePageBuild(id: string | null): void {
  page = id;
}

/** Take the build named by a bootstrap response, and say so once when it is not
 *  the one this page came from. */
export function noteViewerBuild(served: string | null): void {
  if (!buildChanged(page, served)) {
    // The server is back on the build this page is running — a rollback, or a
    // rebuild that landed on the same output. The notice reported a condition
    // that has stopped being true, so it goes, and the next update is news
    // again even if it is the one already announced.
    if (served === page && announced !== null) {
      if (notice !== null) dismissToast(notice);
      announced = null;
      notice = null;
    }
    return;
  }
  if (announced === served) return;
  announced = served;
  notice = toast.info(MESSAGE, {
    sticky: true,
    action: { label: "Reload", run: () => window.location.reload() },
  });
}

/** Test seam: a fresh page load. */
export function resetViewerBuildForTest(): void {
  page = null;
  announced = null;
  notice = null;
}
