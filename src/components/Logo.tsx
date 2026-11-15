/** Sift's mark: a blocky S built like a tiling layout. The short last bar is
 *  the space you got back. Colours follow the rice palette.
 *
 *  LOGO_PARTS is the single source of truth: the app icon, the top bar and
 *  every screen graphic (scanning, empty states) draw from it. */

export type Tone = "accent" | "accent2" | "accent3";

export interface LogoPart {
  x: number;
  y: number;
  w: number;
  h: number;
  tone: Tone;
  /** Direction the part grows in when animated. */
  grow: "right" | "down" | "left" | "pop";
}

/** On a 24×24 grid, in drawing order (the way you'd trace the S). */
export const LOGO_PARTS: LogoPart[] = [
  { x: 3, y: 3, w: 18, h: 3, tone: "accent", grow: "left" },
  { x: 3, y: 3, w: 3, h: 9, tone: "accent", grow: "down" },
  { x: 3, y: 10.5, w: 18, h: 3, tone: "accent2", grow: "right" },
  { x: 18, y: 10.5, w: 3, h: 10.5, tone: "accent2", grow: "down" },
  { x: 3, y: 18, w: 12, h: 3, tone: "accent3", grow: "left" },
  { x: 9, y: 7, w: 2, h: 2, tone: "accent3", grow: "pop" },
  { x: 13, y: 7, w: 2, h: 2, tone: "accent3", grow: "pop" },
];

export function Logo({ size = 22, mono = false }: { size?: number; mono?: boolean }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden className="logo">
      {LOGO_PARTS.map((p, i) => (
        <rect key={i} x={p.x} y={p.y} width={p.w} height={p.h} fill={mono ? "currentColor" : `var(--${p.tone})`} />
      ))}
    </svg>
  );
}
