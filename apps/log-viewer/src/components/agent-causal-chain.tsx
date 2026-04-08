import { ArrowRight } from "lucide-react";

import type { AgentThreadViewModel } from "@/components/agent-thread-view-model";
import { cn } from "@/lib/utils";

function formatTime(value: string) {
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

export function AgentCausalChain({
  agent,
  onFocusEvent,
}: {
  agent: AgentThreadViewModel | null;
  onFocusEvent: (eventId: string) => void;
}) {
  if (!agent) {
    return (
      <p className="text-xs text-muted-foreground">
        Выберите агента справа или кликните по субагентскому событию в timeline.
      </p>
    );
  }

  if (agent.steps.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        Для этого агента нет детерминированной причинной цепочки. Viewer оставляет timeline
        источником истины.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      {agent.steps.map((step, index) => (
        <button
          className={cn(
            "group flex w-full items-start gap-3 rounded-xl border border-border/60 bg-background/40 px-3 py-3 text-left transition-colors hover:bg-muted/40",
          )}
          key={step.eventId}
          onClick={() => {
            onFocusEvent(step.eventId);
          }}
          type="button"
        >
          <div className="flex min-w-[3.5rem] flex-col gap-1">
            <span className="text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
              {step.label}
            </span>
            <span className="font-mono text-[11px] text-muted-foreground">
              #{step.seq}
            </span>
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <span>{formatTime(step.ts)}</span>
              {index < agent.steps.length - 1 ? (
                <ArrowRight className="size-3 text-[color:var(--accent-strong)]" />
              ) : null}
            </div>
            {step.summary ? (
              <p className="mt-1 whitespace-pre-wrap break-words text-xs leading-5 text-foreground/85">
                {step.summary}
              </p>
            ) : (
              <p className="mt-1 text-xs text-muted-foreground">Перейти к событию в timeline.</p>
            )}
          </div>
        </button>
      ))}
    </div>
  );
}
