import { expect, test, type Page } from "@playwright/test";
import { installApiMocks, type ApiMockOptions, type ApiMocks } from "./api";
import type { Repo, Status } from "../src/api/types";

const FIXED_TIME = new Date("2026-10-25T00:00:00Z");
const status: Status = {
  branch: "dev",
  head: "9a3bc2c",
  files: [
    { path: "src/main.rs", index: "M", worktree: " " },
    { path: "src/lib.rs", index: " ", worktree: "M" },
    { path: "README.md", index: " ", worktree: "M" },
  ],
  truncated: false,
};

let mocks: ApiMocks | undefined;

test.afterEach(async () => {
  mocks?.finishPending();
  await mocks?.assertClean();
  mocks = undefined;
});

async function openViewer(page: Page, options: ApiMockOptions = {}) {
  await page.clock.setFixedTime(FIXED_TIME);
  await page.addInitScript(() => {
    const style = document.createElement("style");
    style.textContent =
      "*,*::before,*::after{animation:none!important;transition:none!important;" +
      "caret-color:transparent!important;scroll-behavior:auto!important}" +
      ".xterm-screen{visibility:hidden!important}";
    document.documentElement.append(style);
    Object.defineProperty(crypto, "randomUUID", {
      configurable: true,
      value: () => "visual-test-fixed-viewer-id",
    });
  });
  mocks = await installApiMocks(page, { status, ...options });
  await page.routeWebSocket("**/ws/term**", (socket) => {
    socket.send(JSON.stringify({ type: "hello", client: 1, panes: 1 }));
    socket.send(JSON.stringify({
      type: "created", pane: 1, rows: 24, cols: 80, client: 1, title: "shell",
    }));
    socket.send(JSON.stringify({ type: "replay_complete" }));
  });
  await page.goto("/");
  await expect(page.getByRole("link", { name: "sign out" })).toBeVisible();
  if ((page.viewportSize()?.width ?? 0) >= 768)
    await expect(page.locator("[data-terminal-panel] .xterm-screen")).toBeAttached();
  await page.evaluate(() => document.fonts.ready);
}

async function capture(page: Page, name: string) {
  await expect(page).toHaveScreenshot(name + ".png", {
    maxDiffPixels: 0,
    animations: "disabled",
    caret: "hide",
  });
}

function repos(): Repo[] {
  const titles = [
    "nightcrow",
    "api",
    "workspace-config",
    "very-long-project-name-for-title-scrolling",
    "ui",
    "cross-platform-terminal-integration",
    "docs",
    "extensions",
  ];
  return titles.map((name, index) => ({
    id: "r" + (index + 1),
    name,
    display_path: "~/work/" + name,
  }));
}

test("desktop project tabs keep equal title widths for long and short names", async ({ page }) => {
  await openViewer(page, {
    bootstrap: { repos: repos(), active_repo: "r1", maximized: {}, last_view: {} },
  });
  const tabs = page.locator("[data-repo-id] .nc-title");
  await expect(tabs).toHaveCount(8);
  const widths = await tabs.evaluateAll((elements) =>
    elements.map((element) => element.getBoundingClientRect().width),
  );
  expect(new Set(widths.map((width) => Math.round(width)))).toEqual(new Set([105]));
  await expect(
    page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
  ).resolves.toBe(true);
  await capture(page, "desktop-project-tabs");
});

test("the busy dot overlays the real folder picker Open button", async ({ page }) => {
  await openViewer(page, { deferOpen: true });
  await page.getByRole("button", { name: "open", exact: true }).click();
  await expect(page.getByText("/Users/code0xff/code").first()).toBeVisible();
  await page.getByRole("button", { name: "Open", exact: true }).click();
  await expect(page.getByRole("button", { name: "Open", exact: true }))
    .toHaveAttribute("aria-busy", "true");
  await expect(page.locator('[aria-busy="true"] [data-busy]')).toBeVisible();
  await capture(page, "busy-open-project");
});

test("maximizing the file pane keeps the viewport and toolbar visible", async ({ page }) => {
  await openViewer(page);
  await page.locator("button").filter({ hasText: "src/main.rs" }).first().click();
  await expect(
    page.getByRole("button", { name: "Open the whole file at this change" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Open the whole file at this change" }).click();
  await expect(page.getByRole("button", { name: "Maximize the file pane" })).toBeVisible();
  await page.getByRole("button", { name: "Maximize the file pane" }).click();
  const restore = page.getByRole("button", { name: "Restore the layout" });
  await expect(restore).toBeInViewport();
  const geometry = await page
    .locator('[data-terminal-panel] button[aria-label="New terminal"]')
    .evaluate((button) => {
      const panel = button.closest("[data-terminal-panel]");
      const panelBox = panel!.getBoundingClientRect();
      const buttonBox = button.getBoundingClientRect();
      return {
        panelHeight: panelBox.height,
        panelBottom: panelBox.bottom,
        buttonTop: buttonBox.top,
        overflow: getComputedStyle(panel!).overflow,
      };
    });
  expect(geometry.panelHeight).toBeLessThanOrEqual(2);
  expect(geometry.buttonTop).toBeGreaterThanOrEqual(geometry.panelBottom);
  expect(geometry.overflow).toBe("hidden");
  await expect(
    page.evaluate(() => document.documentElement.scrollHeight === innerHeight),
  ).resolves.toBe(true);
  await capture(page, "maximized-file-pane");
});

test("a non-repository status is shown in the repository view", async ({ page }) => {
  await openViewer(page, {
    unavailable: { reason: "not a git repository" },
    status: { files: [], truncated: false },
  });
  await expect(
    page.getByText("Not a git repository. Run git init here to track changes.").first(),
  ).toBeVisible();
  await capture(page, "unavailable-repository");
});

test("the mobile compose dialog keeps image upload available while uploading", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openViewer(page, { deferImage: true });
  await page.getByRole("button", { name: "Terminal" }).click();
  await expect(page.locator("[data-terminal-panel] .xterm-screen")).toBeAttached();
  const compose = page.getByRole("button", {
    name: "Write a message to the active terminal",
  });
  await expect(compose).toBeVisible();
  await compose.click();
  await expect(page.getByRole("dialog", { name: "Send to shell" })).toBeVisible();
  await page.getByLabel("Choose an image").setInputFiles({
    name: "evidence.png",
    mimeType: "image/png",
    buffer: Buffer.from("visual fixture"),
  });
  await expect(page.getByText("Uploading image…")).toBeVisible();
  await expect(page.getByRole("button", { name: "Attach image" })).toBeDisabled();
  await capture(page, "mobile-compose-image-upload");
});

test("the diff skeleton remains visible while a new diff request is pending", async ({ page }) => {
  await openViewer(page, { deferDiffPath: "src/lib.rs" });
  await page.locator("button").filter({ hasText: "src/lib.rs" }).first().click();
  await expect(page.locator('[aria-label="Loading"][aria-busy="true"]')).toBeVisible();
  await capture(page, "diff-skeleton");
});

test("the file skeleton remains visible while a whole-file request is pending", async ({ page }) => {
  await openViewer(page, { deferFilePath: "src/main.rs" });
  await page.locator("button").filter({ hasText: "src/main.rs" }).first().click();
  await page.getByRole("button", { name: "Open the whole file at this change" }).click();
  await expect(page.locator('[aria-label="Loading"][aria-busy="true"]')).toBeVisible();
  await capture(page, "file-skeleton");
});
