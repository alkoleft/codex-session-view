import {
  startTransition,
  useCallback,
  useDeferredValue,
  useEffect,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  FolderSearch2,
  RadioTower,
  RefreshCcw,
  Target,
  Unplug,
} from "lucide-react";

import {
  detectCodexHome,
  extractErrorMessage,
  initializeCodexHome,
  listIndexedSessions,
  loadSession,
  loadSessionPreviewById,
  loadSessionPreview,
  tailSession,
  type IndexedSessionSummary,
  type LoadedSession,
  type ResolvedCodexHome,
  type SessionDiagnostic,
  type SessionPreview,
  type TailCursor,
  type TimelineItem,
} from "./backend";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { SessionEventList } from "@/components/session-event-list";
import { AgentsPanel } from "@/components/agents-panel";
import {
  buildAgentGraphViewModel,
  preferredAgentThreadId,
} from "@/components/agent-thread-view-model";
import { isTauri } from "@/lib/tauri";
import { cn } from "@/lib/utils";

type BootState = "booting" | "needs_home" | "ready" | "error";

type OpenSessionEventPayload = {
  sessionRef?: string;
  session_ref?: string;
} | string;

const PANEL_CARD_CLASS = "min-h-0 gap-0 border-border bg-card shadow-none";
const SURFACE_CARD_CLASS = "border-border bg-background shadow-none";
const OPEN_SESSION_EVENT = "viewer:open-session";
const CLEAR_SESSION_EVENT = "viewer:clear-session";
const SESSIONS_PAGE_SIZE = 50;
const LIVE_TAIL_POLL_MS = 2500;
const SUMMARY_TEXT_PREVIEW_LIMIT = 220;
const SESSION_ID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function extractExactSessionId(query: string | undefined) {
  const trimmed = query?.trim();
  if (!trimmed || !SESSION_ID_PATTERN.test(trimmed)) {
    return null;
  }
  return trimmed;
}

