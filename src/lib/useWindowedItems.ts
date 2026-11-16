import { useCallback, useLayoutEffect, useRef, useState, type CSSProperties } from "react";

/** Below this many items everything is simply drawn: there is nothing to save. */
const WINDOW_FROM = 60;
/** Rows kept ready above and below what is on screen, so scrolling never shows a gap. */
const OVERSCAN_ROWS = 3;
/** Drawn before the first measurement: enough to fill a large window. */
const FIRST_DRAW = 60;

interface Slice {
  /** Index of the first item to draw. */
  start: number;
  /** Index after the last item to draw. */
  end: number;
  /** Space standing in for the rows above and below that are not drawn. */
  before: number;
  after: number;
}

/**
 * Draws only the part of a long grid or list that is on screen.
 *
 * A library of several hundred games is tens of thousands of elements if every card exists
 * at once: changing section, opening a sheet or scrolling then costs in proportion to the
 * whole library. Here the container keeps its full height (padding stands in for the rows
 * that are not drawn), and only the rows in view, plus a few around them, exist.
 *
 * It measures what is there instead of being told: the columns come from the grid's own
 * tracks (one for a plain list), a row is as tall as the first item plus the grid's gap. So
 * it follows the theme, the window and the text size without knowing about any of them.
 * Items must all be the same height. The scrolling ancestor is the nearest `.scroll-area`.
 * `layoutKey` says which container is in use, when the caller swaps one for another (grid
 * and list).
 */
export function useWindowedItems<T extends HTMLElement>(count: number, layoutKey: unknown = null) {
  const containerRef = useRef<T | null>(null);
  const [slice, setSlice] = useState<Slice>(() => ({
    start: 0,
    end: count > WINDOW_FROM ? FIRST_DRAW : count,
    before: 0,
    after: 0,
  }));
  /** `true` once the first rows have been drawn: later arrivals come from scrolling. */
  const settled = useRef(false);
  // What is drawn now. Measuring happens after every draw: it must ask for another one only
  // when the answer is different, or the two would chase each other.
  const drawn = useRef(slice);
  drawn.current = slice;
  const show = useCallback((next: Slice) => {
    const current = drawn.current;
    if (
      current.start === next.start &&
      current.end === next.end &&
      Math.abs(current.before - next.before) < 0.5 &&
      Math.abs(current.after - next.after) < 0.5
    ) {
      return;
    }
    drawn.current = next;
    setSlice(next);
  }, []);

  const measure = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;
    if (count <= WINDOW_FROM) {
      show({ start: 0, end: count, before: 0, after: 0 });
      return;
    }
    const first = container.firstElementChild;
    const scroller = container.closest<HTMLElement>(".scroll-area");
    if (!scroller) return;
    if (!first) {
      // What was drawn belongs to a longer list: start again from its top
      show({ start: 0, end: Math.min(count, FIRST_DRAW), before: 0, after: 0 });
      return;
    }

    const style = getComputedStyle(container);
    const isGrid = style.display.includes("grid");
    const columns = isGrid
      ? Math.max(1, style.gridTemplateColumns.split(" ").filter(Boolean).length)
      : 1;
    const gap = isGrid ? parseFloat(style.rowGap) || 0 : 0;
    // The laid-out height: unlike the box on screen it ignores an entrance animation
    const height = parseFloat(getComputedStyle(first).height);
    if (!(height > 0)) return;

    const pitch = height + gap;
    const rows = Math.ceil(count / columns);
    // Where the container's first row is, measured from the top of what the scroller shows
    const top = container.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
    const firstRow = Math.min(
      Math.max(0, Math.floor(-top / pitch) - OVERSCAN_ROWS),
      Math.max(0, rows - 1)
    );
    const lastRow = Math.min(
      rows,
      Math.max(firstRow + 1, Math.ceil((scroller.clientHeight - top) / pitch) + OVERSCAN_ROWS)
    );
    show({
      start: firstRow * columns,
      end: Math.min(count, lastRow * columns),
      before: firstRow * pitch,
      after: (rows - lastRow) * pitch,
    });
  }, [count, show]);

  useLayoutEffect(() => {
    const container = containerRef.current;
    const scroller = container?.closest<HTMLElement>(".scroll-area");
    if (!container || !scroller) return;

    let frame = 0;
    const onScroll = () => {
      settled.current = true;
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        measure();
      });
    };
    scroller.addEventListener("scroll", onScroll, { passive: true });
    // The window, the side panel, the theme or the text size changed what fits
    const observer = new ResizeObserver(() => measure());
    observer.observe(scroller);
    observer.observe(container);
    const settle = window.setTimeout(() => {
      settled.current = true;
    }, 1200);
    return () => {
      scroller.removeEventListener("scroll", onScroll);
      observer.disconnect();
      window.clearTimeout(settle);
      if (frame) cancelAnimationFrame(frame);
    };
  }, [measure, layoutKey]);

  // Whatever was just drawn (other games, another size) is measured before it is shown
  useLayoutEffect(() => {
    measure();
  });

  const end = Math.min(slice.end, count);
  const start = Math.min(slice.start, end);
  const style: CSSProperties = { paddingTop: slice.before, paddingBottom: slice.after };

  return {
    containerRef,
    start,
    end,
    /** Padding that keeps the container as tall as if every item were drawn. */
    style,
    /** Whether an item drawn now arrives by scrolling rather than with the view. */
    arrivesLater: () => settled.current,
  };
}
