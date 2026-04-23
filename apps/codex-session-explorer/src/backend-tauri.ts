import type {
  DetectCodexHomeResponse,
  IndexedSessionCatalogPage,
  InitializeCodexHomeResponse,
  ListSessionsArgs,
  LoadedSession,
  ProjectMetricsResponse,
  SessionMetrics,
  SessionMetricsQuery,
  SessionPreview,
  TailCursor,
  TailResult,
} from "./backend-types";
import type {
  ViewerBackendClient,
  ViewerBackendCapabilities,
  ViewerCommandHandlers,
} from "./backend-contract";
import { isTauri, trackedInvoke } from "@/lib/tauri";

const OPEN_SESSION_EVENT = "viewer:open-session";
const CLEAR_SESSION_EVENT = "viewer:clear-session";
const DEFAULT_CAPABILITIES: ViewerBackendCapabilities = {
  liveTail: true,
  needsCodexHome: true,
  showsLocalPaths: true,
  supportsViewerCommands: true,
  showsSessionIndexPath: true,
};

type OpenSessionEventPayload = {
  sessionRef?: string;
  session_ref?: string;
} | string;

function readSessionRef(payload: OpenSessionEventPayload) {
  if (typeof payload === "string") {
    return payload.trim();
  }

  if (typeof payload?.sessionRef === "string") {
    return payload.sessionRef.trim();
  }

  if (typeof payload?.session_ref === "string") {
    return payload.session_ref.trim();
  }

  return "";
}

async function invokeBackend<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw new Error("Tauri viewer backend доступен только внутри Tauri runtime.");
  }

  return trackedInvoke<T>(command, args);
}

async function detectCodexHome() {
  return invokeBackend<DetectCodexHomeResponse>("detect_codex_home");
}

async function initializeCodexHome() {
  return invokeBackend<InitializeCodexHomeResponse>("initialize_codex_home");
}

export class TauriViewerBackendClient implements ViewerBackendClient {
  public readonly capabilities = DEFAULT_CAPABILITIES;

  public readonly mode = "tauri" as const;

  public async initialize() {
    const detected = await detectCodexHome();

    if (!detected.detected_home) {
      return {
        mode: this.mode,
        status: "needs_home" as const,
        capabilities: this.capabilities,
        resolvedHome: null,
        initialization: null,
        statusMessage: "CODEX_HOME не найден; viewer не может открыть выбранную сессию.",
      };
    }

    const initialized = await initializeCodexHome();

    return {
      mode: this.mode,
      status: "ready" as const,
      capabilities: this.capabilities,
      resolvedHome: initialized.resolved_home,
      initialization: initialized,
      statusMessage:
        "Backend инициализирован. Жду выбора сессии из dialog picker, отдельного окна или команды.",
    };
  }

  public async listIndexedSessions(args: ListSessionsArgs = {}) {
    const query = args.query && args.query.trim().length > 0 ? args.query.trim() : null;
    const exactSessionId =
      args.exactSessionId && args.exactSessionId.trim().length > 0
        ? args.exactSessionId.trim()
        : null;

    return invokeBackend<IndexedSessionCatalogPage>("list_indexed_sessions", {
      limit: args.limit ?? 40,
      query,
      exactSessionId,
      cursor: args.cursor ?? null,
    });
  }

  public loadSessionPreview(sessionRef: string) {
    return invokeBackend<SessionPreview>("load_session_preview", {
      sessionRef,
      eventLimit: 80,
    });
  }

  public loadSession(sessionRef: string) {
    return invokeBackend<LoadedSession>("load_session", {
      sessionRef,
      textLimit: 120,
    });
  }

  public loadSessionMetrics(sessionRef: string) {
    return invokeBackend<SessionMetrics>("load_session_metrics", {
      sessionRef,
      textLimit: 120,
    });
  }

  public queryProjectMetrics(query: SessionMetricsQuery) {
    return invokeBackend<ProjectMetricsResponse>("query_project_metrics", {
      query,
    });
  }

  public loadSessionPreviewById(sessionId: string) {
    return invokeBackend<SessionPreview>("load_session_preview_by_id", {
      sessionId,
      eventLimit: 80,
    });
  }

  public tailSession(sessionRef: string, tailCursor: TailCursor) {
    return invokeBackend<TailResult>("tail_session", {
      sessionRef,
      tailCursor,
    });
  }

  public async subscribeToViewerCommands(handlers: ViewerCommandHandlers) {
    const { listen } = await import("@tauri-apps/api/event");

    const unlistenOpen = await listen<OpenSessionEventPayload>(OPEN_SESSION_EVENT, (event) => {
      const sessionRef = readSessionRef(event.payload);

      if (!sessionRef) {
        handlers.onError("Внешняя команда открытия пришла без sessionRef.");
        return;
      }

      handlers.onOpenSession(sessionRef);
    });

    const unlistenClear = await listen(CLEAR_SESSION_EVENT, () => {
      handlers.onClearSession();
    });

    return () => {
      void unlistenOpen();
      void unlistenClear();
    };
  }
}
