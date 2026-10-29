import { beforeEach, describe, expect, it, vi } from "vitest";

const pushApi = vi.hoisted(() => ({
  key: vi.fn(),
  subscribe: vi.fn(),
  unsubscribe: vi.fn(),
  status: vi.fn(),
  test: vi.fn(),
}));
vi.mock("../../api/push", () => ({ pushApi }));

import {
  base64UrlToBytes,
  disablePush,
  enablePush,
  readPushState,
  type PushEnvironment,
} from "./webPush";

function fakeSubscription(endpoint: string) {
  return {
    endpoint,
    unsubscribe: vi.fn().mockResolvedValue(true),
    toJSON: () => ({ endpoint, keys: { p256dh: "k", auth: "a" } }),
  };
}

function env(overrides: Partial<PushEnvironment> = {}, existing: unknown = null) {
  let current = existing;
  const pushManager = {
    getSubscription: vi.fn(async () => current),
    subscribe: vi.fn(async () => {
      current = fakeSubscription("https://push.example/new");
      return current;
    }),
  };
  const environment: PushEnvironment = {
    secure: true,
    serviceWorker: { ready: Promise.resolve({ pushManager }) } as unknown as ServiceWorkerContainer,
    hasPushManager: true,
    permission: () => "default",
    requestPermission: vi.fn(async () => "granted" as NotificationPermission),
    isIos: false,
    standalone: false,
    ...overrides,
  };
  return { environment, pushManager };
}

beforeEach(() => {
  Object.values(pushApi).forEach((fn) => fn.mockReset());
});

describe("push state", () => {
  it("names what blocks push before asking the server anything", async () => {
    expect(await readPushState(env({ secure: false }).environment)).toBe("insecure");
    expect(await readPushState(env({ isIos: true }).environment)).toBe("needs-install");
    expect(await readPushState(env({ serviceWorker: undefined }).environment)).toBe(
      "unsupported",
    );
    expect(await readPushState(env({ permission: () => "denied" }).environment)).toBe("denied");
    expect(pushApi.status).not.toHaveBeenCalled();
  });

  it("is on only while the daemon still holds the subscription", async () => {
    const held = fakeSubscription("https://push.example/1");
    pushApi.status.mockResolvedValueOnce(true);
    expect(await readPushState(env({}, held).environment)).toBe("on");

    const dropped = fakeSubscription("https://push.example/2");
    pushApi.status.mockResolvedValueOnce(false);
    expect(await readPushState(env({}, dropped).environment)).toBe("off");
    expect(dropped.unsubscribe).toHaveBeenCalled();
  });

  it("an installed iOS page is not blocked", async () => {
    expect(await readPushState(env({ isIos: true, standalone: true }).environment)).toBe("off");
  });
});

describe("turning push on and off", () => {
  it("asks permission, subscribes with the daemon key, and registers it", async () => {
    pushApi.key.mockResolvedValue("BAEC");
    pushApi.subscribe.mockResolvedValue({ subscribed: true });
    const { environment, pushManager } = env();

    expect(await enablePush(environment)).toBe("on");
    expect(environment.requestPermission).toHaveBeenCalled();
    const options = pushManager.subscribe.mock.calls[0] as unknown as [PushSubscriptionOptionsInit];
    expect(options[0].userVisibleOnly).toBe(true);
    expect(Array.from(options[0].applicationServerKey as Uint8Array)).toEqual([4, 1, 2]);
    expect(pushApi.subscribe).toHaveBeenCalledWith({
      endpoint: "https://push.example/new",
      keys: { p256dh: "k", auth: "a" },
    });
  });

  it("stops at a refusal and at a daemon without a key", async () => {
    const refused = env({ requestPermission: async () => "denied" }).environment;
    expect(await enablePush(refused)).toBe("denied");
    pushApi.key.mockRejectedValue(new Error("503"));
    expect(await enablePush(env().environment)).toBe("unavailable");
    expect(pushApi.subscribe).not.toHaveBeenCalled();
  });

  it("turning off tells the daemon and the browser", async () => {
    const held = fakeSubscription("https://push.example/1");
    pushApi.unsubscribe.mockResolvedValue({ subscribed: false });
    expect(await disablePush(env({}, held).environment)).toBe("off");
    expect(pushApi.unsubscribe).toHaveBeenCalledWith("https://push.example/1");
    expect(held.unsubscribe).toHaveBeenCalled();
  });
});

it("decodes an unpadded base64url key", () => {
  expect(Array.from(base64UrlToBytes("-_8"))).toEqual([251, 255]);
});
