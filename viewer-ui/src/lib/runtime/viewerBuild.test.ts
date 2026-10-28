import { beforeEach, describe, expect, it } from "vitest";
import { dismissToast, subscribeToasts, type Toast } from "./toast";
import {
  buildChanged,
  notePageBuild,
  noteViewerBuild,
  resetViewerBuildForTest,
} from "./viewerBuild";

/** The toasts standing right now, read the way the Toaster reads them. */
function shown(): Toast[] {
  let current: Toast[] = [];
  subscribeToasts((toasts) => {
    current = toasts;
  })();
  return current;
}

function clearToasts(): void {
  shown().forEach((t) => dismissToast(t.id));
}

describe("buildChanged", () => {
  it("is changed when the server has a different build", () => {
    expect(buildChanged("aaaa1111", "bbbb2222")).toBe(true);
  });

  it("is unchanged when the build matches", () => {
    expect(buildChanged("aaaa1111", "aaaa1111")).toBe(false);
  });

  it("does not report a change when either build is unknown", () => {
    // Before the first response there is nothing to compare. A server that cannot report its build sends
    // null on every poll; treating that as a change would prompt for a refresh forever.
    expect(buildChanged(null, "bbbb2222")).toBe(false);
    expect(buildChanged("aaaa1111", null)).toBe(false);
    expect(buildChanged(null, null)).toBe(false);
  });
});

describe("noteViewerBuild", () => {
  beforeEach(() => {
    resetViewerBuildForTest();
    clearToasts();
  });

  it("does not announce the build this page came from", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild("aaaa1111");
    noteViewerBuild("aaaa1111");
    expect(shown()).toHaveLength(0);
  });

  it("leaves a refresh button when the server updates", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    const [notice] = shown();
    expect(notice.sticky).toBe(true);
    expect(notice.action?.label).toBe("Reload");
  });

  it("announces a new build even when it appears in the first response", () => {
    // If a deployment happens while the login screen is open, the first successful response is already a new build. The document
    // knows which build it came from, so do not infer that from the response.
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    expect(shown()).toHaveLength(1);
  });

  it("does not repeat the same notice on every poll", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    clearToasts();
    noteViewerBuild("bbbb2222");
    expect(shown()).toHaveLength(0);
  });

  it("announces the next update again", () => {
    // Another deployment may happen while the notice is dismissed, so it should be announced again.
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    clearToasts();
    noteViewerBuild("cccc3333");
    expect(shown()).toHaveLength(1);
  });

  it("a server with no build id changes nothing", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild(null);
    expect(shown()).toHaveLength(0);
  });

  it("clears the notice when the server returns to this page’s build", () => {
    // A rollback or rebuild with the same output. The notice represents a current condition, so
    // it should disappear when that condition is gone.
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    noteViewerBuild("aaaa1111");
    expect(shown()).toHaveLength(0);
  });

  it("announces again if redeployed after returning, even with the same build", () => {
    notePageBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    noteViewerBuild("aaaa1111");
    noteViewerBuild("bbbb2222");
    expect(shown()).toHaveLength(1);
  });

  it("a document without a build stamp makes no claim", () => {
    // When `npm run dev` serves the shell from Vite, there is no value to compare, so it stays silent.
    noteViewerBuild("bbbb2222");
    expect(shown()).toHaveLength(0);
  });
});
