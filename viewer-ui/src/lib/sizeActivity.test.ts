import { describe, expect, it, vi } from "vitest";
import { applySizeOwnerUpdate, shouldRequestSizeForActivity } from "./sizeActivity";

describe("size ownership activity", () => {
  it("retries_each_live_activity_even_when_the_render_snapshot_is_stale", () => {
    const serverOwned = { current: true };
    const lastGeneration = { current: "15" };
    const renderSnapshot = true;
    const refit = vi.fn();

    applySizeOwnerUpdate(serverOwned, lastGeneration, true, "17", refit);

    expect(serverOwned.current).toBe(true);
    expect(renderSnapshot).toBe(true);
    expect(shouldRequestSizeForActivity("live")).toBe(true);
    expect(shouldRequestSizeForActivity("live")).toBe(true);
    expect(shouldRequestSizeForActivity("reconnecting")).toBe(false);
    expect(refit).toHaveBeenCalledTimes(1);
    expect(lastGeneration.current).toBe("17");
  });

  it("clears_old_fit_requests_once_on_acquisition_but_not_same_owner_acks", () => {
    const serverOwned = { current: false };
    const lastGeneration = { current: "2" };
    const refit = vi.fn();

    applySizeOwnerUpdate(serverOwned, lastGeneration, true, "3", refit);
    applySizeOwnerUpdate(serverOwned, lastGeneration, true, "3", refit);

    expect(serverOwned.current).toBe(true);
    expect(refit).toHaveBeenCalledTimes(1);
    expect(lastGeneration.current).toBe("3");
  });

  it("forces_a_refit_for_a_false_true_handoff_even_when_react_batches_to_true", () => {
    const serverOwned = { current: true };
    const lastGeneration = { current: "4" };
    const refit = vi.fn();
    let renderedOwner = true;

    applySizeOwnerUpdate(serverOwned, lastGeneration, false, "5", refit);
    renderedOwner = false;
    applySizeOwnerUpdate(serverOwned, lastGeneration, true, "5", refit);
    renderedOwner = true;

    expect(renderedOwner).toBe(true);
    expect(refit).toHaveBeenCalledTimes(1);
    expect(serverOwned.current).toBe(true);
  });
});
