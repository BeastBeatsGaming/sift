import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { api, SAFETY_LABEL, type Safety } from "../api";
import { bytes } from "../format";
import { EmptyGraphic } from "./graphics";

export function SafetyBadge({ safety }: { safety: Safety }) {
  return <span className={`badge badge-${safety}`}>{SAFETY_LABEL[safety]}</span>;
}

export function Bar({ value, tone = "accent" }: { value: number; tone?: string }) {
  return (
    <div className="bar">
      <i style={{ width: `${Math.max(0, Math.min(100, value))}%`, background: `var(--${tone})` }} />
    </div>
  );
}

export function RevealButton({ path }: { path: string }) {
  const [err, setErr] = useState<string | null>(null);
  return (
    <button
      className="icon-btn"
      title={err ?? "Show in Explorer"}
      aria-label="Show in Explorer"
      onClick={(e) => {
        e.stopPropagation();
        e.preventDefault();
        api.reveal(path).catch((x) => setErr(String(x)));
      }}
    >
      {err ? (
        "!"
      ) : (
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="square" aria-hidden>
          <path d="M14 4h6v6M20 4l-9 9M18 14v6H4V6h6" />
        </svg>
      )}
    </button>
  );
}

export function Stat({ label, value, sub, tone }: { label: string; value: ReactNode; sub?: string; tone?: string }) {
  return (
    <div className="stat">
      <span className="stat-label">{label}</span>
      <b className="stat-value" style={tone ? { color: `var(--${tone})` } : undefined}>
        {value}
      </b>
      {sub && <span className="stat-sub">{sub}</span>}
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return (
    <div className="empty">
      <EmptyGraphic />
      <span>{children}</span>
    </div>
  );
}

/** Stagger index for `.stagger` children. */
export const si = (i: number) => ({ "--i": i }) as CSSProperties;

/** Animates a number from its previous value (odometer-style count-up). */
export function useCountUp(target: number, ms = 650): number {
  const [v, setV] = useState(target);
  const from = useRef(0);
  useEffect(() => {
    const start = performance.now();
    const a = from.current;
    let raf = 0;
    const tick = (t: number) => {
      const k = Math.min(1, (t - start) / ms);
      const e = 1 - Math.pow(1 - k, 3);
      setV(a + (target - a) * e);
      if (k < 1) raf = requestAnimationFrame(tick);
      else from.current = target;
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      from.current = target;
    };
  }, [target, ms]);
  return v;
}

/** Byte count that counts up, with the unit set smaller. */
export function Bytes({ n, unitClass = "unit" }: { n: number; unitClass?: string }) {
  const v = useCountUp(n);
  const s = bytes(Math.max(0, Math.round(v)));
  const i = s.lastIndexOf(" ");
  return (
    <>
      {s.slice(0, i)}
      <span className={unitClass}>{s.slice(i + 1)}</span>
    </>
  );
}

/** Segmented HUD meter. `parts` are [value, css colour]; the rest is empty. */
export function Meter({ parts, total, segments = 60, delay = 10 }: { parts: [number, string][]; total: number; segments?: number; delay?: number }) {
  const cells: string[] = [];
  let acc = 0;
  for (const [v, c] of parts) {
    const end = Math.round(((acc + v) / Math.max(1, total)) * segments);
    while (cells.length < Math.min(segments, end)) cells.push(c);
    acc += v;
  }
  while (cells.length < segments) cells.push("var(--line-soft)");
  return (
    <div className="meter" aria-hidden>
      {cells.map((c, i) => (
        <i key={i} style={{ background: c, animationDelay: `${i * delay}ms` }} />
      ))}
    </div>
  );
}
