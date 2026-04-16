import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/tauri", () => ({
  isTauri: vi.fn(),
}));

import { resolveViewerBackendConfig } from "@/backend-config";
import { isTauri } from "@/lib/tauri";

describe("resolveViewerBackendConfig", () => {
  beforeEach(() => {
    vi.mocked(isTauri).mockReset();
  });

  it("defaults to tauri inside Tauri runtime", () => {
    vi.mocked(isTauri).mockReturnValue(true);

    expect(resolveViewerBackendConfig({} as ImportMetaEnv)).toEqual({
      mode: "tauri",
      remoteBaseUrl: null,
    });
  });

  it("defaults to remote outside Tauri runtime", () => {
    vi.mocked(isTauri).mockReturnValue(false);

    expect(resolveViewerBackendConfig({} as ImportMetaEnv)).toEqual({
      mode: "remote",
      remoteBaseUrl: null,
    });
  });

  it("prefers remote mode when a remote base URL is configured", () => {
    vi.mocked(isTauri).mockReturnValue(true);

    expect(
      resolveViewerBackendConfig({
        BASE_URL: "/",
        MODE: "test",
        DEV: true,
        PROD: false,
        SSR: false,
        VITE_VIEWER_REMOTE_BASE_URL: "https://viewer.example.test/",
      } as ImportMetaEnv),
    ).toEqual({
      mode: "remote",
      remoteBaseUrl: "https://viewer.example.test",
    });
  });

  it("keeps remote base url only in remote mode", () => {
    vi.mocked(isTauri).mockReturnValue(false);

    expect(
      resolveViewerBackendConfig({
        BASE_URL: "/",
        MODE: "test",
        DEV: true,
        PROD: false,
        SSR: false,
        VITE_VIEWER_REMOTE_BASE_URL: "https://viewer.example.test/",
      } as ImportMetaEnv),
    ).toEqual({
      mode: "remote",
      remoteBaseUrl: "https://viewer.example.test",
    });
  });
});
