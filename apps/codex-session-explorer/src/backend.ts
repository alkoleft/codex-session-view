import { resolveViewerBackendConfig } from "./backend-config";
import type { ViewerBackendClient, ViewerCommandHandlers } from "./backend-contract";
import { RemoteViewerBackendClient } from "./backend-remote";
import { TauriViewerBackendClient } from "./backend-tauri";
import type { SessionMetricsQuery, TailCursor } from "./backend-types";

export * from "./backend-contract";
export * from "./backend-types";

export function extractErrorMessage(error: unknown) {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  return "Unexpected viewer backend error.";
}

export function createViewerBackendClient(): ViewerBackendClient {
  const config = resolveViewerBackendConfig();

  if (config.mode === "remote") {
    return new RemoteViewerBackendClient({
      baseUrl: config.remoteBaseUrl,
    });
  }

  return new TauriViewerBackendClient();
}

const defaultViewerBackendClient = createViewerBackendClient();

export { defaultViewerBackendClient };

export function detectCodexHome() {
  return defaultViewerBackendClient.initialize().then((result) => ({
    detected_home: result.resolvedHome?.root ?? null,
  }));
}

export function initializeCodexHome() {
  return defaultViewerBackendClient.initialize().then((result) => {
    if (!result.initialization) {
      throw new Error("Viewer backend does not expose CODEX_HOME initialization.");
    }

    return result.initialization;
  });
}

export function listIndexedSessions(args = {}) {
  return defaultViewerBackendClient.listIndexedSessions(args);
}

export function listProjectMetricsCatalog() {
  return defaultViewerBackendClient.listProjectMetricsCatalog();
}

export function loadSessionPreview(sessionRef: string) {
  return defaultViewerBackendClient.loadSessionPreview(sessionRef);
}

export function loadSession(sessionRef: string) {
  return defaultViewerBackendClient.loadSession(sessionRef);
}

export function loadSessionMetrics(sessionRef: string) {
  return defaultViewerBackendClient.loadSessionMetrics(sessionRef);
}

export function queryProjectMetrics(query: SessionMetricsQuery) {
  return defaultViewerBackendClient.queryProjectMetrics(query);
}

export function loadProjectMetricsSessionDetailById(sessionId: string) {
  return defaultViewerBackendClient.loadProjectMetricsSessionDetailById(sessionId);
}

export function loadSessionPreviewById(sessionId: string) {
  return defaultViewerBackendClient.loadSessionPreviewById(sessionId);
}

export function tailSession(sessionRef: string, tailCursor: TailCursor) {
  return defaultViewerBackendClient.tailSession(sessionRef, tailCursor);
}

export function subscribeToViewerCommands(handlers: ViewerCommandHandlers) {
  return defaultViewerBackendClient.subscribeToViewerCommands(handlers);
}
