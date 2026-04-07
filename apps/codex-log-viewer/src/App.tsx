import { memo, startTransition, useEffect, useRef, useState } from "react";

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

function PreviewEventRow({ event }: { event: SessionPreviewEvent }) {
  return (
    <article className={`preview-row tone-${event.category.toLowerCase()}`}>
      <div className="preview-kicker">
        <span>#{event.seq}</span>
        <time>{formatTime(event.ts)}</time>
        <code>{event.event_type}</code>
      </div>
      <p>{event.summary}</p>
      {event.text ? <small>{event.text}</small> : null}
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

const SessionSidebar = memo(
  function SessionSidebar({
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
      <aside className="panel sidebar">
        <div className="panel-header">
          <div>
            <p className="eyebrow">Catalog</p>
            <h2>Sessions</h2>
          </div>
          <button
            className="ghost-button"
            disabled={bootState !== "ready" || sessionsBusy || sessionsAppending}
            onClick={onRefresh}
            type="button"
          >
            Refresh
          </button>
        </div>
        <label className="search-field">
          <span>Query</span>
          <input
            onChange={(event) => onSearchChange(event.target.value)}
            placeholder="session id, thread name, cwd"
            type="search"
            value={search}
          />
        </label>
        <div className="panel-scroller">
          <div className="session-list">
            {sessions.map((session) => (
              <button
                key={session.session_ref}
                className={`session-card ${
                  session.session_ref === selectedSessionRef ? "selected" : ""
                }`}
                onClick={() => {
                  onOpenSession(session.session_ref);
                }}
                type="button"
              >
                <div className="session-headline">
                  <strong>{session.thread_name ?? session.session_id}</strong>
                  <span>{session.is_active_like ? "active-like" : "archived"}</span>
                </div>
                <p>{session.session_ref}</p>
                <div className="session-meta">
                  <span>{formatDateTime(session.updated_at)}</span>
                  <span>{session.index_status}</span>
                </div>
              </button>
            ))}
            {!sessions.length && !sessionsBusy ? (
              <div className="empty-state">
                <strong>Каталог пуст</strong>
                <p>После инициализации viewer здесь появятся rollout-сессии.</p>
              </div>
            ) : null}
          </div>
          {nextCursor ? (
            <div className="session-list-footer">
              <p className="catalog-note">Показано {sessions.length} сессий. Остальные догружаются по кнопке.</p>
              <button
                className="ghost-button"
                disabled={sessionsBusy || sessionsAppending}
                onClick={onLoadMore}
                type="button"
              >
                {sessionsAppending ? "Loading…" : "Load More"}
              </button>
            </div>
          ) : null}
          {diagnostics.length > 0 ? (
            <div className="diagnostic-list">
              {diagnostics.slice(0, 4).map((diagnostic, index) => (
                <article className="diagnostic-card" key={`${diagnostic.kind}-${index}`}>
                  <strong>{diagnostic.kind}</strong>
                  <p>{diagnostic.message}</p>
                </article>
              ))}
            </div>
          ) : null}
        </div>
      </aside>
    );
  },
  (prev, next) =>
    prev.bootState === next.bootState &&
    prev.search === next.search &&
    prev.sessions === next.sessions &&
    prev.diagnostics === next.diagnostics &&
    prev.selectedSessionRef === next.selectedSessionRef &&
    prev.sessionsBusy === next.sessionsBusy &&
    prev.sessionsAppending === next.sessionsAppending &&
    prev.nextCursor === next.nextCursor,
);

export function App() {
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
      setBootState("error");
      setBootError(extractErrorMessage(error));
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
      setBootState("error");
      setBootError(extractErrorMessage(error));
    } finally {
      if (requestId === sessionListRequestIdRef.current) {
        setSessionsAppending(false);
      }
    }
  }

  async function openSession(sessionRef: string) {
    selectedSessionRefRef.current = sessionRef;
    setSelectedSessionRef(sessionRef);
    setSessionBusy(true);
    setSessionError(null);
    setLiveEvents([]);
    try {
      const preview = await loadSessionPreview(sessionRef);
      if (selectedSessionRefRef.current !== sessionRef) {
        return;
      }
      applyPreview(preview);
      setTailStatus("Лёгкий preview загружен. Full tree path отключён для локализации лагов.");
    } catch (error) {
      if (selectedSessionRefRef.current === sessionRef) {
        setSessionError(extractErrorMessage(error));
      }
    } finally {
      setSessionBusy(false);
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
    return () => window.clearTimeout(timeoutId);
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
          startTransition(() => {
            setLiveEvents((prev) => [...result.events.reverse(), ...prev].slice(0, 40));
            setSelectedPreview((current) =>
              current
                ? {
                    ...current,
                    event_count: current.event_count + result.events.length,
                    last_ts: result.events[0]?.ts ?? current.last_ts,
                  }
                : current,
            );
          });
          setTailStatus(
            `Получено новых событий: ${result.events.length}. Full tree sync сейчас отключён.`,
          );
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
  }, [selectedSessionRef]);

  const liveTailRows = liveEvents.map(summarizeTailEvent);

  return (
    <main className="app-shell">
      <section className="masthead panel">
        <div>
          <p className="eyebrow">Codex Sessions</p>
          <h1>Log Viewer</h1>
          <p className="lead">
            Slim mode: viewer intentionally skips the full recursive event tree
            and renders only lightweight preview data plus live tail.
          </p>
        </div>
        <div className="status-grid">
          <div className="metric-card">
            <span>Backend</span>
            <strong>{bootState}</strong>
          </div>
          <div className="metric-card">
            <span>Detected home</span>
            <strong>{detectedHome ?? "not found"}</strong>
          </div>
          <div className="metric-card">
            <span>Mode</span>
            <strong>preview + tail</strong>
          </div>
        </div>
      </section>

      <section className="content-grid slim-grid">
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

        <section className="panel timeline-panel">
          <div className="panel-header">
            <div>
              <p className="eyebrow">Preview</p>
              <h2>{selectedPreview?.session_id ?? "No session selected"}</h2>
            </div>
            <button
              className="ghost-button"
              disabled={!selectedSessionRef || sessionBusy}
              onClick={() => {
                if (selectedSessionRef) {
                  void openSession(selectedSessionRef);
                }
              }}
              type="button"
            >
              {sessionBusy ? "Loading…" : "Refresh Preview"}
            </button>
          </div>

          {bootState === "needs_home" ? (
            <div className="empty-state prominent">
              <strong>CODEX_HOME не найден</strong>
              <p>Viewer ожидает уже существующий локальный `CODEX_HOME`.</p>
            </div>
          ) : null}

          {bootError ? (
            <div className="empty-state prominent error-state">
              <strong>Viewer bootstrap failed</strong>
              <p>{bootError}</p>
            </div>
          ) : null}

          {sessionError ? (
            <div className="empty-state prominent error-state">
              <strong>Session preview failed</strong>
              <p>{sessionError}</p>
            </div>
          ) : null}

          <div className="panel-scroller">
            {selectedPreview ? (
              <div className="preview-list">
                {selectedPreview.recent_events.map((event) => (
                  <PreviewEventRow
                    event={event}
                    key={`${event.seq}-${event.ts}-${event.event_type}`}
                  />
                ))}
              </div>
            ) : (
              <div className="empty-state prominent">
                <strong>Сессия не выбрана</strong>
                <p>В slim mode full tree отключён, поэтому показывается только лёгкий preview.</p>
              </div>
            )}
          </div>
        </section>

        <aside className="panel inspector">
          <div className="panel-header">
            <div>
              <p className="eyebrow">Inspector</p>
              <h2>Live State</h2>
            </div>
            {resolvedHome ? <span className="thread-pill subtle">initialized</span> : null}
          </div>
          <div className="panel-scroller">
            <div className="inspector-stack">
              <article className="info-card">
                <h3>Resolved home</h3>
                <p>{resolvedHome?.root ?? "not initialized"}</p>
                <small>{resolvedHome?.session_index_path ?? "session index unavailable"}</small>
              </article>

              <article className="info-card">
                <h3>Tail status</h3>
                <p>{tailStatus}</p>
                {tailCursor ? (
                  <small>
                    offset {tailCursor.offset} · next_seq {tailCursor.next_seq}
                  </small>
                ) : null}
              </article>

              <article className="info-card">
                <h3>Session summary</h3>
                <dl className="summary-grid">
                  <div>
                    <dt>Events</dt>
                    <dd>{selectedPreview?.event_count ?? 0}</dd>
                  </div>
                  <div>
                    <dt>First ts</dt>
                    <dd>{formatDateTime(selectedPreview?.first_ts ?? null)}</dd>
                  </div>
                  <div>
                    <dt>Last ts</dt>
                    <dd>{formatDateTime(selectedPreview?.last_ts ?? null)}</dd>
                  </div>
                  <div>
                    <dt>Live tail</dt>
                    <dd>{liveTailRows.length}</dd>
                  </div>
                </dl>
              </article>

              <article className="info-card">
                <h3>Live tail</h3>
                <div className="mini-list">
                  {liveTailRows.map((event) => (
                    <div className="mini-row" key={`${event.seq}-${event.ts}`}>
                      <span>{formatTime(event.ts)}</span>
                      <strong>{event.eventType}</strong>
                      <p>{event.text ?? event.fallback}</p>
                    </div>
                  ))}
                  {!liveTailRows.length ? (
                    <p className="mini-empty">Новые tail-события появятся здесь автоматически.</p>
                  ) : null}
                </div>
              </article>
            </div>
          </div>
        </aside>
      </section>
    </main>
  );
}
