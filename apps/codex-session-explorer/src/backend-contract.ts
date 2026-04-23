import type {
  IndexedSessionCatalogPage,
  InitializeCodexHomeResponse,
  ListSessionsArgs,
  LoadedSession,
  ProjectMetricsResponse,
  ResolvedCodexHome,
  SessionMetrics,
  SessionMetricsQuery,
  SessionPreview,
  TailCursor,
  TailResult,
} from "./backend-types";

export type ViewerBackendMode = "tauri" | "remote";

export type ViewerBackendCapabilities = {
  liveTail: boolean;
  needsCodexHome: boolean;
  showsLocalPaths: boolean;
  supportsViewerCommands: boolean;
  showsSessionIndexPath: boolean;
};

export type ViewerBootstrapResult = {
  mode: ViewerBackendMode;
  status: "ready" | "needs_home";
  capabilities: ViewerBackendCapabilities;
  resolvedHome: ResolvedCodexHome | null;
  initialization: InitializeCodexHomeResponse | null;
  statusMessage: string;
};

export type ViewerCommandHandlers = {
  onOpenSession: (sessionRef: string) => void;
  onClearSession: () => void;
  onError: (message: string) => void;
};

export interface ViewerBackendClient {
  capabilities: ViewerBackendCapabilities;
  mode: ViewerBackendMode;
  initialize: () => Promise<ViewerBootstrapResult>;
  listIndexedSessions: (args?: ListSessionsArgs) => Promise<IndexedSessionCatalogPage>;
  loadSession: (sessionRef: string) => Promise<LoadedSession>;
  loadSessionMetrics: (sessionRef: string) => Promise<SessionMetrics>;
  queryProjectMetrics: (query: SessionMetricsQuery) => Promise<ProjectMetricsResponse>;
  loadSessionPreview: (sessionRef: string) => Promise<SessionPreview>;
  loadSessionPreviewById: (sessionId: string) => Promise<SessionPreview>;
  subscribeToViewerCommands: (handlers: ViewerCommandHandlers) => Promise<() => void>;
  tailSession: (sessionRef: string, tailCursor: TailCursor) => Promise<TailResult>;
}
