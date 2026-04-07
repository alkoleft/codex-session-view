import { memo, startTransition, useEffect, useRef, useState } from "react";
import {
  FolderSearch2,
  Layers3,
  RadioTower,
  RefreshCcw,
  Search,
  Sparkles,
} from "lucide-react";

import {
  detectCodexHome,
  extractErrorMessage,
  initializeCodexHome,
  listSessions,
  loadSessionPreview,
  tailSession,
  type EventRecord,
  type ResolvedCodexHome,
  type SessionDiagnostic,
  type SessionPreview,
  type SessionPreviewEvent,
  type SessionSummary,
  type TailCursor,
} from "./backend";
import { cn } from "@/lib/utils";

type BootState = "booting" | "needs_home" | "ready" | "error";

const SESSIONS_PAGE_SIZE = 40;

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

function categoryTone(category: string) {
  switch (category.toLowerCase()) {
    case "system":
      return "tone-system";
    case "tool":
      return "tone-tool";
    case "error":
      return "tone-error";
    default:
      return "tone-message";
  }
}

function bootStateLabel(state: BootState) {
  switch (state) {
    case "booting":
      return "booting";
    case "needs_home":
      return "needs_home";
    case "ready":
      return "ready";
    case "error":
      return "error";
  }
}

function MetricCard({
  label,
  value,
}: {
  label: string;
  value: string;
}) {
  return (
    <article className="ui-card flex min-h-[112px] flex-col justify-between p-4">
      <span className="text-[0.72rem] font-semibold uppercase tracking-[0.16em] text-[color:var(--ink-soft)]">
        {label}
      </span>
      <strong className="ui-selectable text-base leading-snug">{value}</strong>
    </article>
  );
}

function PreviewEventCard({ event }: { event: SessionPreviewEvent }) {
  return (
    <article className="ui-card p-4">
      <div className="mb-3 flex items-center justify-between gap-3 text-xs text-[color:var(--ink-soft)]">
        <div className="flex items-center gap-2">
          <span className="ui-code-pill">#{event.seq}</span>
          <time>{formatTime(event.ts)}</time>
        </div>
        <span className={cn("ui-code-pill", categoryTone(event.category))}>
          {event.event_type}
        </span>
      </div>
      <p className="ui-selectable text-sm leading-6 text-[color:var(--ink)]">{event.summary}</p>
      {event.text ? (
        <small className="ui-selectable mt-3 block text-xs leading-5 text-[color:var(--ink-soft)]">
          {event.text}
        </small>
      ) : null}
    </article>
  );
}

type SessionSidebarProps = {
  bootState: BootState;
  search: string;
  sessions: SessionSummary[];
  diagnostics: SessionDiagnostic[];
  selectedSessionRef: string | null;
  sessionsBusy: boolean;
  sessionsAppending: boolean;
  nextCursor: string | null;
  onRefresh: () => void;
  onSearchChange: (value: string) => void;
  onOpenSession: (sessionRef: string) => void;
  onLoadMore: () => void;
};

