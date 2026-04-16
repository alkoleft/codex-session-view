import type { ViewerBackendMode } from "./backend-contract";
import { isTauri } from "./lib/tauri";

export type ViewerBackendConfig = {
  mode: ViewerBackendMode;
  remoteBaseUrl: string | null;
};

function normalizeMode(value: string | undefined): ViewerBackendMode | null {
  if (value === "tauri" || value === "remote") {
    return value;
  }

  return null;
}

function normalizeBaseUrl(value: string | undefined) {
  const trimmed = value?.trim();
  if (!trimmed) {
    return null;
  }

  return trimmed.replace(/\/+$/, "");
}

export function resolveViewerBackendConfig(env = import.meta.env): ViewerBackendConfig {
  const explicitMode = normalizeMode(env.VITE_VIEWER_BACKEND_MODE);
  const remoteBaseUrl = normalizeBaseUrl(env.VITE_VIEWER_REMOTE_BASE_URL);
  const mode =
    explicitMode ??
    (remoteBaseUrl ? "remote" : null) ??
    (isTauri() ? "tauri" : "remote");

  return {
    mode,
    remoteBaseUrl: mode === "remote" ? remoteBaseUrl : null,
  };
}
