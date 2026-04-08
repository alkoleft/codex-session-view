import type { EventNode, ThreadNode, TimelineItem } from "@/backend";

export function sortTimelineItemsForRender(items: TimelineItem[]): TimelineItem[] {
  return items
    .map((item, index) => ({
      item,
      index,
      renderSeq: timelineItemRenderSeq(item),
    }))
    .sort((left, right) => compareRenderSeq(left.renderSeq, right.renderSeq, left.index, right.index))
    .map(({ item }) => item);
}

export function sortEntriesByRenderSeq<T extends { renderSeq: number; index: number }>(
  entries: T[],
): T[] {
  return [...entries].sort((left, right) =>
    compareRenderSeq(left.renderSeq, right.renderSeq, left.index, right.index));
}

export function timelineItemRenderSeq(item: TimelineItem): number {
  if ("Event" in item) {
    return eventNodeRenderSeq(item.Event);
  }

  return threadRenderSeq(item.Thread);
}

export function timelineItemsRenderSeq(items: TimelineItem[]): number {
  const renderSeq = items.reduce<number>((max, item) => {
    const itemRenderSeq = timelineItemRenderSeq(item);
    return itemRenderSeq > max ? itemRenderSeq : max;
  }, Number.NEGATIVE_INFINITY);

  return Number.isFinite(renderSeq) ? renderSeq : Number.MAX_SAFE_INTEGER;
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

function compareRenderSeq(leftRenderSeq: number, rightRenderSeq: number, leftIndex: number, rightIndex: number) {
  if (leftRenderSeq !== rightRenderSeq) {
    return leftRenderSeq - rightRenderSeq;
  }

  return leftIndex - rightIndex;
}