const SessionSidebar = memo(function SessionSidebar({
  bootState,
  search,
  sessions,
  diagnostics,
  selectedSessionRef,
  sessionsBusy,
  sessionsAppending,
  nextCursor,
  onRefresh,
  onSearchChange,
  onOpenSession,
  onLoadMore,
}: SessionSidebarProps) {
  return (
    <aside className="ui-panel flex min-h-0 flex-col overflow-hidden p-5">
      <div className="mb-4 flex items-start justify-between gap-3">
        <div>
          <p className="ui-eyebrow">Catalog</p>
          <h2 className="mt-2 text-2xl font-semibold tracking-[-0.03em]">Sessions</h2>
        </div>
        <button
          className="ui-button"
          disabled={bootState !== "ready" || sessionsBusy || sessionsAppending}
          onClick={onRefresh}
          type="button"
        >
          <RefreshCcw className="h-4 w-4" />
          Refresh
        </button>
      </div>

      <label className="mb-4 block">
        <span className="mb-2 flex items-center gap-2 text-sm text-[color:var(--ink-soft)]">
          <Search className="h-4 w-4" />
          Query
        </span>
        <input
          className="ui-input"
          onChange={(event) => onSearchChange(event.target.value)}
          placeholder="session id, thread name, cwd"
          type="search"
          value={search}
        />
      </label>

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
        {sessions.map((session) => (
          <button
            key={session.session_ref}
            className={cn(
              "ui-card block w-full p-4 text-left transition hover:border-[#af672266]",
              session.session_ref === selectedSessionRef &&
                "border-[#af672288] bg-[rgba(250,240,225,0.92)]",
            )}
            onClick={() => {
              onOpenSession(session.session_ref);
            }}
            type="button"
          >
            <div className="mb-2 flex items-start justify-between gap-3">
              <strong className="ui-selectable text-sm leading-5">
                {session.thread_name ?? session.session_id}
              </strong>
              <span className="ui-code-pill">
                {session.is_active_like ? "active-like" : session.index_status}
              </span>
            </div>
            <p className="ui-selectable break-all text-xs leading-5 text-[color:var(--ink-soft)]">
              {session.session_ref}
            </p>
            <div className="mt-3 flex items-center justify-between gap-2 text-xs text-[color:var(--ink-soft)]">
              <span>{formatDateTime(session.updated_at)}</span>
              <span>{session.cwd ?? "cwd n/a"}</span>
            </div>
          </button>
        ))}

        {!sessions.length && !sessionsBusy ? (
          <div className="ui-card p-4 text-sm leading-6 text-[color:var(--ink-soft)]">
            <strong className="block text-[color:var(--ink)]">Каталог пуст</strong>
            <p className="mt-2">После инициализации здесь появятся rollout-сессии.</p>
          </div>
        ) : null}

        {nextCursor ? (
          <div className="ui-card p-4">
            <p className="text-sm leading-6 text-[color:var(--ink-soft)]">
              Показано {sessions.length} сессий. Остальные догружаются по кнопке.
            </p>
            <button
              className="ui-button mt-3"
              disabled={sessionsBusy || sessionsAppending}
              onClick={onLoadMore}
              type="button"
            >
              {sessionsAppending ? "Loading…" : "Load More"}
            </button>
          </div>
        ) : null}

        {diagnostics.length > 0 ? (
          <div className="space-y-3 pb-1">
            {diagnostics.slice(0, 4).map((diagnostic, index) => (
              <article
                className="ui-card border-[color:rgba(151,82,100,0.18)] p-4"
                key={`${diagnostic.kind}-${index}`}
              >
                <strong className="text-sm">{diagnostic.kind}</strong>
                <p className="mt-2 text-sm leading-6 text-[color:var(--ink-soft)]">
                  {diagnostic.message}
                </p>
              </article>
            ))}
          </div>
        ) : null}
      </div>
    </aside>
  );
});

