import type { Palette } from "./api";

/** Rice palette roles that map 1:1 onto Sift's CSS variables. Safe / review /
 *  danger colours (--ok, --warn, --alert) stay fixed so their meaning never
 *  depends on the wallpaper. */
const ROLES = ["bg", "surface", "overlay", "border", "fg", "muted", "accent", "accent2", "accent3"] as const;

/** Muted text must stay readable on panels whatever the wallpaper picks. */
const MIN_MUTED_CONTRAST = 5.5;

type RGB = [number, number, number];

function parse(hex: string | undefined): RGB | null {
  const m = hex?.match(/^#([0-9a-f]{6})([0-9a-f]{2})?$/i);
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

const toHex = (c: RGB) => "#" + c.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");
const mix = (a: RGB, b: RGB, t: number): RGB => [0, 1, 2].map((i) => a[i] + (b[i] - a[i]) * t) as RGB;

function luminance([r, g, b]: RGB): number {
  const f = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

function contrast(a: RGB, b: RGB): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

/** Nudge `muted` toward `fg` until it reads well on both bg and surface. */
export function readableMuted(p: Palette): string | null {
  const muted = parse(p.muted);
  const fg = parse(p.fg);
  const grounds = [parse(p.bg), parse(p.surface)].filter(Boolean) as RGB[];
  if (!muted || !fg || grounds.length === 0) return null;
  const worst = (c: RGB) => Math.min(...grounds.map((g) => contrast(c, g)));
  for (let t = 0; t <= 1.0001; t += 0.05) {
    const c = mix(muted, fg, t);
    if (worst(c) >= MIN_MUTED_CONTRAST) return toHex(c);
  }
  return toHex(fg);
}

export function applyPalette(p: Palette | null) {
  const root = document.documentElement.style;
  for (const r of ROLES) {
    const v = p?.[r];
    if (v) root.setProperty(`--${r}`, v);
    else root.removeProperty(`--${r}`);
  }
  const muted = p && readableMuted(p);
  if (muted) root.setProperty("--muted", muted);
}
