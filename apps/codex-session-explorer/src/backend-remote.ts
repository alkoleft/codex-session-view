import type {
  IndexedSessionCatalogPage,
  ListSessionsArgs,
  LoadedSession,
  SessionPreview,
  TailCursor,
  TailResult,
} from "./backend-types";
import type {
  ViewerBackendClient,
  ViewerBackendCapabilities,
  ViewerCommandHandlers,
} from "./backend-contract";

const DEFAULT_CAPABILITIES: ViewerBackendCapabilities = {
  liveTail: false,
  needsCodexHome: false,
  showsLocalPaths: false,
  supportsViewerCommands: false,
  showsSessionIndexPath: false,
};

type RemoteViewerBackendClientOptions = {
  baseUrl: string | null;
  fetch?: typeof fetch;
};

function resolveRemoteBaseUrl(baseUrl: string | null) {
  if (baseUrl) {
    return baseUrl;
  }

  if (typeof window !== "undefined" && window.location.origin) {
    return window.location.origin;
  }

  return null;
}

async function readRemoteError(response: Response) {
  const fallback = `Remote viewer backend returned ${response.status}.`;
  const contentType = response.headers.get("content-type") ?? "";

  if (contentType.includes("application/json")) {
    try {
      const payload = (await response.json()) as { message?: unknown };
      if (typeof payload.message === "string" && payload.message.trim()) {
        return payload.message.trim();
      }
    } catch {
      return fallback;
    }
  }

  try {
    const text = (await response.text()).trim();
    return text || fallback;
  } catch {
    return fallback;
  }
}

export class RemoteViewerBackendClient implements ViewerBackendClient {
  public readonly capabilities = DEFAULT_CAPABILITIES;

  public readonly mode = "remote" as const;

  private readonly baseUrl: string | null;

  private readonly fetchImpl: typeof fetch;

  public constructor(options: RemoteViewerBackendClientOptions) {
    this.baseUrl = resolveRemoteBaseUrl(options.baseUrl);
    this.fetchImpl = options.fetch ?? fetch;
  }

  public async initialize() {
    if (!this.baseUrl) {
      throw new Error(
        "Remote backend mode requires VITE_VIEWER_REMOTE_BASE_URL or a browser origin.",
      );
    }

    return {
      mode: this.mode,
      status: "ready" as const,
      capabilities: this.capabilities,
      resolvedHome: null,
      initialization: null,
      statusMessage: "Remote backend подключён. Выберите сессию из каталога.",
    };
  }

  public listIndexedSessions(args: ListSessionsArgs = {}) {
    return this.postJson<IndexedSessionCatalogPage>("/api/viewer/list_indexed_sessions", {
      limit: args.limit ?? 40,
      query: args.query?.trim() || null,
      exact_session_id: args.exactSessionId?.trim() || null,
      cursor: args.cursor ?? null,
    });
  }

  public loadSessionPreview(sessionRef: string) {
    return this.postJson<SessionPreview>("/api/viewer/load_session_preview", {
      session_ref: sessionRef,
      event_limit: 80,
    });
  }

  public loadSession(sessionRef: string) {
    return this.postJson<LoadedSession>("/api/viewer/load_session", {
      session_ref: sessionRef,
      text_limit: 120,
    });
  }

  public loadSessionPreviewById(sessionId: string) {
    return this.postJson<SessionPreview>("/api/viewer/load_session_preview_by_id", {
      session_id: sessionId,
      event_limit: 80,
    });
  }

  public async subscribeToViewerCommands(handlers: ViewerCommandHandlers) {
    void handlers;
    return () => {};
  }

  public async tailSession(sessionRef: string, tailCursor: TailCursor): Promise<TailResult> {
    void sessionRef;
    void tailCursor;
    throw new Error("Live tail пока не поддерживается во внешнем viewer backend.");
  }

  private async postJson<T>(path: string, body: Record<string, unknown>): Promise<T> {
    if (!this.baseUrl) {
      throw new Error("Remote viewer backend base URL is not configured.");
    }

    const response = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify(body),
    });

    if (!response.ok) {
      throw new Error(await readRemoteError(response));
    }

    return (await response.json()) as T;
  }
}
