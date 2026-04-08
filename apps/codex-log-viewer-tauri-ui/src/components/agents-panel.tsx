import { AlertTriangle, Bot, GitBranch, Target } from "lucide-react";

import type {
  AgentGraphViewModel,
  AgentThreadStatus,
  AgentThreadViewModel,
} from "@/components/agent-thread-view-model";
import { AgentCausalChain } from "@/components/agent-causal-chain";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

const SURFACE_CARD_CLASS = "border-border bg-background shadow-none";

export function AgentsPanel({
  activeAgent,
  graph,
  onFocusEvent,
  onSelectAgent,
}: {
  activeAgent: AgentThreadViewModel | null;
  graph: AgentGraphViewModel | null;
  onFocusEvent: (eventId: string) => void;
  onSelectAgent: (threadId: string, focusEventId?: string | null) => void;
}) {
  if (!graph || graph.agents.length === 0) {
    return (
      <Card className={SURFACE_CARD_CLASS} size="sm">
        <CardContent className="pt-6">
          <div className="flex flex-col gap-2 text-sm text-muted-foreground">
            <div className="flex items-center gap-2 text-foreground">
              <Bot className="size-4 text-[color:var(--accent-strong)]" />
              <span className="font-medium">Фоновые агенты не обнаружены</span>
            </div>
            <p>
              Для выбранной сессии viewer не нашёл subagent-thread. Timeline остаётся единственным
              источником деталей.
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      {graph.unboundEventIds.length > 0 ? (
        <div className="rounded-xl border border-amber-500/35 bg-amber-500/10 px-3 py-2 text-xs text-amber-100">
          <div className="flex items-center gap-2 font-medium text-amber-200">
            <AlertTriangle className="size-3.5" />
            <span>Есть `unknown/unbound` события</span>
          </div>
          <p className="mt-1 text-amber-100/90">
            Viewer не смог надёжно привязать {graph.unboundEventIds.length} collab-событий к
            конкретному агенту и оставил их только в timeline.
          </p>
        </div>
      ) : null}

      <Card className={SURFACE_CARD_CLASS} size="sm">
        <CardHeader className="gap-2">
          <CardTitle className="text-sm">Обзор агентов</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          {graph.agents.map((agent) => {
            const isActive = activeAgent?.threadId === agent.threadId;
            const paddingLeft = `${Math.max(0, agent.depth * 0.9)}rem`;

            return (
              <button
                className={cn(
                  "w-full rounded-2xl border px-3 py-3 text-left transition-colors",
                  isActive
                    ? "border-[color:var(--accent-strong)] bg-muted/30"
                    : "border-border/60 bg-background/40 hover:bg-muted/30",
                )}
                key={agent.threadId}
                onClick={() => {
                  onSelectAgent(agent.threadId, agent.spawnEventId ?? agent.firstEventId);
                }}
                style={{ paddingLeft: `calc(${paddingLeft} + 0.75rem)` }}
                type="button"
              >
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="ui-selectable break-words text-sm font-medium text-foreground">
                        {agent.displayLabel}
                      </span>
                      <AgentStatusBadge status={agent.status} />
                    </div>
                    {agent.promptPreview ? (
                      <p className="mt-1 text-xs leading-5 text-muted-foreground">
                        {agent.promptPreview}
                      </p>
                    ) : null}
                  </div>
                  <span className="font-mono text-[11px] text-muted-foreground">
                    {agent.threadId}
                  </span>
                </div>
                {agent.lastMeaningfulText ? (
                  <p className="mt-2 whitespace-pre-wrap break-words text-xs leading-5 text-foreground/85">
                    {agent.lastMeaningfulText}
                  </p>
                ) : (
                  <p className="mt-2 text-xs text-muted-foreground">
                    Детальный прогресс пока виден только в timeline.
                  </p>
                )}
                {agent.unboundEventIds.length > 0 ? (
                  <p className="mt-2 text-[11px] text-amber-300">
                    duplicate/unbound hints: {agent.unboundEventIds.length}
                  </p>
                ) : null}
              </button>
            );
          })}
        </CardContent>
      </Card>

      <Card className={SURFACE_CARD_CLASS} size="sm">
        <CardHeader className="gap-2">
          <CardTitle className="text-sm">Причинная цепочка</CardTitle>
        </CardHeader>
        <CardContent>
          <AgentCausalChain agent={activeAgent} onFocusEvent={onFocusEvent} />
        </CardContent>
      </Card>

      <Card className={SURFACE_CARD_CLASS} size="sm">
        <CardHeader className="gap-2">
          <CardTitle className="text-sm">Зачем создан агент</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {activeAgent ? (
            <>
              {activeAgent.spawnPrompt ? (
                <div className="rounded-xl border border-border/60 bg-background/40 px-3 py-3">
                  <p className="ui-selectable whitespace-pre-wrap break-words text-sm leading-6 text-foreground">
                    {activeAgent.spawnPrompt}
                  </p>
                </div>
              ) : (
                <p className="text-xs text-muted-foreground">
                  Для выбранного агента нет исходного `spawn_agent.prompt`.
                </p>
              )}
              <div className="grid gap-3 sm:grid-cols-2">
                <AgentMetaItem icon={Target} label="Thread" value={activeAgent.threadId} />
                <AgentMetaItem label="Status" value={activeAgent.status} />
                <AgentMetaItem label="Role" value={activeAgent.role ?? "n/a"} />
                <AgentMetaItem label="Type" value={activeAgent.requestedAgentType ?? "n/a"} />
                <AgentMetaItem label="Model" value={activeAgent.model ?? "n/a"} />
                <AgentMetaItem label="Effort" value={activeAgent.reasoningEffort ?? "n/a"} />
                <AgentMetaItem
                  icon={GitBranch}
                  label="Parent thread"
                  value={activeAgent.parentThreadId ?? "n/a"}
                />
              </div>
            </>
          ) : (
            <p className="text-xs text-muted-foreground">
              Выберите агента, чтобы увидеть полный prompt и метаданные запуска.
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function AgentMetaItem({
  icon: Icon,
  label,
  value,
}: {
  icon?: typeof Target;
  label: string;
  value: string;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <div className="flex items-center gap-1 text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
        {Icon ? <Icon className="size-3" /> : null}
        <span>{label}</span>
      </div>
      <div className="ui-selectable break-words text-sm text-foreground">{value}</div>
    </div>
  );
}

function AgentStatusBadge({ status }: { status: AgentThreadStatus }) {
  const className = agentStatusBadgeClassName(status);
  return <Badge className={className}>{status}</Badge>;
}

function agentStatusBadgeClassName(status: AgentThreadStatus) {
  switch (status) {
    case "failed":
      return "border-rose-500/35 bg-rose-500/12 text-rose-200";
    case "aborted":
      return "border-amber-500/35 bg-amber-500/12 text-amber-200";
    case "closed":
      return "border-sky-500/35 bg-sky-500/12 text-sky-200";
    case "completed":
      return "border-emerald-500/35 bg-emerald-500/12 text-emerald-200";
    case "waiting":
      return "border-indigo-500/35 bg-indigo-500/12 text-indigo-200";
    case "running":
      return "border-cyan-500/35 bg-cyan-500/12 text-cyan-200";
    default:
      return "border-border/60 bg-background/60 text-muted-foreground";
  }
}
