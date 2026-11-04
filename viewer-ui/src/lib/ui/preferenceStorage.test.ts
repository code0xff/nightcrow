// @vitest-environment happy-dom

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { stubLocalStorage } from "../shared/fakeStorage";
import { readPreference, writePreference } from "./preferenceStorage";

const KEY = "nightcrow.test.preference";

beforeEach(stubLocalStorage);
afterEach(stubLocalStorage);

describe("preferenceStorage", () => {
  it("preserves the stored string exactly", () => {
    const value = "  raw\u0000 preference  ";

    writePreference(KEY, value);

    expect(readPreference(KEY)).toBe(value);
  });

  it("returns null and ignores writes when storage is absent", () => {
    Reflect.deleteProperty(globalThis, "localStorage");

    expect(readPreference(KEY)).toBeNull();
    expect(() => writePreference(KEY, "value")).not.toThrow();
  });

  it("returns null and ignores writes when the storage getter throws", () => {
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      get() {
        throw new Error("storage disabled");
      },
    });

    expect(readPreference(KEY)).toBeNull();
    expect(() => writePreference(KEY, "value")).not.toThrow();
  });

  it("returns null when reading throws", () => {
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: {
        getItem() {
          throw new Error("storage disabled");
        },
        setItem() {},
      },
    });

    expect(readPreference(KEY)).toBeNull();
  });

  it("ignores write failures", () => {
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: {
        getItem() {
          return null;
        },
        setItem() {
          throw new Error("storage quota exceeded");
        },
      },
    });

    expect(() => writePreference(KEY, "value")).not.toThrow();
  });
});
