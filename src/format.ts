export function bytes(n: number, digits = 1): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return i === 0 ? `${v} B` : `${v.toFixed(v >= 100 ? 0 : digits)} ${units[i]}`;
}

export const count = (n: number) => n.toLocaleString("en-IN");

export const pct = (part: number, whole: number) => (whole > 0 ? (part / whole) * 100 : 0);

export function ago(unixSecs: number): string {
  if (!unixSecs) return "—";
  const days = (Date.now() / 1000 - unixSecs) / 86400;
  if (days < 1) return "today";
  if (days < 30) return `${Math.floor(days)} d ago`;
  if (days < 365) return `${Math.floor(days / 30)} mo ago`;
  return `${(days / 365).toFixed(1)} y ago`;
}

export function basename(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}

export function dirname(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  const d = i > 0 ? p.slice(0, i) : p;
  return /^[A-Za-z]:$/.test(d) ? d + "\\" : d;
}
