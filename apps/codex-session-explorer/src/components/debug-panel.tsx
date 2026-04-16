import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Bug,
  ExternalLink,
  PanelBottom,
  PanelRightClose,
  RefreshCcw,
  TerminalSquare,
  X,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  DEBUG_EVENT_NAME,
  emitDebugEvent,
  serializeError,
  type DebugEvent,
  type InvokeDebugEvent,
  type LogDebugEvent,
} from "@/lib/debug-events";
import { isTauri } from "@/lib/tauri";
import { cn } from "@/lib/utils";

type DebugDock = "right" | "bottom";

type AppSnapshot = {
  name: string | null;
  version: string | null;
  identifier: string | null;
  tauriVersion: string | null;
  windowLabel: string | null;
  windowTitle: string | null;
  viewport: string;
  appDataDir: string | null;
  appConfigDir: string | null;
  resourceDir: string | null;
};

type ErrorEntry = {
  id: string;
  timestamp: string;
  source: "error" | "unhandledrejection";
  message: string;
};

const MAX_ITEMS = 12;
const DEBUG_DOCK_KEY = "ctui-debug-dock";

function pushRecent<T>(items: T[], nextItem: T) {
  return [nextItem, ...items].slice(0, MAX_ITEMS);
}

function formatTimestamp(value: string) {
  try {
    return new Intl.DateTimeFormat("ru-RU", {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    }).format(new Date(value));
  } catch {
    return value;
  }
}

function isTypingTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  return (
    target.isContentEditable ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  );
}

function stringifyValue(value: unknown) {
  if (value === null || value === undefined) {
    return "n/a";
  }

  if (typeof value === "string") {
    return value;
  }

  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

async function attachTauriConsoleBridge() {
  const { attachConsole } = await import("@tauri-apps/plugin-log");
  return attachConsole();
}

async function loadTauriSnapshot() {
  const [{ getIdentifier, getName, getTauriVersion, getVersion }, pathApi, windowApi] =
    await Promise.all([
      import("@tauri-apps/api/app"),
      import("@tauri-apps/api/path"),
      import("@tauri-apps/api/window"),
    ]);

  const currentWindow = windowApi.getCurrentWindow();
  const [name, version, identifier, tauriVersion, title, appDataPath, appConfigPath, resourcePath] =
    await Promise.all([
      getName(),
      getVersion(),
      getIdentifier(),
      getTauriVersion(),
      currentWindow.title(),
      pathApi.appDataDir(),
      pathApi.appConfigDir(),
      pathApi.resourceDir(),
    ]);

  return {
    name,
    version,
    identifier,
    tauriVersion,
    windowLabel: currentWindow.label,
    windowTitle: title,
    appDataDir: appDataPath,
    appConfigDir: appConfigPath,
    resourceDir: resourcePath,
  };
}

function DebugSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="ui-card flex min-h-0 flex-col p-4">
      <h3 className="text-sm font-semibold tracking-[-0.02em]">{title}</h3>
      <div className="mt-3 min-h-0 flex-1">{children}</div>
    </section>
  );
}

function KeyValueList({
  entries,
}: {
  entries: Array<[label: string, value: string | null]>;
}) {
  return (
    <dl className="grid gap-2 text-sm">
      {entries.map(([label, value]) => (
        <div className="grid grid-cols-[108px_minmax(0,1fr)] gap-3" key={label}>
          <dt className="text-[color:var(--ink-soft)]">{label}</dt>
          <dd className="ui-selectable break-all">{value ?? "n/a"}</dd>
        </div>
      ))}
    </dl>
  );
}