export default function App() {
  const [bootState, setBootState] = useState<BootState>("booting");
  const [bootError, setBootError] = useState<string | null>(null);
  const [detectedHome, setDetectedHome] = useState<string | null>(null);
  const [resolvedHome, setResolvedHome] = useState<ResolvedCodexHome | null>(null);
  const [search, setSearch] = useState("");
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [diagnostics, setDiagnostics] = useState<SessionDiagnostic[]>([]);
  const [sessionsBusy, setSessionsBusy] = useState(false);
  const [sessionsAppending, setSessionsAppending] = useState(false);
  const [sessionsNextCursor, setSessionsNextCursor] = useState<string | null>(null);
  const [selectedSessionRef, setSelectedSessionRef] = useState<string | null>(null);
  const [selectedPreview, setSelectedPreview] = useState<SessionPreview | null>(null);
  const [sessionBusy, setSessionBusy] = useState(false);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [tailCursor, setTailCursor] = useState<TailCursor | null>(null);
  const [tailStatus, setTailStatus] = useState("Ожидание инициализации viewer backend.");
  const [liveEvents, setLiveEvents] = useState<EventRecord[]>([]);
  const tailCursorRef = useRef<TailCursor | null>(null);
  const selectedSessionRefRef = useRef<string | null>(null);
  const sessionListRequestIdRef = useRef(0);
  const sessionRequestIdRef = useRef(0);

  function applyPreview(preview: SessionPreview) {
    tailCursorRef.current = preview.tail_cursor;
    setTailCursor(preview.tail_cursor);
    startTransition(() => {
      setSelectedPreview(preview);
    });
  }

  async function refreshSessions(queryValue = search) {
    const requestId = ++sessionListRequestIdRef.current;
    setSessionsBusy(true);

    try {
      const page = await listSessions({
        query: queryValue,
        limit: SESSIONS_PAGE_SIZE,
      });

      if (requestId !== sessionListRequestIdRef.current) {
        return;
      }

      startTransition(() => {
        setSessions(page.items);
        setDiagnostics(page.diagnostics);
        setSessionsNextCursor(page.next_cursor);
      });

      if (!selectedSessionRefRef.current && page.items[0]) {
        void openSession(page.items[0].session_ref);
      }
    } catch (error) {
      if (requestId === sessionListRequestIdRef.current) {
        setBootState("error");
        setBootError(extractErrorMessage(error));
      }
    } finally {
      if (requestId === sessionListRequestIdRef.current) {
        setSessionsBusy(false);
      }
    }
  }

  async function loadMoreSessions() {
    if (!sessionsNextCursor || sessionsBusy || sessionsAppending) {
      return;
    }

    const requestId = ++sessionListRequestIdRef.current;
    setSessionsAppending(true);

    try {
      const page = await listSessions({
        query: search,
        cursor: sessionsNextCursor,
        limit: SESSIONS_PAGE_SIZE,
      });

      if (requestId !== sessionListRequestIdRef.current) {
        return;
      }

      startTransition(() => {
        setSessions((current) => [...current, ...page.items]);
        setSessionsNextCursor(page.next_cursor);
      });
    } catch (error) {
      if (requestId === sessionListRequestIdRef.current) {
        setBootState("error");
        setBootError(extractErrorMessage(error));
      }
    } finally {
      setSessionsAppending(false);
    }
  }

  async function openSession(sessionRef: string) {
    const requestId = ++sessionRequestIdRef.current;
    selectedSessionRefRef.current = sessionRef;
    setSelectedSessionRef(sessionRef);
    setSessionBusy(true);
    setSessionError(null);
    setLiveEvents([]);
    tailCursorRef.current = null;
    setTailCursor(null);

    try {
      const preview = await loadSessionPreview(sessionRef);
      if (
        selectedSessionRefRef.current !== sessionRef ||
        requestId !== sessionRequestIdRef.current
      ) {
        return;
      }

      applyPreview(preview);
      setTailStatus("Slim preview загружен. Full tree path по-прежнему отключён.");
    } catch (error) {
      if (
        selectedSessionRefRef.current === sessionRef &&
        requestId === sessionRequestIdRef.current
      ) {
        setSessionError(extractErrorMessage(error));
      }
    } finally {
      if (requestId === sessionRequestIdRef.current) {
        setSessionBusy(false);
      }
    }
  }

  useEffect(() => {
    let active = true;

    async function bootstrap() {
      try {
        const detected = await detectCodexHome();
        if (!active) {
          return;
        }

        setDetectedHome(detected.detected_home);
        if (!detected.detected_home) {
          setBootState("needs_home");
          setTailStatus("CODEX_HOME не найден; viewer не может открыть session catalog.");
          return;
        }

        const initialized = await initializeCodexHome();
        if (!active) {
          return;
        }

        setResolvedHome(initialized.resolved_home);
        setBootState("ready");
        setTailStatus("Backend инициализирован. Загружаю список сессий.");
        await refreshSessions("");
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
    if (bootState !== "ready") {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      void refreshSessions(search);
    }, 250);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [search, bootState]);

  useEffect(() => {
    if (!selectedSessionRef || !tailCursorRef.current) {
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
            setLiveEvents((prev) => [...nextLiveEvents, ...prev].slice(0, 40));
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
          timeoutId = window.setTimeout(poll, 2500);
        }
      }
    }

    timeoutId = window.setTimeout(poll, 2500);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [selectedSessionRef, tailCursor]);

  const liveTailRows = liveEvents.map(summarizeTailEvent);

  return (
    <main data-ui-scroll-container className="h-full px-4 py-4 sm:px-6 sm:py-6">
      <div className="mx-auto flex max-w-[1520px] min-h-full flex-col gap-5">
        <section className="ui-panel grid gap-5 p-6 lg:grid-cols-[1.5fr_0.95fr] lg:p-7">
          <div>
            <div className="mb-4 flex items-center gap-3">
              <span className="ui-code-pill tone-tool">
                <Sparkles className="mr-1 inline h-3.5 w-3.5" />
                tauri-ui shell
              </span>
              <span className="ui-code-pill">preview + tail</span>
            </div>
            <p className="ui-eyebrow">Codex Sessions</p>
            <h1 className="mt-3 max-w-[12ch] text-4xl font-semibold tracking-[-0.06em] sm:text-6xl">
              Log Viewer Tauri UI
            </h1>
            <p className="ui-selectable mt-5 max-w-[62ch] text-sm leading-7 text-[color:var(--ink-soft)] sm:text-[15px]">
              Отдельное desktop-приложение на базе паттернов `agmmnn/tauri-ui`: собственный
              shell, скрытие окна до первого paint и scroll container, но тот же read-only
              backend contract для каталога сессий, preview и live tail.
            </p>
          </div>

          <div className="grid gap-3 sm:grid-cols-3 lg:grid-cols-1 xl:grid-cols-3">
            <MetricCard label="Backend" value={bootStateLabel(bootState)} />
            <MetricCard label="Detected home" value={detectedHome ?? "not found"} />
            <MetricCard label="Tail mode" value="preview + live tail" />
          </div>
        </section>

        <section className="grid min-h-0 gap-5 xl:flex-1 xl:grid-cols-[340px_minmax(0,1fr)_360px]">
          <SessionSidebar
            bootState={bootState}
            diagnostics={diagnostics}
            nextCursor={sessionsNextCursor}
            onLoadMore={() => {
              void loadMoreSessions();
            }}
            onOpenSession={(sessionRef) => {
              void openSession(sessionRef);
            }}
            onRefresh={() => {
              void refreshSessions(search);
            }}
            onSearchChange={setSearch}
            search={search}
            selectedSessionRef={selectedSessionRef}
            sessions={sessions}
            sessionsAppending={sessionsAppending}
            sessionsBusy={sessionsBusy}
          />

          <section className="ui-panel flex min-h-0 flex-col overflow-hidden p-5">
            <div className="mb-4 flex items-start justify-between gap-3">
              <div>
                <p className="ui-eyebrow">Preview</p>
                <h2 className="mt-2 text-2xl font-semibold tracking-[-0.03em]">
                  {selectedPreview?.session_id ?? "No session selected"}
                </h2>
              </div>
              <button
                className="ui-button ui-button-primary"
                disabled={!selectedSessionRef || sessionBusy}
                onClick={() => {
                  if (selectedSessionRef) {
                    void openSession(selectedSessionRef);
                  }
                }}
                type="button"
              >
                <RefreshCcw className="h-4 w-4" />
                {sessionBusy ? "Loading…" : "Refresh Preview"}
              </button>
            </div>

            {bootState === "needs_home" ? (
              <div className="ui-card mb-4 p-4 text-sm leading-6 text-[color:var(--ink-soft)]">
                <strong className="block text-[color:var(--ink)]">CODEX_HOME не найден</strong>
                <p className="mt-2">Viewer ожидает уже существующий локальный `CODEX_HOME`.</p>
              </div>
            ) : null}

            {bootError ? (
              <div className="ui-card mb-4 border-[color:rgba(151,82,100,0.18)] p-4 text-sm leading-6 text-[color:var(--ink-soft)]">
                <strong className="block text-[color:var(--ink)]">Viewer bootstrap failed</strong>
                <p className="ui-selectable mt-2">{bootError}</p>
              </div>
            ) : null}

            {sessionError ? (
              <div className="ui-card mb-4 border-[color:rgba(151,82,100,0.18)] p-4 text-sm leading-6 text-[color:var(--ink-soft)]">
                <strong className="block text-[color:var(--ink)]">Session preview failed</strong>
                <p className="ui-selectable mt-2">{sessionError}</p>
              </div>
            ) : null}

            <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
              {selectedPreview ? (
                selectedPreview.recent_events.map((event) => (
                  <PreviewEventCard
                    event={event}
                    key={`${event.seq}-${event.ts}-${event.event_type}`}
                  />
                ))
              ) : (
                <div className="ui-card p-5 text-sm leading-6 text-[color:var(--ink-soft)]">
                  <strong className="block text-[color:var(--ink)]">Сессия не выбрана</strong>
                  <p className="mt-2">
                    Новый Tauri UI сохраняет slim mode: рендерит только лёгкий preview и live tail
                    без full tree rendering.
                  </p>
                </div>
              )}
            </div>
          </section>

          <aside className="ui-panel flex min-h-0 flex-col overflow-hidden p-5">
            <div className="mb-4 flex items-start justify-between gap-3">
              <div>
                <p className="ui-eyebrow">Inspector</p>
                <h2 className="mt-2 text-2xl font-semibold tracking-[-0.03em]">Live State</h2>
              </div>
              {resolvedHome ? <span className="ui-code-pill tone-system">initialized</span> : null}
            </div>

            <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
              <article className="ui-card p-4">
                <div className="mb-3 flex items-center gap-2">
                  <FolderSearch2 className="h-4 w-4 text-[color:var(--accent)]" />
                  <h3 className="text-sm font-semibold">Resolved home</h3>
                </div>
                <p className="ui-selectable break-all text-sm leading-6">
                  {resolvedHome?.root ?? "not initialized"}
                </p>
                <small className="ui-selectable mt-2 block break-all text-xs leading-5 text-[color:var(--ink-soft)]">
                  {resolvedHome?.session_index_path ?? "session index unavailable"}
                </small>
              </article>

              <article className="ui-card p-4">
                <div className="mb-3 flex items-center gap-2">
                  <RadioTower className="h-4 w-4 text-[color:var(--emerald)]" />
                  <h3 className="text-sm font-semibold">Tail status</h3>
                </div>
                <p className="text-sm leading-6 text-[color:var(--ink)]">{tailStatus}</p>
                {tailCursor ? (
                  <small className="mt-2 block text-xs leading-5 text-[color:var(--ink-soft)]">
                    offset {tailCursor.offset} · next_seq {tailCursor.next_seq}
                  </small>
                ) : null}
              </article>

              <article className="ui-card p-4">
                <div className="mb-3 flex items-center gap-2">
                  <Layers3 className="h-4 w-4 text-[color:var(--plum)]" />
                  <h3 className="text-sm font-semibold">Session summary</h3>
                </div>
                <dl className="grid grid-cols-2 gap-3 text-sm">
                  <div>
                    <dt className="text-[color:var(--ink-soft)]">Events</dt>
                    <dd className="mt-1 font-semibold">{selectedPreview?.event_count ?? 0}</dd>
                  </div>
                  <div>
                    <dt className="text-[color:var(--ink-soft)]">Live tail</dt>
                    <dd className="mt-1 font-semibold">{liveTailRows.length}</dd>
                  </div>
                  <div>
                    <dt className="text-[color:var(--ink-soft)]">First ts</dt>
                    <dd className="ui-selectable mt-1">
                      {formatDateTime(selectedPreview?.first_ts ?? null)}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-[color:var(--ink-soft)]">Last ts</dt>
                    <dd className="ui-selectable mt-1">
                      {formatDateTime(selectedPreview?.last_ts ?? null)}
                    </dd>
                  </div>
                </dl>
              </article>

              <article className="ui-card p-4">
                <div className="mb-3 flex items-center gap-2">
                  <Sparkles className="h-4 w-4 text-[color:var(--accent)]" />
                  <h3 className="text-sm font-semibold">Live tail</h3>
                </div>
                <div className="space-y-3">
                  {liveTailRows.map((event) => (
                    <div
                      className="rounded-[18px] border border-[color:var(--line)] bg-white/55 p-3"
                      key={`${event.seq}-${event.ts}`}
                    >
                      <div className="mb-2 flex items-center justify-between gap-3 text-xs text-[color:var(--ink-soft)]">
                        <span>{formatTime(event.ts)}</span>
                        <span className="ui-code-pill">{event.eventType}</span>
                      </div>
                      <p className="ui-selectable text-sm leading-6 text-[color:var(--ink)]">
                        {event.text ?? event.fallback}
                      </p>
                    </div>
                  ))}

                  {!liveTailRows.length ? (
                    <p className="text-sm leading-6 text-[color:var(--ink-soft)]">
                      Новые tail-события появятся здесь автоматически после выбора сессии.
                    </p>
                  ) : null}
                </div>
              </article>
            </div>
          </aside>
        </section>
      </div>
    </main>
  );
}