function formatDateTime(value: string | null) {
  if (!value) {
    return "n/a";
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function formatTokenCount(value: number | null) {
  if (value == null) {
    return "n/a";
  }

  return new Intl.NumberFormat("ru-RU").format(value);
}

function formatBoolValue(value: boolean | null) {
  if (value == null) {
    return "n/a";
  }

  return value ? "yes" : "no";
}

type SessionSummaryField = {
  label: string;
  value: string;
  monospace?: boolean;
};

type ParsedSessionSource = {
  label: string | null;
  parentThreadId: string | null;
  depth: string | null;
};

function shouldCollapseSummaryText(value: string) {
  return value.length > SUMMARY_TEXT_PREVIEW_LIMIT || value.includes("\n");
}

function truncateSummaryText(value: string) {
  if (!shouldCollapseSummaryText(value)) {
    return value;
  }

  const clipped = value.slice(0, SUMMARY_TEXT_PREVIEW_LIMIT).trimEnd();
  return `${clipped}…`;
}

function isCompactThreadSummary(value: string) {
  const trimmed = value.trim();
  return trimmed.length > 0 && trimmed.length <= 96 && !trimmed.includes("\n");
}

function parseSessionSource(value: string | null): ParsedSessionSource {
  const trimmed = value?.trim();
  if (!trimmed) {
    return {
      label: null,
      parentThreadId: null,
      depth: null,
    };
  }

  if (!trimmed.startsWith("{")) {
    return {
      label: trimmed,
      parentThreadId: null,
      depth: null,
    };
  }

  try {
    const parsed = JSON.parse(trimmed) as {
      subagent?: {
        thread_spawn?: {
          parent_thread_id?: unknown;
          depth?: unknown;
        };
      };
    };
    const threadSpawn = parsed.subagent?.thread_spawn;
    if (threadSpawn) {
      return {
        label: "subagent",
        parentThreadId:
          typeof threadSpawn.parent_thread_id === "string" && threadSpawn.parent_thread_id.trim()
            ? threadSpawn.parent_thread_id.trim()
            : null,
        depth:
          typeof threadSpawn.depth === "number" || typeof threadSpawn.depth === "string"
            ? String(threadSpawn.depth)
            : null,
      };
    }
  } catch {
    return {
      label: trimmed,
      parentThreadId: null,
      depth: null,
    };
  }

  return {
    label: "structured",
    parentThreadId: null,
    depth: null,
  };
}

function formatProviderModel(
  provider: string | null | undefined,
  model: string | null | undefined,
) {
  const providerValue = provider?.trim();
  const modelValue = model?.trim();

  if (providerValue && modelValue) {
    return `${providerValue}/${modelValue}`;
  }

  return providerValue || modelValue || null;
}

function isAbsolutePath(value: string) {
  return /^(?:\/|\\\\|[A-Za-z]:[\\/])/.test(value);
}

function resolveSessionPath(sessionsDir: string | null, sessionRef: string | null) {
  if (!sessionRef) {
    return null;
  }

  if (isAbsolutePath(sessionRef)) {
    return sessionRef;
  }

  if (!sessionsDir) {
    return null;
  }

  const separator = sessionsDir.includes("\\") ? "\\" : "/";
  const normalizedDir = sessionsDir.replace(/[\\/]+$/, "");
  const normalizedRef = sessionRef.replace(/^[\\/]+/, "").replace(/[\\/]+/g, separator);
  return `${normalizedDir}${separator}${normalizedRef}`;
}

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

function findLatestTimelineEvent(items: TimelineItem[]): { eventId: string; seq: number } | null {
  let latest: { eventId: string; seq: number } | null = null;

  for (const item of items) {
    if ("Event" in item) {
      const candidate = {
        eventId: item.Event.event.event_id,
        seq: item.Event.event.seq,
      };
      if (!latest || candidate.seq > latest.seq) {
        latest = candidate;
      }

      const childLatest = findLatestTimelineEvent(item.Event.children);
      if (childLatest && (!latest || childLatest.seq > latest.seq)) {
        latest = childLatest;
      }
      continue;
    }

    const childLatest = findLatestTimelineEvent(item.Thread.items);
    if (childLatest && (!latest || childLatest.seq > latest.seq)) {
      latest = childLatest;
    }
  }

  return latest;
}

function findLatestSessionEventId(session: LoadedSession) {
  const rootsLatest = findLatestTimelineEvent(
    session.tree.roots.flatMap((thread) => thread.items),
  );
  const orphanLatest = session.tree.orphan_events.reduce<{ eventId: string; seq: number } | null>(
    (current, event) => {
      const candidate = { eventId: event.event_id, seq: event.seq };
      if (!current || candidate.seq > current.seq) {
        return candidate;
      }
      return current;
    },
    null,
  );

  if (!rootsLatest) {
    return orphanLatest?.eventId ?? null;
  }
  if (!orphanLatest) {
    return rootsLatest.eventId;
  }
  return rootsLatest.seq >= orphanLatest.seq ? rootsLatest.eventId : orphanLatest.eventId;
}

function CatalogMetaItem({
  label,
  title,
  value,
}: {
  label: string;
  title?: string;
  value: string;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
        {label}
      </span>
      <p className="ui-selectable truncate text-xs text-foreground/80" title={title ?? value}>
        {value}
      </p>
    </div>
  );
}

function SessionSummaryMetaItem({
  label,
  monospace = false,
  value,
}: {
  label: string;
  monospace?: boolean;
  value: string;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <dt className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
        {label}
      </dt>
      <dd
        className={cn(
          "ui-selectable break-words text-sm text-foreground",
          monospace && "font-mono text-xs",
        )}
        title={value}
      >
        {value}
      </dd>
    </div>
  );
}

function SessionSummaryLongField({
  label,
  monospace = false,
  value,
}: {
  label: string;
  monospace?: boolean;
  value: string;
}) {
  const [expanded, setExpanded] = useState(false);
  const collapsible = shouldCollapseSummaryText(value);
  const displayValue = collapsible && !expanded ? truncateSummaryText(value) : value;

  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
        {label}
      </div>
      <div
        className={cn(
          "ui-selectable whitespace-pre-wrap break-words text-sm text-foreground",
          monospace && "font-mono text-xs",
        )}
      >
        {displayValue}
      </div>
      {collapsible ? (
        <button
          className="inline-flex items-center self-start text-[11px] font-semibold uppercase tracking-[0.08em] text-[color:var(--accent-strong)] transition-opacity hover:opacity-80"
          onClick={() => setExpanded((current) => !current)}
          type="button"
        >
          {expanded ? "Скрыть" : "Показать полностью"}
        </button>
      ) : null}
    </div>
  );
}

function SessionCatalogCard({
  onOpen,
  selected,
  session,
}: {
  onOpen: (sessionId: string) => void;
  selected: boolean;
  session: IndexedSessionSummary;
}) {
  return (
    <button className="w-full text-left" onClick={() => onOpen(session.session_id)} type="button">
      <Card
        className={cn(
          SURFACE_CARD_CLASS,
          "transition-colors hover:bg-muted/40",
          selected && "ring-2 ring-ring/50",
        )}
        size="sm"
      >
        <CardHeader className="gap-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <CardDescription className="text-[11px] font-semibold uppercase tracking-[0.16em]">
                Thread
              </CardDescription>
              <CardTitle
                className="ui-selectable line-clamp-2 text-sm leading-6"
                title={session.thread_name ?? "thread_name not set"}
              >
                {session.thread_name ?? "thread_name not set"}
              </CardTitle>
            </div>
          </div>
        </CardHeader>
        <CardContent className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <CatalogMetaItem label="Updated" value={formatDateTime(session.updated_at)} />
          <CatalogMetaItem
            label="Tokens"
            value={formatTokenCount(session.tokens_used)}
          />
          <CatalogMetaItem
            label="Cwd"
            title={session.cwd ?? "n/a"}
            value={session.cwd ?? "n/a"}
          />
          {session.agent_name ? (
            <CatalogMetaItem
              label="Agent"
              title={session.agent_name}
              value={session.agent_name}
            />
          ) : null}
        </CardContent>
      </Card>
    </button>
  );
}

export default function App() {
  const [bootState, setBootState] = useState<BootState>("booting");
  const [bootError, setBootError] = useState<string | null>(null);
  const [resolvedHome, setResolvedHome] = useState<ResolvedCodexHome | null>(null);
  const [isSessionDialogOpen, setIsSessionDialogOpen] = useState(false);
  const [sessionQuery, setSessionQuery] = useState("");
  const deferredSessionQuery = useDeferredValue(sessionQuery);
  const [catalogSessions, setCatalogSessions] = useState<IndexedSessionSummary[]>([]);
  const [catalogDiagnostics, setCatalogDiagnostics] = useState<SessionDiagnostic[]>([]);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [catalogBusy, setCatalogBusy] = useState(false);
  const [catalogAppending, setCatalogAppending] = useState(false);
  const [catalogNextCursor, setCatalogNextCursor] = useState<string | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [selectedSessionRef, setSelectedSessionRef] = useState<string | null>(null);
  const [selectedPreview, setSelectedPreview] = useState<SessionPreview | null>(null);
  const [selectedLoadedSession, setSelectedLoadedSession] = useState<LoadedSession | null>(null);
  const [sessionBusy, setSessionBusy] = useState(false);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [selectedAgentThreadId, setSelectedAgentThreadId] = useState<string | null>(null);
  const [tailCursor, setTailCursor] = useState<TailCursor | null>(null);
  const [liveTailEnabled, setLiveTailEnabled] = useState(false);
  const [tailStatus, setTailStatus] = useState("Ожидание инициализации viewer backend.");
  const [timelineFocusEventId, setTimelineFocusEventId] = useState<string | null>(null);
  const [timelineFocusRevision, setTimelineFocusRevision] = useState(0);
  const tailCursorRef = useRef<TailCursor | null>(null);
  const selectedSessionRefRef = useRef<string | null>(null);
  const sessionCatalogRequestIdRef = useRef(0);
  const sessionRequestIdRef = useRef(0);
  const pendingSessionRefRef = useRef<string | null>(null);
  const initialCatalogRequestedRef = useRef(false);
  const initialAutoloadPendingRef = useRef(false);
  const displayedSessionId = selectedPreview?.session_id ?? selectedSessionId ?? null;
  const displayedSessionRef = selectedPreview?.session_ref ?? selectedSessionRef ?? null;
  const displayedSessionPath = resolveSessionPath(
    resolvedHome?.sessions_dir ?? null,
    displayedSessionRef,
  );
  const selectedIndexedSummary =
    selectedPreview?.indexed_summary
    ?? (displayedSessionId
      ? catalogSessions.find((session) => session.session_id === displayedSessionId) ?? null
      : null);
  const sessionSource = parseSessionSource(selectedIndexedSummary?.source ?? null);
  const providerModelValue = formatProviderModel(
    selectedIndexedSummary?.model_provider,
    selectedIndexedSummary?.model,
  );
  const threadSummaryValue =
    selectedIndexedSummary?.thread_name
    && !["title", "first_user_message"].includes(selectedIndexedSummary.thread_name_source ?? "")
    && isCompactThreadSummary(selectedIndexedSummary.thread_name)
      ? selectedIndexedSummary.thread_name
      : null;
  const sessionSummaryFields: SessionSummaryField[] = selectedIndexedSummary
    ? [
        threadSummaryValue ? { label: "Thread", value: threadSummaryValue } : null,
        selectedIndexedSummary.created_at
          ? { label: "Created", value: formatDateTime(selectedIndexedSummary.created_at) }
          : null,
        selectedIndexedSummary.updated_at
          ? { label: "Updated", value: formatDateTime(selectedIndexedSummary.updated_at) }
          : null,
        providerModelValue
          ? { label: "Model", value: providerModelValue }
          : null,
        selectedIndexedSummary.reasoning_effort
          ? { label: "Effort", value: selectedIndexedSummary.reasoning_effort }
          : null,
        selectedIndexedSummary.approval_mode
          ? { label: "Approval", value: selectedIndexedSummary.approval_mode }
          : null,
        selectedIndexedSummary.sandbox_policy_kind
          ? { label: "Sandbox", value: selectedIndexedSummary.sandbox_policy_kind }
          : null,
        selectedIndexedSummary.memory_mode
          ? { label: "Memory", value: selectedIndexedSummary.memory_mode }
          : null,
        selectedIndexedSummary.cli_version
          ? { label: "CLI", value: selectedIndexedSummary.cli_version }
          : null,
        sessionSource.label
          ? { label: "Source", value: sessionSource.label }
          : null,
        sessionSource.depth
          ? { label: "Depth", value: sessionSource.depth }
          : null,
        selectedIndexedSummary.agent_name
          ? { label: "Agent", value: selectedIndexedSummary.agent_name }
          : null,
        selectedIndexedSummary.agent_role
          ? { label: "Role", value: selectedIndexedSummary.agent_role }
          : null,
        selectedIndexedSummary.tokens_used != null
          ? { label: "Tokens", value: formatTokenCount(selectedIndexedSummary.tokens_used) }
          : null,
        selectedIndexedSummary.has_user_event
          ? { label: "User event", value: formatBoolValue(selectedIndexedSummary.has_user_event) }
          : null,
        selectedIndexedSummary.archived
          ? {
              label: "Archived",
              value: selectedIndexedSummary.archived_at
                ? `yes · ${formatDateTime(selectedIndexedSummary.archived_at)}`
                : "yes",
            }
          : null,
      ].filter((field): field is SessionSummaryField => field != null)
    : [];
  const agentGraph = selectedLoadedSession ? buildAgentGraphViewModel(selectedLoadedSession) : null;
  const activeAgentThreadId =
    selectedAgentThreadId && agentGraph?.byThreadId[selectedAgentThreadId]
      ? selectedAgentThreadId
      : preferredAgentThreadId(agentGraph);
  const activeAgent =
    activeAgentThreadId && agentGraph
      ? agentGraph.byThreadId[activeAgentThreadId] ?? null
      : null;

  const applyPreview = useCallback((preview: SessionPreview) => {
    tailCursorRef.current = preview.tail_cursor;
    setTailCursor(preview.tail_cursor);
    startTransition(() => {
      setSelectedPreview(preview);
    });
  }, []);

  const clearSelectedSession = useCallback((reason?: string) => {
    sessionRequestIdRef.current += 1;
    pendingSessionRefRef.current = null;
    selectedSessionRefRef.current = null;
    tailCursorRef.current = null;
    setSelectedSessionId(null);
    setSelectedAgentThreadId(null);
    setSelectedSessionRef(null);
    setSelectedPreview(null);
    setSelectedLoadedSession(null);
    setTailCursor(null);
    setTimelineFocusEventId(null);
    setTimelineFocusRevision(0);
    setSessionError(null);
    setSessionBusy(false);
    setTailStatus(
      reason ??
        "Жду выбора сессии из dialog picker, отдельного окна или команды. Main viewer больше не рендерит sidebar session catalog.",
    );
  }, []);

  const toggleLiveTail = useCallback(() => {
    const next = !liveTailEnabled;
    setLiveTailEnabled(next);

    if (sessionBusy) {
      return;
    }

    if (next) {
      setTailStatus(
        selectedSessionRef
          ? "Live tail включен. Автоматическое обновление состояния возобновлено."
          : "Live tail включен. Автоматическое обновление начнётся после выбора сессии.",
      );
      return;
    }

    setTailStatus("Live tail выключен. Автоматическое обновление остановлено.");
  }, [liveTailEnabled, selectedSessionRef, sessionBusy]);

  const focusTimelineEvent = useCallback((eventId: string | null) => {
    if (!eventId) {
      return;
    }

    setTimelineFocusEventId(eventId);
    setTimelineFocusRevision((current) => current + 1);
  }, []);

  const activateAgent = useCallback(
    (threadId: string, focusEventId?: string | null) => {
      setSelectedAgentThreadId(threadId);
      if (focusEventId) {
        focusTimelineEvent(focusEventId);
      }
    },
    [focusTimelineEvent],
  );

  const refreshSessionCatalog = useCallback(
    async ({
      append = false,
      cursor = null,
      query = deferredSessionQuery,
    }: {
      append?: boolean;
      cursor?: string | null;
      query?: string;
    } = {}) => {
      if (bootState !== "ready") {
        return;
      }

      const requestId = ++sessionCatalogRequestIdRef.current;
      setCatalogError(null);
      if (append) {
        setCatalogAppending(true);
      } else {
        setCatalogBusy(true);
      }

      try {
        const page = await listIndexedSessions({
          cursor,
          exactSessionId: extractExactSessionId(query),
          limit: SESSIONS_PAGE_SIZE,
          query,
        });
        if (requestId !== sessionCatalogRequestIdRef.current) {
          return;
        }

        startTransition(() => {
          setCatalogSessions((current) => (append ? [...current, ...page.items] : page.items));
          setCatalogDiagnostics(page.diagnostics);
          setCatalogNextCursor(page.next_cursor);
        });
      } catch (error) {
        if (requestId === sessionCatalogRequestIdRef.current) {
          const message = extractErrorMessage(error);
          setCatalogError(message);
          if (!append) {
            setTailStatus(message);
          }
        }
      } finally {
        if (requestId === sessionCatalogRequestIdRef.current) {
          setCatalogBusy(false);
          setCatalogAppending(false);
        }
      }
    },
    [bootState, deferredSessionQuery],
  );

  const openSession = useCallback(
    async (sessionRef: string) => {
      const normalizedSessionRef = sessionRef.trim();

      if (!normalizedSessionRef) {
        return;
      }

      const requestId = ++sessionRequestIdRef.current;
      selectedSessionRefRef.current = normalizedSessionRef;
      setSelectedSessionId(null);
      setSelectedAgentThreadId(null);
      setSelectedSessionRef(normalizedSessionRef);
      setSelectedPreview(null);
      setSessionBusy(true);
      setSessionError(null);
      setSelectedLoadedSession(null);
      setTimelineFocusEventId(null);
      setTimelineFocusRevision(0);
      tailCursorRef.current = null;
      setTailCursor(null);
      setTailStatus("Загружаю основную ленту выбранной сессии.");

      try {
        const loadedSession = await loadSession(normalizedSessionRef);
        if (
          selectedSessionRefRef.current !== normalizedSessionRef ||
          requestId !== sessionRequestIdRef.current
        ) {
          return;
        }

        selectedSessionRefRef.current = loadedSession.session_ref;
        tailCursorRef.current = loadedSession.tail_cursor;
        setSelectedSessionId(loadedSession.session_id);
        setSelectedSessionRef(loadedSession.session_ref);
        setSelectedLoadedSession(loadedSession);
        setTailCursor(loadedSession.tail_cursor);
        setTailStatus("Основная лента загружена. Догружаю summary выбранной сессии.");

        const preview = await loadSessionPreview(loadedSession.session_ref);
        if (
          selectedSessionRefRef.current !== loadedSession.session_ref ||
          requestId !== sessionRequestIdRef.current
        ) {
          return;
        }

        applyPreview(preview);
        setTailStatus("Сессия загружена.");
      } catch (error) {
        if (requestId === sessionRequestIdRef.current) {
          const message = extractErrorMessage(error);
          setSessionError(message);
          setTailStatus(message);
        }
      } finally {
        if (requestId === sessionRequestIdRef.current) {
          setSessionBusy(false);
        }
      }
    },
    [applyPreview],
  );

  const openSessionById = useCallback(
    async (sessionId: string) => {
      const normalizedSessionId = sessionId.trim();

      if (!normalizedSessionId) {
        return;
      }

      const requestId = ++sessionRequestIdRef.current;
      selectedSessionRefRef.current = null;
      setSelectedSessionId(normalizedSessionId);
      setSelectedAgentThreadId(null);
      setSelectedSessionRef(null);
      setSelectedPreview(null);
      setSessionBusy(true);
      setSessionError(null);
      setSelectedLoadedSession(null);
      setTimelineFocusEventId(null);
      setTimelineFocusRevision(0);
      tailCursorRef.current = null;
      setTailCursor(null);
      setTailStatus("Резолвлю выбранную запись каталога и загружаю основную ленту.");

      try {
        const preview = await loadSessionPreviewById(normalizedSessionId);
        if (requestId !== sessionRequestIdRef.current) {
          return;
        }

        selectedSessionRefRef.current = preview.session_ref;
        setSelectedSessionId(preview.session_id);
        setSelectedSessionRef(preview.session_ref);
        setTailStatus("Session ref найден. Загружаю основную ленту rollout.");

        const loadedSession = await loadSession(preview.session_ref);
        if (requestId !== sessionRequestIdRef.current) {
          return;
        }

        tailCursorRef.current = loadedSession.tail_cursor;
        setTailCursor(loadedSession.tail_cursor);
        setSelectedLoadedSession(loadedSession);
        setTailStatus("Основная лента загружена. Догружаю summary rollout.");

        applyPreview(preview);
        setTailStatus("Сессия загружена.");
      } catch (error) {
        if (requestId === sessionRequestIdRef.current) {
          const message = extractErrorMessage(error);
          setSessionError(message);
          setTailStatus(message);
        }
      } finally {
        if (requestId === sessionRequestIdRef.current) {
          setSessionBusy(false);
        }
      }
    },
    [applyPreview],
  );

  const openSessionFromDialog = useCallback(
    (sessionId: string) => {
      setIsSessionDialogOpen(false);
      void openSessionById(sessionId);
    },
    [openSessionById],
  );

  const loadMoreCatalogSessions = useCallback(() => {
    if (!catalogNextCursor || catalogBusy || catalogAppending) {
      return;
    }

    void refreshSessionCatalog({
      append: true,
      cursor: catalogNextCursor,
      query: sessionQuery,
    });
  }, [catalogAppending, catalogBusy, catalogNextCursor, refreshSessionCatalog, sessionQuery]);

  useEffect(() => {
    let active = true;

    async function bootstrap() {
      try {
        const detected = await detectCodexHome();
        if (!active) {
          return;
        }

        if (!detected.detected_home) {
          setBootState("needs_home");
          setTailStatus("CODEX_HOME не найден; viewer не может открыть выбранную сессию.");
          return;
        }

        const initialized = await initializeCodexHome();
        if (!active) {
          return;
        }

        setResolvedHome(initialized.resolved_home);
        setBootState("ready");
        setTailStatus(
          "Backend инициализирован. Жду выбора сессии из dialog picker, отдельного окна или команды.",
        );
      } catch (error) {
        if (!active) {
          return;
        }

        setBootState("error");
        setBootError(extractErrorMessage(error));
      }
    }

    void bootstrap();

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (bootState !== "ready" || !pendingSessionRefRef.current) {
      return;
    }

    const sessionRef = pendingSessionRefRef.current;
    pendingSessionRefRef.current = null;
    void openSession(sessionRef);
  }, [bootState, openSession]);

  useEffect(() => {
    if (bootState !== "ready" || initialCatalogRequestedRef.current) {
      return;
    }

    initialCatalogRequestedRef.current = true;
    initialAutoloadPendingRef.current = !pendingSessionRefRef.current;
    if (initialAutoloadPendingRef.current) {
      setTailStatus("Backend инициализирован. Ищу активную сессию в indexed catalog.");
    }
    void refreshSessionCatalog({ query: "" });
  }, [bootState, refreshSessionCatalog]);

  useEffect(() => {
    if (!initialAutoloadPendingRef.current || catalogBusy) {
      return;
    }

    initialAutoloadPendingRef.current = false;
    if (pendingSessionRefRef.current || sessionRequestIdRef.current > 0) {
      return;
    }

    const activeSession = catalogSessions[0];
    if (activeSession) {
      void openSessionById(activeSession.session_id);
      return;
    }

    if (!catalogError) {
      setTailStatus("Backend инициализирован, но активная сессия в indexed catalog не найдена.");
    }
  }, [catalogBusy, catalogError, catalogSessions, openSessionById]);

  useEffect(() => {
    if (!isSessionDialogOpen || bootState !== "ready") {
      return;
    }

    void refreshSessionCatalog();
  }, [bootState, isSessionDialogOpen, refreshSessionCatalog]);

  useEffect(() => {
    if (!isTauri()) {
      return;
    }

    let unlistenOpen: (() => void) | undefined;
    let unlistenClear: (() => void) | undefined;

    async function bindListeners() {
      unlistenOpen = await listen<OpenSessionEventPayload>(OPEN_SESSION_EVENT, (event) => {
        const sessionRef = readSessionRef(event.payload);

        if (!sessionRef) {
          setSessionError("Внешняя команда открытия пришла без sessionRef.");
          return;
        }

        if (bootState !== "ready") {
          pendingSessionRefRef.current = sessionRef;
          setTailStatus(`Выбор сессии ${sessionRef} получен и будет применён после инициализации.`);
          return;
        }

        void openSession(sessionRef);
      });

      unlistenClear = await listen(CLEAR_SESSION_EVENT, () => {
        clearSelectedSession("Выбранная сессия очищена внешней командой.");
      });
    }

    void bindListeners();

    return () => {
      void unlistenOpen?.();
      void unlistenClear?.();
    };
  }, [bootState, clearSelectedSession, openSession]);

  useEffect(() => {
    if (!liveTailEnabled || !selectedSessionRef || !tailCursorRef.current) {
      return;
    }

    const sessionRef = selectedSessionRef;
    let cancelled = false;
    let timeoutId = 0;

    async function poll() {
      const currentCursor = tailCursorRef.current;
      if (!currentCursor) {
        return;
      }

      try {
        const result = await tailSession(sessionRef, currentCursor);
        if (cancelled) {
          return;
        }

        tailCursorRef.current = result.next_cursor;
        setTailCursor(result.next_cursor);

        if (result.reset) {
          setLiveTailEnabled(false);
          setTailStatus(
            "Сессия была переписана или ротирована. Live tail остановлен; обновите основную ленту вручную.",
          );
          return;
        }

        if (result.events.length > 0) {
          const selectionRequestId = sessionRequestIdRef.current;
          setTailStatus(`Получено новых событий: ${result.events.length}. Обновляю основную ленту.`);

          const [loadedSession, preview] = await Promise.all([
            loadSession(sessionRef),
            loadSessionPreview(sessionRef),
          ]);
          if (
            cancelled ||
            selectedSessionRefRef.current !== sessionRef ||
            selectionRequestId !== sessionRequestIdRef.current
          ) {
            return;
          }

          tailCursorRef.current = preview.tail_cursor;
          setTailCursor(preview.tail_cursor);
          const latestEventId = findLatestSessionEventId(loadedSession);
          startTransition(() => {
            setSelectedLoadedSession(loadedSession);
            setSelectedPreview(preview);
          });
          if (latestEventId) {
            setTimelineFocusEventId(latestEventId);
            setTimelineFocusRevision((current) => current + 1);
          }
          setTailStatus(`Основная лента обновлена. Новых событий: ${result.events.length}.`);
        } else {
          setTailStatus("Новых событий пока нет, tail остаётся активным.");
        }
      } catch (error) {
        if (!cancelled) {
          setTailStatus(extractErrorMessage(error));
        }
      } finally {
        if (!cancelled) {
          timeoutId = window.setTimeout(poll, LIVE_TAIL_POLL_MS);
        }
      }
    }

    timeoutId = window.setTimeout(poll, LIVE_TAIL_POLL_MS);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [liveTailEnabled, selectedSessionRef]);

  return (
    <>
      <Dialog onOpenChange={setIsSessionDialogOpen} open={isSessionDialogOpen}>
        <DialogContent className="max-w-[min(72rem,calc(100%-2rem))] gap-0 p-0 sm:max-w-[72rem]">
          <DialogHeader className="gap-3 border-b border-border/50 px-4 py-4 sm:px-5">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="flex flex-col gap-2">
                <DialogTitle>Выбор сессии</DialogTitle>
                <p className="text-xs text-muted-foreground">{catalogSessions.length} loaded</p>
              </div>
            </div>
          </DialogHeader>

          <div className="flex flex-col gap-4 px-4 py-4 sm:px-5">
            <div className="flex flex-col gap-3 sm:flex-row">
              <Input
                aria-label="Search sessions"
                className="sm:flex-1"
                onChange={(event) => {
                  setSessionQuery(event.target.value);
                }}
                placeholder="thread name or session id"
                type="search"
                value={sessionQuery}
              />
              <Button
                disabled={bootState !== "ready" || catalogBusy || catalogAppending}
                onClick={() => {
                  void refreshSessionCatalog({ query: sessionQuery });
                }}
                type="button"
                variant="outline"
              >
                <RefreshCcw className={cn(catalogBusy && "animate-spin")} data-icon="inline-start" />
                Refresh
              </Button>
            </div>

            {bootState === "booting" ? (
              <Alert>
                <RefreshCcw className="size-4 animate-spin" />
                <AlertTitle>Viewer инициализируется</AlertTitle>
                <AlertDescription>
                  Диалог выбора сессии станет доступен после bootstrap backend.
                </AlertDescription>
              </Alert>
            ) : null}

            {bootState === "needs_home" ? (
              <Alert>
                <AlertTriangle className="size-4" />
                <AlertTitle>CODEX_HOME не найден</AlertTitle>
                <AlertDescription>
                  Пока локальный `CODEX_HOME` не найден, viewer не может прочитать session catalog.
                </AlertDescription>
              </Alert>
            ) : null}

            {bootState === "error" && bootError ? (
              <Alert variant="destructive">
                <AlertTriangle className="size-4" />
                <AlertTitle>Viewer bootstrap failed</AlertTitle>
                <AlertDescription className="ui-selectable">{bootError}</AlertDescription>
              </Alert>
            ) : null}

            {catalogError ? (
              <Alert variant="destructive">
                <AlertTriangle className="size-4" />
                <AlertTitle>Session catalog failed</AlertTitle>
                <AlertDescription className="ui-selectable">{catalogError}</AlertDescription>
              </Alert>
            ) : null}

            {catalogDiagnostics.length > 0 ? (
              <div className="flex flex-col gap-2">
                {catalogDiagnostics.slice(0, 4).map((diagnostic, index) => (
                  <Alert key={`${diagnostic.kind}-${index}`} variant="destructive">
                    <AlertTriangle className="size-4" />
                    <AlertTitle>{diagnostic.kind}</AlertTitle>
                    <AlertDescription className="flex flex-col gap-1">
                      <span>{diagnostic.message}</span>
                      {diagnostic.session_refs.length > 0 ? (
                        <span className="ui-selectable text-xs">
                          {diagnostic.session_refs.join(", ")}
                        </span>
                      ) : null}
                    </AlertDescription>
                  </Alert>
                ))}
              </div>
            ) : null}

            <ScrollArea className="h-[min(58vh,42rem)]">
              <div className="flex flex-col gap-3 pr-4">
                {catalogSessions.map((session) => (
                  <SessionCatalogCard
                    key={session.session_id}
                    onOpen={openSessionFromDialog}
                    selected={session.session_id === (selectedPreview?.session_id ?? selectedSessionId)}
                    session={session}
                  />
                ))}

                {catalogBusy && !catalogSessions.length ? (
                  <Alert>
                    <RefreshCcw className="size-4 animate-spin" />
                    <AlertTitle>Каталог загружается</AlertTitle>
                    <AlertDescription>
                      Viewer читает каталог сессий из локального `CODEX_HOME`.
                    </AlertDescription>
                  </Alert>
                ) : null}

                {!catalogSessions.length && !catalogBusy && bootState === "ready" ? (
                  <Alert>
                    <Target className="size-4" />
                    <AlertTitle>Сессии не найдены</AlertTitle>
                    <AlertDescription>
                      Попробуйте очистить поиск или проверьте источники каталога в `CODEX_HOME`.
                    </AlertDescription>
                  </Alert>
                ) : null}
              </div>
            </ScrollArea>
          </div>

          <DialogFooter className="sm:justify-between">
            <p className="ui-selectable break-all text-xs text-muted-foreground">
              {resolvedHome?.session_index_path ?? "session index unavailable"}
            </p>

            <div className="flex flex-col-reverse gap-2 sm:flex-row">
              {catalogNextCursor ? (
                <Button
                  disabled={catalogBusy || catalogAppending}
                  onClick={loadMoreCatalogSessions}
                  type="button"
                  variant="outline"
                >
                  {catalogAppending ? "Loading…" : "Load More"}
                </Button>
              ) : null}

              <DialogClose asChild>
                <Button type="button" variant="outline">
                  Close
                </Button>
              </DialogClose>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <main data-ui-scroll-container className="h-full px-4 py-4 sm:px-6 sm:py-6">
        <div className="flex min-h-full w-full flex-col gap-5">
          <Card className={PANEL_CARD_CLASS}>
            <CardHeader className="gap-3 py-4">
              <div className="flex flex-col gap-3 xl:flex-row xl:items-start xl:justify-between">
                <div className="flex min-w-0 flex-1 flex-col gap-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <CardTitle className="text-lg leading-none">Codex Log Viewer</CardTitle>
                    <span className="inline-flex items-center rounded-md border border-border px-2 py-1 text-xs font-medium text-muted-foreground">
                      Tail: {liveTailEnabled ? "on" : "off"}
                    </span>
                  </div>
                  <div className="flex min-w-0 flex-col gap-1">
                    <p className="text-sm leading-6 text-foreground">{tailStatus}</p>
                    {tailCursor ? (
                      <p className="text-xs leading-5 text-muted-foreground">
                        offset {tailCursor.offset} · next_seq {tailCursor.next_seq}
                      </p>
                    ) : null}
                  </div>
                </div>

                <div className="flex flex-wrap gap-2">
                  <Button onClick={toggleLiveTail} type="button" variant="outline">
                    <RadioTower data-icon="inline-start" />
                    Tail {liveTailEnabled ? "Off" : "On"}
                  </Button>
                  <Button
                    onClick={() => {
                      setIsSessionDialogOpen(true);
                    }}
                    type="button"
                  >
                    <FolderSearch2 data-icon="inline-start" />
                    Choose Session
                  </Button>
                  {selectedSessionId || selectedSessionRef ? (
                    <Button
                      onClick={() => {
                        clearSelectedSession("Выбор очищен локально. Сессию можно открыть из диалога.");
                      }}
                      type="button"
                      variant="outline"
                    >
                      <Unplug data-icon="inline-start" />
                      Clear Selection
                    </Button>
                  ) : null}
                </div>
              </div>
            </CardHeader>
          </Card>

          <section className="grid min-h-0 flex-1 gap-5 xl:grid-cols-[minmax(0,1.75fr)_minmax(360px,0.9fr)]">
            <Card className={cn(PANEL_CARD_CLASS, "h-full")}>
              <CardHeader className="gap-3">
                <div className="flex items-center gap-2">
                  <Target className="size-4 text-[color:var(--accent-strong)]" />
                  <CardTitle className="text-base">Session</CardTitle>
                </div>
                <CardDescription className="ui-selectable break-all">
                  <span className="block">Id: {displayedSessionId ?? "not set"}</span>
                  <span className="block">Path: {displayedSessionPath ?? "not resolved"}</span>
                </CardDescription>
                <CardAction className="flex gap-2">
                  <Button
                    onClick={() => {
                      setIsSessionDialogOpen(true);
                    }}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    <FolderSearch2 data-icon="inline-start" />
                    Choose Session
                  </Button>
                  {selectedSessionRef ? (
                    <Button
                      disabled={sessionBusy}
                      onClick={() => {
                        void openSession(selectedSessionRef);
                      }}
                      size="sm"
                      type="button"
                    >
                      <RefreshCcw data-icon="inline-start" />
                      {sessionBusy ? "Loading…" : "Refresh Session"}
                    </Button>
                  ) : null}
                </CardAction>
              </CardHeader>

              <CardContent className="flex min-h-0 flex-1 flex-col gap-4 pt-4">
                {bootState === "needs_home" ? (
                  <Alert>
                    <AlertTriangle className="size-4" />
                    <AlertTitle>CODEX_HOME не найден</AlertTitle>
                    <AlertDescription>
                      Viewer ожидает уже существующий локальный `CODEX_HOME`.
                    </AlertDescription>
                  </Alert>
                ) : null}

                {bootError ? (
                  <Alert variant="destructive">
                    <AlertTriangle className="size-4" />
                    <AlertTitle>Viewer bootstrap failed</AlertTitle>
                    <AlertDescription className="ui-selectable">{bootError}</AlertDescription>
                  </Alert>
                ) : null}

                {sessionError ? (
                  <Alert variant="destructive">
                    <AlertTriangle className="size-4" />
                    <AlertTitle>Session load failed</AlertTitle>
                    <AlertDescription className="ui-selectable">{sessionError}</AlertDescription>
                  </Alert>
                ) : null}

                <Separator />

                <ScrollArea className="min-h-0 flex-1">
                  <div className="flex flex-col gap-2 pr-4">
                    {selectedLoadedSession ? (
                      <SessionEventList
                        focusEventId={timelineFocusEventId}
                        focusRevision={timelineFocusRevision}
                        onAgentSelect={(threadId) => {
                          setSelectedAgentThreadId(threadId);
                        }}
                        selectedAgentThreadId={activeAgentThreadId}
                        session={selectedLoadedSession}
                      />
                    ) : selectedPreview ? (
                      <Alert>
                        <RefreshCcw className={cn("size-4", sessionBusy && "animate-spin")} />
                        <AlertTitle>
                          {sessionBusy ? "Догружаю полную сессию" : "Полная сессия недоступна"}
                        </AlertTitle>
                        <AlertDescription>
                          {sessionBusy
                            ? "Preview и summary уже обновлены. Timeline появится после чтения полного дерева событий."
                            : "Preview загружен, но полное дерево событий не удалось прочитать. Повторите загрузку сессии."}
                        </AlertDescription>
                      </Alert>
                    ) : (
                      <Alert>
                        <Target className="size-4" />
                        <AlertTitle>Viewer ждёт выбора</AlertTitle>
                        <AlertDescription>
                          Откройте встроенный диалог каталога, чтобы выбрать сессию.
                        </AlertDescription>
                      </Alert>
                    )}
                  </div>
                </ScrollArea>
              </CardContent>
            </Card>

            <Card className={cn(PANEL_CARD_CLASS, "h-full")}>
              <CardHeader className="gap-3">
                <CardTitle className="text-base">Agents</CardTitle>
              </CardHeader>

              <CardContent className="min-h-0 flex-1 pt-4">
                <ScrollArea className="h-full min-h-0">
                  <div className="flex flex-col gap-3 pr-4">
                    <Card className={SURFACE_CARD_CLASS} size="sm">
                      <CardHeader className="gap-2">
                        <CardTitle className="text-sm">Session summary</CardTitle>
                      </CardHeader>
                      <CardContent>
                        <div className="flex flex-col gap-4">
                          <dl className="grid grid-cols-2 gap-3 text-sm">
                            <SessionSummaryMetaItem
                              label="Events"
                              value={String(selectedPreview?.event_count ?? 0)}
                            />
                            <SessionSummaryMetaItem
                              label="Tail"
                              value={liveTailEnabled ? "on" : "off"}
                            />
                            <SessionSummaryMetaItem
                              label="First ts"
                              value={formatDateTime(selectedPreview?.first_ts ?? null)}
                            />
                            <SessionSummaryMetaItem
                              label="Last ts"
                              value={formatDateTime(selectedPreview?.last_ts ?? null)}
                            />
                          </dl>

                          {selectedIndexedSummary ? (
                            <>
                              <Separator />
                              {sessionSummaryFields.length > 0 ? (
                                <dl className="grid gap-3 sm:grid-cols-2">
                                  {sessionSummaryFields.map((field) => (
                                    <SessionSummaryMetaItem
                                      key={`${field.label}-${field.value}`}
                                      label={field.label}
                                      monospace={field.monospace}
                                      value={field.value}
                                    />
                                  ))}
                                </dl>
                              ) : null}

                              <div className="flex flex-col gap-3">
                                {selectedIndexedSummary.cwd ? (
                                  <SessionSummaryLongField
                                    label="Cwd"
                                    value={selectedIndexedSummary.cwd}
                                  />
                                ) : null}
                                {selectedIndexedSummary.git_branch ? (
                                  <SessionSummaryLongField
                                    label="Branch"
                                    monospace
                                    value={selectedIndexedSummary.git_branch}
                                  />
                                ) : null}
                                {selectedIndexedSummary.git_sha ? (
                                  <SessionSummaryLongField
                                    label="Commit"
                                    monospace
                                    value={selectedIndexedSummary.git_sha}
                                  />
                                ) : null}
                                {sessionSource.parentThreadId ? (
                                  <SessionSummaryLongField
                                    label="Parent thread"
                                    monospace
                                    value={sessionSource.parentThreadId}
                                  />
                                ) : null}
                                {selectedIndexedSummary.git_origin_url ? (
                                  <SessionSummaryLongField
                                    label="Git origin"
                                    monospace
                                    value={selectedIndexedSummary.git_origin_url}
                                  />
                                ) : null}
                                {selectedIndexedSummary.agent_path ? (
                                  <SessionSummaryLongField
                                    label="Agent path"
                                    monospace
                                    value={selectedIndexedSummary.agent_path}
                                  />
                                ) : null}
                              </div>
                            </>
                          ) : (
                            <p className="text-xs text-muted-foreground">
                              Метаданные `threads` для выбранной сессии недоступны.
                            </p>
                          )}
                        </div>
                      </CardContent>
                    </Card>

                    <AgentsPanel
                      activeAgent={activeAgent}
                      graph={agentGraph}
                      onFocusEvent={focusTimelineEvent}
                      onSelectAgent={activateAgent}
                    />

                  </div>
                </ScrollArea>
              </CardContent>
            </Card>
          </section>
        </div>
      </main>
    </>
  );
}