export function DebugPanel() {
  const [open, setOpen] = useState(false);
  const [dock, setDock] = useState<DebugDock>(() => {
    const savedDock = window.sessionStorage.getItem(DEBUG_DOCK_KEY);
    return savedDock === "bottom" || savedDock === "right" ? savedDock : "right";
  });
  const [snapshot, setSnapshot] = useState<AppSnapshot>({
    name: null,
    version: null,
    identifier: null,
    tauriVersion: null,
    windowLabel: null,
    windowTitle: null,
    viewport: `${window.innerWidth} x ${window.innerHeight}`,
    appDataDir: null,
    appConfigDir: null,
    resourceDir: null,
  });
  const [locationHref, setLocationHref] = useState(window.location.href);
  const [invokeLogs, setInvokeLogs] = useState<InvokeDebugEvent[]>([]);
  const [externalLinks, setExternalLinks] = useState<string[]>([]);
  const [logEntries, setLogEntries] = useState<LogDebugEvent[]>([]);
  const [errors, setErrors] = useState<ErrorEntry[]>([]);

  useEffect(() => {
    window.sessionStorage.setItem(DEBUG_DOCK_KEY, dock);
  }, [dock]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented || event.repeat) {
        return;
      }

      if (!(event.metaKey || event.ctrlKey) || event.shiftKey || event.altKey) {
        return;
      }

      if (event.key.toLowerCase() !== "d" || isTypingTarget(event.target)) {
        return;
      }

      event.preventDefault();
      setOpen((current) => !current);
    }

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  useEffect(() => {
    function updateLocation() {
      setLocationHref(window.location.href);
    }

    window.addEventListener("hashchange", updateLocation);
    window.addEventListener("popstate", updateLocation);

    return () => {
      window.removeEventListener("hashchange", updateLocation);
      window.removeEventListener("popstate", updateLocation);
    };
  }, []);

  useEffect(() => {
    function handleDebugEvent(event: Event) {
      const debugEvent = (event as CustomEvent<DebugEvent>).detail;

      if (debugEvent.kind === "external-link") {
        setExternalLinks((current) => pushRecent(current, debugEvent.href));
        return;
      }

      if (debugEvent.kind === "log") {
        setLogEntries((current) => pushRecent(current, debugEvent));
        return;
      }

      setInvokeLogs((current) => pushRecent(current, debugEvent));
    }

    function handleError(event: ErrorEvent) {
      setErrors((current) =>
        pushRecent(current, {
          id: crypto.randomUUID(),
          timestamp: new Date().toISOString(),
          source: "error",
          message: event.message || "Unknown runtime error",
        }),
      );
    }

    function handleRejection(event: PromiseRejectionEvent) {
      const reason = serializeError(event.reason);

      setErrors((current) =>
        pushRecent(current, {
          id: crypto.randomUUID(),
          timestamp: new Date().toISOString(),
          source: "unhandledrejection",
          message: reason.message,
        }),
      );
    }

    window.addEventListener(DEBUG_EVENT_NAME, handleDebugEvent as EventListener);
    window.addEventListener("error", handleError);
    window.addEventListener("unhandledrejection", handleRejection);

    return () => {
      window.removeEventListener(DEBUG_EVENT_NAME, handleDebugEvent as EventListener);
      window.removeEventListener("error", handleError);
      window.removeEventListener("unhandledrejection", handleRejection);
    };
  }, []);

  useEffect(() => {
    const originalConsole = {
      log: window.console.log,
      info: window.console.info,
      warn: window.console.warn,
      error: window.console.error,
      debug: window.console.debug,
    };

    function capture(
      level: LogDebugEvent["level"],
      args: unknown[],
      originalMethod: (...data: unknown[]) => void,
    ) {
      const message = args
        .map((value) => {
          if (typeof value === "string") {
            return value;
          }

          try {
            return JSON.stringify(value);
          } catch {
            return String(value);
          }
        })
        .join(" ");

      emitDebugEvent({
        id: crypto.randomUUID(),
        kind: "log",
        level,
        message,
        timestamp: new Date().toISOString(),
      });

      originalMethod(...args);
    }

    window.console.log = (...args: unknown[]) => {
      capture("log", args, originalConsole.log);
    };
    window.console.info = (...args: unknown[]) => {
      capture("info", args, originalConsole.info);
    };
    window.console.warn = (...args: unknown[]) => {
      capture("warn", args, originalConsole.warn);
    };
    window.console.error = (...args: unknown[]) => {
      capture("error", args, originalConsole.error);
    };
    window.console.debug = (...args: unknown[]) => {
      capture("debug", args, originalConsole.debug);
    };

    let detachConsole: (() => void) | undefined;

    if (isTauri()) {
      void attachTauriConsoleBridge()
        .then((detach) => {
          detachConsole = detach;
        })
        .catch((error) => {
          emitDebugEvent({
            id: crypto.randomUUID(),
            kind: "log",
            level: "error",
            message: `attachConsole failed: ${serializeError(error).message}`,
            timestamp: new Date().toISOString(),
          });
        });
    }

    return () => {
      detachConsole?.();
      window.console.log = originalConsole.log;
      window.console.info = originalConsole.info;
      window.console.warn = originalConsole.warn;
      window.console.error = originalConsole.error;
      window.console.debug = originalConsole.debug;
    };
  }, []);

  async function loadSnapshot() {
    const nextSnapshot: AppSnapshot = {
      name: null,
      version: null,
      identifier: null,
      tauriVersion: null,
      windowLabel: null,
      windowTitle: null,
      viewport: `${window.innerWidth} x ${window.innerHeight}`,
      appDataDir: null,
      appConfigDir: null,
      resourceDir: null,
    };

    if (isTauri()) {
      Object.assign(nextSnapshot, await loadTauriSnapshot());
    }

    return nextSnapshot;
  }

  function refreshSnapshot() {
    void loadSnapshot()
      .then((nextSnapshot) => {
        setSnapshot(nextSnapshot);
      })
      .catch((error) => {
        setErrors((current) =>
          pushRecent(current, {
            id: crypto.randomUUID(),
            timestamp: new Date().toISOString(),
            source: "unhandledrejection",
            message: `snapshot refresh failed: ${serializeError(error).message}`,
          }),
        );
      });
  }

  useEffect(() => {
    let cancelled = false;

    void loadSnapshot()
      .then((nextSnapshot) => {
        if (!cancelled) {
          setSnapshot(nextSnapshot);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setErrors((current) =>
            pushRecent(current, {
              id: crypto.randomUUID(),
              timestamp: new Date().toISOString(),
              source: "unhandledrejection",
              message: `initial snapshot failed: ${serializeError(error).message}`,
            }),
          );
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  if (!open) {
    return (
      <div className="fixed right-4 bottom-4 z-50">
        <Button
          className="rounded-full shadow-[0_16px_48px_rgba(24,35,32,0.18)]"
          onClick={() => {
            setOpen(true);
          }}
          size="sm"
          variant="secondary"
        >
          <TerminalSquare data-icon="inline-start" />
          Debug
        </Button>
      </div>
    );
  }

  return (
    <aside
      className={cn(
        "ui-selectable fixed z-50 flex overflow-hidden rounded-[24px] border border-[color:var(--line-strong)] bg-[color:var(--panel-strong)] text-[color:var(--ink)] shadow-[0_24px_70px_rgba(24,35,32,0.22)] backdrop-blur-xl",
        dock === "right" ? "top-4 right-4 bottom-4 w-[420px]" : "right-4 bottom-4 left-4 h-[340px]",
      )}
    >
      <div className="flex min-h-0 flex-1 flex-col p-4">
        <div className="mb-4 flex items-start justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <TerminalSquare className="h-4 w-4 text-[color:var(--accent-strong)]" />
              <h2 className="text-sm font-semibold tracking-[-0.02em]">Tauri UI Debug Panel</h2>
            </div>
            <p className="mt-1 text-xs leading-5 text-[color:var(--ink-soft)]">
              `Cmd/Ctrl + D` переключает панель. Здесь собираются invoke, внешние ссылки,
              ошибки и Rust logs из `tauri-plugin-log`.
            </p>
          </div>

          <div className="flex items-center gap-2">
            <Badge variant="secondary">DEV</Badge>
            <Badge variant={isTauri() ? "outline" : "destructive"}>
              {isTauri() ? "Tauri" : "Browser"}
            </Badge>
          </div>
        </div>

        <div className="mb-4 flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Button
              onClick={() => {
                void refreshSnapshot();
              }}
              size="icon-sm"
              title="Refresh panel snapshot"
              variant="outline"
            >
              <RefreshCcw />
            </Button>
            <Button
              onClick={() => {
                setDock((current) => (current === "right" ? "bottom" : "right"));
              }}
              size="icon-sm"
              title={dock === "right" ? "Dock bottom" : "Dock right"}
              variant="outline"
            >
              {dock === "right" ? <PanelBottom /> : <PanelRightClose />}
            </Button>
          </div>

          <Button
            onClick={() => {
              setOpen(false);
            }}
            size="icon-sm"
            title="Close debug panel"
            variant="ghost"
          >
            <X />
          </Button>
        </div>

        <Tabs className="min-h-0 flex-1" defaultValue="overview">
          <TabsList className="grid w-full grid-cols-3">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="runtime">Runtime</TabsTrigger>
            <TabsTrigger value="errors">Errors</TabsTrigger>
          </TabsList>

          <TabsContent className="min-h-0 flex-1 pt-3" value="overview">
            <div
              className={cn(
                "grid min-h-0 gap-3",
                dock === "bottom" ? "md:grid-cols-3" : "grid-cols-1",
              )}
            >
              <DebugSection title="App">
                <KeyValueList
                  entries={[
                    ["name", snapshot.name],
                    ["version", snapshot.version],
                    ["identifier", snapshot.identifier],
                    ["tauri", snapshot.tauriVersion],
                    ["window", snapshot.windowLabel],
                    ["title", snapshot.windowTitle],
                    ["viewport", snapshot.viewport],
                  ]}
                />
              </DebugSection>

              <DebugSection title="Location">
                <p className="ui-selectable break-all text-sm leading-6">{locationHref}</p>
              </DebugSection>

              <DebugSection title="Paths">
                <KeyValueList
                  entries={[
                    ["data", snapshot.appDataDir],
                    ["config", snapshot.appConfigDir],
                    ["resource", snapshot.resourceDir],
                  ]}
                />
              </DebugSection>
            </div>
          </TabsContent>

          <TabsContent className="min-h-0 flex-1 pt-3" value="runtime">
            <div
              className={cn(
                "grid min-h-0 gap-3",
                dock === "bottom" ? "md:grid-cols-3" : "grid-cols-1",
              )}
            >
              <DebugSection title={`invoke() (${invokeLogs.length})`}>
                <div className="flex h-full min-h-0 flex-col gap-3 overflow-y-auto pr-1">
                  {invokeLogs.length ? (
                    invokeLogs.map((entry) => (
                      <article className="rounded-[18px] border border-[color:var(--line)] bg-white/60 p-3" key={entry.id}>
                        <div className="mb-2 flex items-center justify-between gap-2">
                          <Badge variant={entry.status === "success" ? "secondary" : "destructive"}>
                            {entry.status}
                          </Badge>
                          <span className="text-xs text-[color:var(--ink-soft)]">
                            {formatTimestamp(entry.timestamp)}
                          </span>
                        </div>
                        <p className="font-medium">{entry.command}</p>
                        <pre className="ui-selectable mt-2 overflow-x-auto text-xs leading-5 whitespace-pre-wrap text-[color:var(--ink-soft)]">
                          {stringifyValue(entry.status === "success" ? entry.result : entry.error ?? entry.args)}
                        </pre>
                      </article>
                    ))
                  ) : (
                    <p className="text-sm leading-6 text-[color:var(--ink-soft)]">
                      Пока нет tracked invoke вызовов.
                    </p>
                  )}
                </div>
              </DebugSection>

              <DebugSection title={`External Links (${externalLinks.length})`}>
                <div className="flex h-full min-h-0 flex-col gap-3 overflow-y-auto pr-1">
                  {externalLinks.length ? (
                    externalLinks.map((href, index) => (
                      <div
                        className="rounded-[18px] border border-[color:var(--line)] bg-white/60 p-3 text-sm leading-6"
                        key={`${href}-${index}`}
                      >
                        <div className="mb-2 flex items-center gap-2">
                          <ExternalLink className="h-4 w-4 text-[color:var(--accent-strong)]" />
                          <span className="font-medium">system browser</span>
                        </div>
                        <p className="ui-selectable break-all text-[color:var(--ink-soft)]">{href}</p>
                      </div>
                    ))
                  ) : (
                    <p className="text-sm leading-6 text-[color:var(--ink-soft)]">
                      Кликните по внешней ссылке в приложении, и событие появится здесь.
                    </p>
                  )}
                </div>
              </DebugSection>

              <DebugSection title={`Logs (${logEntries.length})`}>
                <div className="flex h-full min-h-0 flex-col gap-3 overflow-y-auto pr-1">
                  {logEntries.length ? (
                    logEntries.map((entry) => (
                      <article className="rounded-[18px] border border-[color:var(--line)] bg-white/60 p-3" key={entry.id}>
                        <div className="mb-2 flex items-center justify-between gap-2">
                          <Badge
                            variant={
                              entry.level === "error"
                                ? "destructive"
                                : entry.level === "warn"
                                  ? "outline"
                                  : "secondary"
                            }
                          >
                            {entry.level}
                          </Badge>
                          <span className="text-xs text-[color:var(--ink-soft)]">
                            {formatTimestamp(entry.timestamp)}
                          </span>
                        </div>
                        <p className="ui-selectable break-all text-sm leading-6">{entry.message}</p>
                      </article>
                    ))
                  ) : (
                    <p className="text-sm leading-6 text-[color:var(--ink-soft)]">
                      Логи появятся после frontend console сообщений или Rust log targets.
                    </p>
                  )}
                </div>
              </DebugSection>
            </div>
          </TabsContent>

          <TabsContent className="min-h-0 flex-1 pt-3" value="errors">
            <DebugSection title={`Errors (${errors.length})`}>
              <div className="flex h-full min-h-0 flex-col gap-3 overflow-y-auto pr-1">
                {errors.length ? (
                  errors.map((entry) => (
                    <article className="rounded-[18px] border border-[color:rgba(151,82,100,0.22)] bg-[rgba(151,82,100,0.06)] p-3" key={entry.id}>
                      <div className="mb-2 flex items-center justify-between gap-2">
                        <div className="flex items-center gap-2">
                          <AlertTriangle className="h-4 w-4 text-[color:var(--rose)]" />
                          <Badge variant="destructive">{entry.source}</Badge>
                        </div>
                        <span className="text-xs text-[color:var(--ink-soft)]">
                          {formatTimestamp(entry.timestamp)}
                        </span>
                      </div>
                      <p className="ui-selectable break-all text-sm leading-6">{entry.message}</p>
                    </article>
                  ))
                ) : (
                  <div className="flex h-full items-center justify-center rounded-[18px] border border-dashed border-[color:var(--line)] bg-white/40 p-6 text-center text-sm leading-6 text-[color:var(--ink-soft)]">
                    <div>
                      <Bug className="mx-auto mb-3 h-5 w-5 text-[color:var(--ink-soft)]" />
                      Непойманных runtime ошибок пока нет.
                    </div>
                  </div>
                )}
              </div>
            </DebugSection>
          </TabsContent>
        </Tabs>
      </div>
    </aside>
  );
}
