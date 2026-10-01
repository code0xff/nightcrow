// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  ScreenScaleProvider,
  useScreenScale,
} from "./screenScale";

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.style.removeProperty("font-size");
});

function ScaleButton() {
  const { scale, setScale } = useScreenScale();
  return (
    <button onClick={() => setScale(130)}>
      {scale}%
    </button>
  );
}

describe("ScreenScaleProvider", () => {
  it("persists_the_browser_scale_and_applies_it_to_the_root_rem_size", () => {
    render(
      <ScreenScaleProvider>
        <ScaleButton />
      </ScreenScaleProvider>,
    );

    fireEvent.click(screen.getByRole("button"));

    expect(localStorage.getItem("nightcrow.viewer.scale")).toBe("130");
    expect(document.documentElement.style.fontSize).toBe("18.2px");
  });
});
