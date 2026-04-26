import { afterEach, describe, expect, it, vi } from "vitest";

import { RemoteViewerBackendClient } from "@/backend-remote";

const baseUrl = "https://viewer.example.test";

describe("RemoteViewerBackendClient", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("initializes without CODEX_HOME and loads indexed sessions via HTTP JSON", async () => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [],
          next_cursor: null,
          diagnostics: [],
        }),
        {
          status: 200,
          headers: {
            "content-type": "application/json",
          },
        },
      ),
    );

    const client = new RemoteViewerBackendClient({
      baseUrl,
      fetch: fetchMock,
    });

    await expect(client.initialize()).resolves.toMatchObject({
      mode: "remote",
      status: "ready",
      resolvedHome: null,
    });

    await expect(
      client.listIndexedSessions({
        query: " task-ui ",
        exactSessionId: " session-id ",
        cursor: "cursor-1",
        limit: 10,
      }),
    ).resolves.toEqual({
      items: [],
      next_cursor: null,
      diagnostics: [],
    });

    expect(fetchMock).toHaveBeenCalledWith(`${baseUrl}/api/viewer/list_indexed_sessions`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify({
        limit: 10,
        query: "task-ui",
        exact_session_id: "session-id",
        cursor: "cursor-1",
      }),
    });
  });

  it("falls back to the browser origin when no base URL is configured", async () => {
    vi.stubGlobal("window", {
      location: {
        origin: "http://localhost",
      },
    });

    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [],
          next_cursor: null,
          diagnostics: [],
        }),
        {
          status: 200,
          headers: {
            "content-type": "application/json",
          },
        },
      ),
    );

    const client = new RemoteViewerBackendClient({
      baseUrl: null,
      fetch: fetchMock,
    });

    await expect(client.initialize()).resolves.toMatchObject({
      mode: "remote",
      status: "ready",
    });

    await client.listIndexedSessions();

    expect(fetchMock).toHaveBeenCalledWith(
      "http://localhost/api/viewer/list_indexed_sessions",
      expect.objectContaining({
        method: "POST",
      }),
    );
  });

  it("uses window.fetch without losing the browser binding", async () => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [],
          next_cursor: null,
          diagnostics: [],
        }),
        {
          status: 200,
          headers: {
            "content-type": "application/json",
          },
        },
      ),
    );

    vi.stubGlobal("window", {
      location: {
        origin: "http://localhost",
      },
      fetch: fetchMock,
    });

    const client = new RemoteViewerBackendClient({
      baseUrl: null,
    });

    await client.listIndexedSessions();

    expect(fetchMock).toHaveBeenCalledWith(
      "http://localhost/api/viewer/list_indexed_sessions",
      expect.objectContaining({
        method: "POST",
      }),
    );
  });

  it("uses JSON error payloads from the remote API", async () => {
    const client = new RemoteViewerBackendClient({
      baseUrl,
      fetch: vi.fn<typeof fetch>().mockResolvedValue(
        new Response(JSON.stringify({ message: "backend exploded" }), {
          status: 500,
          headers: {
            "content-type": "application/json",
          },
        }),
      ),
    });

    await expect(client.loadSession("sessions/demo.jsonl")).rejects.toThrow("backend exploded");
  });

  it("loads project-metrics pinned-session detail via the dedicated endpoint", async () => {
    const fetchMock = vi.fn<typeof fetch>().mockResolvedValue(
      new Response(
        JSON.stringify({
          session_id: "session-1",
          session_ref: "2026/04/23/rollout-session-1.jsonl",
          title: "Implement parser fix",
          start_user_request: "Fix the parser regression",
          start_user_request_source: "indexed_first_user_message",
          task_summary: "Implement parser fix",
          task_summary_source: "indexed_title",
          agent_role: "worker",
          task_class: "implementation",
          task_class_confidence: "confident",
        }),
        {
          status: 200,
          headers: {
            "content-type": "application/json",
          },
        },
      ),
    );

    const client = new RemoteViewerBackendClient({
      baseUrl,
      fetch: fetchMock,
    });

    await expect(client.loadProjectMetricsSessionDetailById("session-1")).resolves.toMatchObject({
      session_id: "session-1",
      start_user_request: "Fix the parser regression",
    });

    expect(fetchMock).toHaveBeenCalledWith(
      `${baseUrl}/api/viewer/load_project_metrics_session_detail_by_id`,
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
        },
        body: JSON.stringify({
          session_id: "session-1",
        }),
      },
    );
  });

  it("fails fast when live tail is requested", async () => {
    const client = new RemoteViewerBackendClient({
      baseUrl,
      fetch: vi.fn<typeof fetch>(),
    });

    await expect(
      client.tailSession("sessions/demo.jsonl", {
        session_ref: "sessions/demo.jsonl",
        offset: 0,
        file_identity: {
          device: null,
          inode: null,
          size: 0,
          modified_unix_ms: null,
        },
        pending_fragment: [],
        call_names: {},
        resolved_parent_thread_id: null,
        next_seq: 1,
        recent_dedup_keys: [],
      }),
    ).rejects.toThrow("Live tail пока не поддерживается");
  });
});
