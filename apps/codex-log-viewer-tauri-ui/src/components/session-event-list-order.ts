import type { EventNode, ThreadNode, TimelineItem } from "@/backend";

export function sortTimelineItemsForRender(items: TimelineItem[]): TimelineItem[] {
  return items
    .map((item, index) => ({
      item,
      index,
      renderSeq: timelineItemRenderSeq(item),
    }))
    .sort((left, right) => {
      if (left.renderSeq !== right.renderSeq) {
        return left.renderSeq - right.renderSeq;
      }
      return left.index - right.index;
    })
    .map(({ item }) => item);
}

function timelineItemRenderSeq(item: TimelineItem): number {
  if ("Event" in item) {
    return eventNodeRenderSeq(item.Event);
  }

  return threadRenderSeq(item.Thread);
}

function eventNodeRenderSeq(node: EventNode): number {
  return node.event.operation_terminal_seq ?? node.event.seq;
}

function threadRenderSeq(thread: ThreadNode): number {
  const firstSeq = thread.items.reduce<number>((min, item) => {
    const itemRenderSeq = timelineItemRenderSeq(item);
    return itemRenderSeq < min ? itemRenderSeq : min;
  }, Number.POSITIVE_INFINITY);

  return Number.isFinite(firstSeq) ? firstSeq : Number.MAX_SAFE_INTEGER;
}
