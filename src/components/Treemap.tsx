import { useEffect, useMemo, useRef, useState } from "react";
import { bytes } from "../format";

export interface TreemapItem {
  key: string;
  label: string;
  size: number;
  tone: "dir" | "regen" | "files" | "denied";
  onClick?: () => void;
}

type Rect = { x: number; y: number; w: number; h: number };

/** Ratio of the worst (most elongated) rectangle if `row` is laid along `side`. */
function worst(row: number[], side: number): number {
  const s = row.reduce((a, b) => a + b, 0);
  const max = Math.max(...row);
  const min = Math.min(...row);
  return Math.max((side * side * max) / (s * s), (s * s) / (side * side * min));
}

/** Squarified treemap (Bruls et al.). `sizes` must be sorted largest first. */
export function squarify(sizes: number[], rect: Rect): Rect[] {
  const total = sizes.reduce((a, b) => a + b, 0);
  if (total <= 0 || rect.w <= 0 || rect.h <= 0) return sizes.map(() => ({ ...rect, w: 0, h: 0 }));
  const scale = (rect.w * rect.h) / total;
  const areas = sizes.map((s) => s * scale);
  const out: Rect[] = [];
  let r = { ...rect };
  let i = 0;
  while (i < areas.length) {
    const side = Math.min(r.w, r.h);
    const row = [areas[i]];
    let j = i + 1;
    while (j < areas.length && worst([...row, areas[j]], side) <= worst(row, side)) {
      row.push(areas[j]);
      j++;
    }
    const sum = row.reduce((a, b) => a + b, 0);
    if (r.w >= r.h) {
      const cw = sum / r.h;
      let y = r.y;
      for (const a of row) {
        out.push({ x: r.x, y, w: cw, h: a / cw });
        y += a / cw;
      }
      r = { x: r.x + cw, y: r.y, w: r.w - cw, h: r.h };
    } else {
      const rh = sum / r.w;
      let x = r.x;
      for (const a of row) {
        out.push({ x, y: r.y, w: a / rh, h: rh });
        x += a / rh;
      }
      r = { x: r.x, y: r.y + rh, w: r.w, h: r.h - rh };
    }
    i = j;
  }
  return out;
}

export function Treemap({ items }: { items: TreemapItem[] }) {
  const ref = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setBox({ w: e.contentRect.width, h: e.contentRect.height }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const visible = useMemo(() => items.filter((i) => i.size > 0).sort((a, b) => b.size - a.size), [items]);
  const rects = useMemo(() => squarify(visible.map((i) => i.size), { x: 0, y: 0, ...box }), [visible, box]);
  const total = visible.reduce((a, b) => a + b.size, 0);

  return (
    <div className="treemap" ref={ref}>
      {rects.map((r, idx) => {
        const it = visible[idx];
        if (r.w < 2 || r.h < 2) return null;
        const big = r.w > 70 && r.h > 34;
        const share = total ? it.size / total : 0;
        return (
          <button
            key={it.key}
            className={`tm-cell tm-${it.tone}`}
            style={{
              left: r.x,
              top: r.y,
              width: r.w,
              height: r.h,
              // larger blocks are more saturated, so the eye lands on them first
              ["--strength" as string]: Math.min(1, 0.35 + share * 2.2),
            }}
            onClick={it.onClick}
            disabled={!it.onClick}
            title={`${it.label} — ${bytes(it.size)}`}
          >
            {big && (
              <span className="tm-label">
                <span className="tm-name">{it.label}</span>
                <span className="tm-size">{bytes(it.size)}</span>
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
