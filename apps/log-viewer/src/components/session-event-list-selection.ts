import type { EventNode } from "@/backend";

export function preferredTerminalChildIndexFromSnapshot(node: EventNode) {
  const terminalSeq = node.event.operation_terminal_seq;
  if (terminalSeq == null) {
    return null;
  }

  const rootEventId = node.event.operation_root_event_id ?? node.event.event_id;
  for (let index = 0; index < node.children.length; index += 1) {
    const item = node.children[index];
    if (!("Event" in item)) {
      continue;
    }

    const event = item.Event.event;
    if (
      event.seq === terminalSeq
      && event.operation_root_event_id === rootEventId
      && event.operation_is_preferred_terminal
    ) {
      return index;
    }
  }

  return null;
}
