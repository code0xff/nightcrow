// @vitest-environment happy-dom

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { BusyLabel } from "./BusyLabel";

afterEach(cleanup);

describe("BusyLabel", () => {
  it("keeps the label when busy so the button does not change words", () => {
    const { container, rerender } = render(<BusyLabel busy={false}>Sign in</BusyLabel>);
    expect(container.textContent).toBe("Sign in");

    rerender(<BusyLabel busy={true}>Sign in</BusyLabel>);
    // The words are still there — only faded — so nothing is swapped in.
    expect(container.textContent).toBe("Sign in");
  });

  it("shows the spinner only while busy", () => {
    const { container, rerender } = render(<BusyLabel busy={false}>Open</BusyLabel>);
    expect(container.querySelector("svg")).toBeNull();

    rerender(<BusyLabel busy={true}>Open</BusyLabel>);
    expect(container.querySelector("svg")).not.toBeNull();
  });

  it("reserves the label's space when busy so the width holds", () => {
    // The label stays laid out (faded, not removed), which is what keeps the
    // button from resizing between idle and busy.
    const { container } = render(<BusyLabel busy={true}>Cloning</BusyLabel>);
    const faded = container.querySelector(".opacity-0");
    expect(faded?.textContent).toBe("Cloning");
  });
});
