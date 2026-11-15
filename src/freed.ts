import type { HistoryView } from "./api";

export interface MonthFreed {
  key: string; // "2026-11"
  label: string; // "Nov"
  bytes: number;
  cleanups: number;
}

export interface FreedSummary {
  total: number;
  cleanups: number;
  thisMonth: number;
  /** Last `n` calendar months, oldest first, including empty ones. */
  months: MonthFreed[];
}

const keyOf = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;

/** Totals of space reclaimed, per local calendar month. */
export function summarizeFreed(h: Pick<HistoryView, "freed"> | null, n = 6): FreedSummary {
  const now = new Date();
  const months: MonthFreed[] = [];
  for (let i = n - 1; i >= 0; i--) {
    const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
    months.push({ key: keyOf(d), label: d.toLocaleString("en-IN", { month: "short" }), bytes: 0, cleanups: 0 });
  }
  let total = 0;
  for (const p of h?.freed ?? []) {
    total += p.freed;
    const m = months.find((m) => m.key === keyOf(new Date(p.created * 1000)));
    if (m) {
      m.bytes += p.freed;
      m.cleanups++;
    }
  }
  return { total, cleanups: h?.freed.length ?? 0, thisMonth: months[months.length - 1].bytes, months };
}
