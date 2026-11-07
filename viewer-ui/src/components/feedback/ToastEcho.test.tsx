// @vitest-environment happy-dom

import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { dismissToast, toast } from "../../lib/runtime/toast";
import { ToastEcho } from "./ToastEcho";

const raised: number[] = [];

afterEach(() => {
  cleanup();
  act(() => raised.splice(0).forEach(dismissToast));
});

describe("ToastEcho", () => {
  it("repeats_the_newest_error_toast_as_an_alert", () => {
    render(<ToastEcho />);
    expect(screen.queryByRole("alert")).toBeNull();
    act(() => {
      raised.push(toast.error("Microphone access is blocked."));
    });
    expect(screen.getByRole("alert").textContent).toBe("Microphone access is blocked.");
    act(() => {
      raised.push(toast.error("Image must be 10 MiB or smaller."));
    });
    expect(screen.getByRole("alert").textContent).toBe("Image must be 10 MiB or smaller.");
  });

  it("announces_an_error_raised_again_even_behind_a_newer_one", () => {
    render(<ToastEcho />);
    act(() => {
      raised.push(toast.error("A"));
      raised.push(toast.error("B"));
    });
    expect(screen.getByRole("alert").textContent).toBe("B");
    act(() => {
      toast.error("A");
    });
    expect(screen.getByRole("alert").textContent).toBe("A");
  });

  it("does_not_bring_back_an_older_error_when_the_newer_one_is_dismissed", () => {
    render(<ToastEcho />);
    let older = 0;
    let newer = 0;
    act(() => {
      older = toast.error("older");
      newer = toast.error("newer");
    });
    raised.push(older);
    act(() => dismissToast(newer));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("ignores_toasts_that_are_not_errors_and_clears_when_dismissed", () => {
    render(<ToastEcho />);
    act(() => {
      raised.push(toast.info("Saved."));
    });
    expect(screen.queryByRole("alert")).toBeNull();
    let id = 0;
    act(() => {
      id = toast.error("Could not attach image.");
    });
    expect(screen.getByRole("alert")).toBeTruthy();
    act(() => dismissToast(id));
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
