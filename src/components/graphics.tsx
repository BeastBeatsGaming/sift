/* Screen graphics, drawn from the logo's own parts (LOGO_PARTS) so they
   always match the mark exactly. Colours come from the theme variables. */
import { LOGO_PARTS, type LogoPart } from "./Logo";

const origin = (p: LogoPart) =>
  // grows rightwards = anchored on its left edge, and so on
  p.grow === "right" ? "left center" : p.grow === "left" ? "right center" : p.grow === "down" ? "center top" : "center";

/** Scanning: the S traces itself bar by bar, holds, clears and repeats,
 *  while a scan line sweeps over it. */
export function ScanGraphic({ size = 144 }: { size?: number }) {
  const step = 0.14; // seconds between parts
  return (
    <svg width={size} height={size} viewBox="-1 -1 26 26" aria-hidden className="g-scan">
      <style>{`
        .g-scan .p{transform-box:fill-box;animation:g-build 2.8s cubic-bezier(.2,.7,.2,1) infinite both}
        .g-scan .p.x{animation-name:g-build-x}
        .g-scan .p.y{animation-name:g-build-y}
        .g-scan .p.pop{animation-name:g-build-pop}
        .g-scan .ln{animation:g-line 1.4s linear infinite}
        @keyframes g-build-x{0%{transform:scaleX(0)}22%,82%{transform:scaleX(1)}96%,100%{transform:scaleX(0)}}
        @keyframes g-build-y{0%{transform:scaleY(0)}22%,82%{transform:scaleY(1)}96%,100%{transform:scaleY(0)}}
        @keyframes g-build-pop{0%,8%{transform:scale(0)}22%,82%{transform:scale(1)}96%,100%{transform:scale(0)}}
        @keyframes g-line{from{transform:translateY(0)}to{transform:translateY(24px)}}
      `}</style>
      <rect x="-0.5" y="-0.5" width="25" height="25" fill="none" stroke="var(--line)" strokeWidth="0.25" />
      {/* faint ghost of the full mark underneath */}
      {LOGO_PARTS.map((p, i) => (
        <rect key={`g${i}`} x={p.x} y={p.y} width={p.w} height={p.h} fill={`var(--${p.tone})`} opacity="0.12" />
      ))}
      {LOGO_PARTS.map((p, i) => (
        <rect
          key={i}
          className={`p ${p.grow === "pop" ? "pop" : p.grow === "down" ? "y" : "x"}`}
          x={p.x}
          y={p.y}
          width={p.w}
          height={p.h}
          fill={`var(--${p.tone})`}
          style={{ transformOrigin: origin(p), animationDelay: `${i * step}s` }}
        />
      ))}
      <rect className="ln" x="0" y="0" width="24" height="0.5" fill="var(--fg)" opacity="0.35" />
    </svg>
  );
}

/** Nothing to clean: the mark, flat and quiet. */
export function EmptyGraphic({ size = 88 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden>
      {/* one group so overlapping parts merge into a single seamless shape */}
      <g fill="var(--muted)" opacity="0.4">
        {LOGO_PARTS.map((p, i) => (
          <rect key={i} x={p.x} y={p.y} width={p.w} height={p.h} />
        ))}
      </g>
    </svg>
  );
}

/** Cleaned: a check mark that steps in, in the same blocky style. */
export function DoneGraphic({ size = 64 }: { size?: number }) {
  const check = [
    [1, 4], [2, 5], [3, 6], [4, 5], [5, 4], [6, 3], [7, 2],
  ];
  return (
    <svg width={size} height={size} viewBox="0 0 72 72" aria-hidden className="g-done">
      <style>{`.g-done .c{animation:g-on2 .2s steps(2) both}@keyframes g-on2{from{opacity:0}}`}</style>
      <rect x="0.5" y="0.5" width="71" height="71" fill="none" stroke="color-mix(in srgb, var(--ok) 45%, transparent)" />
      {check.map(([x, y], i) => (
        <rect key={i} className="c" x={x * 8 + 1} y={y * 8 + 1} width={6} height={6} fill="var(--ok)" style={{ animationDelay: `${i * 60}ms` }} />
      ))}
    </svg>
  );
}
