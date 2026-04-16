export type EventCardTone =
  | "message"
  | "shell"
  | "plan"
  | "patch"
  | "tokens"
  | "collab"
  | "meta"
  | "task"
  | "error"
  | "generic";

export function eventToneFromEventType(eventType: string): EventCardTone {
  if (eventType.startsWith("message.")) {
    return "message";
  }
  if (eventType.startsWith("shell.")) {
    return "shell";
  }
  if (eventType === "todo.update") {
    return "plan";
  }
  if (eventType === "patch.apply" || eventType === "patch.apply.duplicate") {
    return "patch";
  }
  if (eventType === "info.tokens") {
    return "tokens";
  }
  if (eventType.startsWith("collab.") || eventType === "user.input.request") {
    return "collab";
  }
  if (
    eventType.startsWith("task.")
    || eventType === "agent.failed"
    || eventType === "agent.aborted"
  ) {
    return "task";
  }
  if (
    eventType === "agent.reasoning"
    || eventType === "runtime.context"
    || eventType === "agent.meta"
    || eventType === "agent.session"
    || eventType === "agent.session.foreign"
    || eventType === "context.compacted"
    || eventType === "context.compacted.duplicate"
  ) {
    return "meta";
  }
  if (eventType === "error" || eventType === "stderr.line") {
    return "error";
  }

  return "generic";
}
