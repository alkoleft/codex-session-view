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
  loadSessionPreviewById,
  loadSessionPreview,
  tailSession,
  type EventRecord,
  type IndexedSessionSummary,
  type ResolvedCodexHome,
  type SessionDiagnostic,
  type SessionPreview,
  type SessionPreviewEvent,
  type TailCursor,
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
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
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

function formatTime(value: string | null) {
  if (!value) {
    return "n/a";
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat("ru-RU", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
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

function summarizeTailEvent(event: EventRecord) {
  const text = typeof event.payload.text === "string" ? event.payload.text : null;
  return {
    seq: event.seq,
    ts: event.ts,
    eventType: event.event_type,
    text,
    fallback: event.raw_type,
  };
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

function PreviewEventCard({ event }: { event: SessionPreviewEvent }) {
  return (
    <Card className={SURFACE_CARD_CLASS} size="sm">
      <CardContent className="flex flex-col gap-2 p-4">
        <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
          <span className="font-mono">#{event.seq}</span>
          <time>{formatTime(event.ts)}</time>
          <span className="font-mono">{event.event_type}</span>
        </div>
        <p className="ui-selectable text-sm leading-6">{event.summary}</p>
        {event.text ? (
          <p className="ui-selectable text-xs leading-5 text-muted-foreground">{event.text}</p>
        ) : null}
      </CardContent>
    </Card>
  );
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
  const [sessionBusy, setSessionBusy] = useState(false);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [tailCursor, setTailCursor] = useState<TailCursor | null>(null);
  const [liveTailEnabled, setLiveTailEnabled] = useState(false);
  const [tailStatus, setTailStatus] = useState("Ожидание инициализации viewer backend.");
  const [liveEvents, setLiveEvents] = useState<EventRecord[]>([]);
  const tailCursorRef = useRef<TailCursor | null>(null);
  const selectedSessionRefRef = useRef<string | null>(null);
  const sessionCatalogRequestIdRef = useRef(0);
  const sessionRequestIdRef = useRef(0);
  const pendingSessionRefRef = useRef<string | null>(null);
  const displayedSessionId = selectedPreview?.session_id ?? selectedSessionId ?? null;
  const displayedSessionRef = selectedPreview?.session_ref ?? selectedSessionRef ?? null;
  const displayedSessionPath = resolveSessionPath(
    resolvedHome?.sessions_dir ?? null,
    displayedSessionRef,
  );

  const applyPreview = useCallback((preview: SessionPreview) => {
    tailCursorRef.current = preview.tail_cursor;
    setTailCursor(preview.tail_cursor);
    startTransition(() => {
      setSelectedPreview(preview);
    });
  }, []);

  const clearSelectedSession = useCallback((reason?: string) => {
    selectedSessionRefRef.current = null;
    tailCursorRef.current = null;
    setSelectedSessionId(null);
    setSelectedSessionRef(null);
    setSelectedPreview(null);
    setTailCursor(null);
    setSessionError(null);
    setSessionBusy(false);
    setLiveEvents([]);
    setTailStatus(
      reason ??
        "Жду выбора сессии из dialog picker, отдельного окна или команды. Main viewer больше не рендерит sidebar session catalog.",
    );
  }, []);

  const toggleLiveTail = useCallback(() => {
    const next = !liveTailEnabled;
    setLiveTailEnabled(next);

    if (next) {
      setTailStatus(
        selectedSessionRef
          ? "Live tail включен. Автоматическое обновление состояния возобновлено."
          : "Live tail включен. Автоматическое обновление начнётся после выбора сессии.",
      );
      return;
    }

    setTailStatus("Live tail выключен. Автоматическое обновление остановлено.");
  }, [liveTailEnabled, selectedSessionRef]);

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
          setCatalogError(extractErrorMessage(error));
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
      setSelectedSessionRef(normalizedSessionRef);
      setSessionBusy(true);
      setSessionError(null);
      setLiveEvents([]);
      tailCursorRef.current = null;
      setTailCursor(null);
      setTailStatus("Загружаю preview выбранной сессии.");

      try {
        const preview = await loadSessionPreview(normalizedSessionRef);
        if (
          selectedSessionRefRef.current !== normalizedSessionRef ||
          requestId !== sessionRequestIdRef.current
        ) {
          return;
        }

        setSelectedSessionId(preview.session_id);
        applyPreview(preview);
        setTailStatus(
          liveTailEnabled
            ? "Slim preview загружен. Live tail активен для выбранной сессии."
            : "Slim preview загружен. Live tail выключен; включите его кнопкой при необходимости.",
        );
      } catch (error) {
        if (
          selectedSessionRefRef.current === normalizedSessionRef &&
          requestId === sessionRequestIdRef.current
        ) {
          setSessionError(extractErrorMessage(error));
        }
      } finally {
        if (requestId === sessionRequestIdRef.current) {
          setSessionBusy(false);
        }
      }
    },
    [applyPreview, liveTailEnabled],
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
      setSelectedSessionRef(null);
      setSessionBusy(true);
      setSessionError(null);
      setLiveEvents([]);
      tailCursorRef.current = null;
      setTailCursor(null);
      setTailStatus("Открываю rollout для выбранной записи каталога.");

      try {
        const preview = await loadSessionPreviewById(normalizedSessionId);
        if (requestId !== sessionRequestIdRef.current) {
          return;
        }

        selectedSessionRefRef.current = preview.session_ref;
        setSelectedSessionId(preview.session_id);
        setSelectedSessionRef(preview.session_ref);
        applyPreview(preview);
        setTailStatus(
          liveTailEnabled
            ? "Preview загружен по session_id. Live tail активен для найденного rollout."
            : "Preview загружен по session_id. Live tail выключен; включите его кнопкой при необходимости.",
        );
      } catch (error) {
        if (requestId === sessionRequestIdRef.current) {
          setSessionError(extractErrorMessage(error));
        }
      } finally {
        if (requestId === sessionRequestIdRef.current) {
          setSessionBusy(false);
        }
      }
    },
    [applyPreview, liveTailEnabled],
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
          setTailStatus("Сессия была переписана или ротирована; перечитываю preview.");
          await openSession(sessionRef);
          return;
        }

        if (result.events.length > 0) {
          const newestEvent = result.events[result.events.length - 1] ?? null;
          const nextLiveEvents = [...result.events].reverse();
          startTransition(() => {
            setLiveEvents((prev) => [...nextLiveEvents, ...prev].slice(0, 80));
            setSelectedPreview((current) =>
              current
                ? {
                    ...current,
                    event_count: current.event_count + result.events.length,
                    last_ts: newestEvent?.ts ?? current.last_ts,
                  }
                : current,
            );
          });
          setTailStatus(`Получено новых событий: ${result.events.length}.`);
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
  }, [liveTailEnabled, openSession, selectedSessionRef, tailCursor]);

  const liveTailRows = liveEvents.map(summarizeTailEvent);

  return (
    <>
      <Dialog onOpenChange={setIsSessionDialogOpen} open={isSessionDialogOpen}>
        <DialogContent className="max-w-[min(72rem,calc(100%-2rem))] gap-0 p-0 sm:max-w-[72rem]">
          <DialogHeader className="gap-3 border-b border-border/50 px-4 py-4 sm:px-5">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="flex flex-col gap-2">
                <DialogTitle>Выбор сессии</DialogTitle>
                <DialogDescription className="max-w-3xl">
                  Диалог читает каталог через backend-команду `list_indexed_sessions`: сначала из
                  `state_*.sqlite`, таблицы `threads`, и с fallback на `session_index.jsonl`.
                  Конкретный rollout-файл резолвится по `session_id` только в момент открытия
                  preview.
                </DialogDescription>
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
                </div>

                <div className="flex flex-wrap gap-2">
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
                      {sessionBusy ? "Loading…" : "Refresh Preview"}
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
                    <AlertTitle>Session preview failed</AlertTitle>
                    <AlertDescription className="ui-selectable">{sessionError}</AlertDescription>
                  </Alert>
                ) : null}

                <Separator />

                <ScrollArea className="min-h-0 flex-1">
                  <div className="flex flex-col gap-2 pr-4">
                    {selectedPreview ? (
                      selectedPreview.recent_events.map((event) => (
                        <PreviewEventCard
                          event={event}
                          key={`${event.seq}-${event.ts}-${event.event_type}`}
                        />
                      ))
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
                <CardTitle className="text-base">Inspector</CardTitle>
                <CardDescription>Live state и служебная информация выбранной сессии.</CardDescription>
              </CardHeader>

              <CardContent className="min-h-0 flex-1 pt-4">
                <ScrollArea className="h-full min-h-0">
                  <div className="flex flex-col gap-3 pr-4">
                    <Card className={SURFACE_CARD_CLASS} size="sm">
                      <CardHeader className="gap-2">
                        <div className="flex items-center justify-between gap-2">
                          <CardTitle className="text-sm">Tail status</CardTitle>
                          <Button onClick={toggleLiveTail} size="sm" type="button" variant="outline">
                            <RadioTower data-icon="inline-start" />
                            {liveTailEnabled ? "Off" : "On"}
                          </Button>
                        </div>
                      </CardHeader>
                      <CardContent className="flex flex-col gap-2">
                        <p className="text-sm leading-6">{tailStatus}</p>
                        {tailCursor ? (
                          <p className="text-xs leading-5 text-muted-foreground">
                            offset {tailCursor.offset} · next_seq {tailCursor.next_seq}
                          </p>
                        ) : null}
                      </CardContent>
                    </Card>

                    <Card className={SURFACE_CARD_CLASS} size="sm">
                      <CardHeader className="gap-2">
                        <CardTitle className="text-sm">Session summary</CardTitle>
                      </CardHeader>
                      <CardContent>
                        <dl className="grid grid-cols-2 gap-3 text-sm">
                          <div className="flex flex-col gap-1">
                            <dt className="text-muted-foreground">Events</dt>
                            <dd className="font-semibold">{selectedPreview?.event_count ?? 0}</dd>
                          </div>
                          <div className="flex flex-col gap-1">
                            <dt className="text-muted-foreground">Live tail</dt>
                            <dd className="font-semibold">
                              {liveTailEnabled ? liveTailRows.length : "paused"}
                            </dd>
                          </div>
                          <div className="flex flex-col gap-1">
                            <dt className="text-muted-foreground">First ts</dt>
                            <dd className="ui-selectable">
                              {formatDateTime(selectedPreview?.first_ts ?? null)}
                            </dd>
                          </div>
                          <div className="flex flex-col gap-1">
                            <dt className="text-muted-foreground">Last ts</dt>
                            <dd className="ui-selectable">
                              {formatDateTime(selectedPreview?.last_ts ?? null)}
                            </dd>
                          </div>
                        </dl>
                      </CardContent>
                    </Card>

                    <Card className={SURFACE_CARD_CLASS} size="sm">
                      <CardHeader className="gap-2">
                        <CardTitle className="text-sm">Live tail</CardTitle>
                      </CardHeader>
                      <CardContent className="flex flex-col gap-2">
                        {liveTailRows.map((event) => (
                          <div
                            className="rounded-md border border-border px-3 py-2"
                            key={`${event.seq}-${event.ts}`}
                          >
                            <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                              <span>{formatTime(event.ts)}</span>
                              <span className="font-mono">{event.eventType}</span>
                            </div>
                            <p className="ui-selectable mt-2 text-sm leading-6">
                              {event.text ?? event.fallback}
                            </p>
                          </div>
                        ))}

                        {!liveTailRows.length ? (
                          <Alert>
                            <RadioTower className="size-4" />
                            <AlertTitle>{liveTailEnabled ? "Tail idle" : "Tail paused"}</AlertTitle>
                            <AlertDescription>
                              {liveTailEnabled
                                ? "Новые tail-события появятся здесь после выбора сессии."
                                : "Автоматическое обновление выключено. Включите live tail кнопкой выше."}
                            </AlertDescription>
                          </Alert>
                        ) : null}
                      </CardContent>
                    </Card>
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
