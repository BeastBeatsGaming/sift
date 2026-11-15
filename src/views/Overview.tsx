import { useEffect, useState } from "react";
import { api, type BackgroundStatus, type Suggestion, type Summary } from "../api";
import { Bar, Bytes, Meter, SafetyBadge, si } from "../components/ui";
import { bytes, count, pct } from "../format";
import type { FreedSummary } from "../freed";

function when(t: number) {
  return new Date(t * 1000).toLocaleString("en-IN", { weekday: "short", hour: "2-digit", minute: "2-digit" }).toUpperCase();
}

export function Overview({
  summary,
  suggestions,
  freed,
  inTrash,
  background,
  openSettings,
  goSuggestions,
  admin,
  onAdmin,
}: {
  summary: Summary;
  suggestions: Suggestion[];
  freed: FreedSummary;
  inTrash: number;
  background: boolean;
  openSettings: () => void;
  goSuggestions: () => void;
  admin: boolean;
  onAdmin: () => void;
}) {
  const actionable = suggestions.filter((s) => s.safety !== "Leave");
  const info = suggestions.filter((s) => s.safety === "Leave");
  const safe = actionable.filter((s) => s.safety === "Safe").reduce((a, s) => a + s.bytes, 0);
  const review = actionable.filter((s) => s.safety === "Review" || s.safety === "TradeOff").reduce((a, s) => a + s.bytes, 0);
  const st = summary.stats;
  const maxCat = summary.categories[0]?.[1] ?? 1;
  const hasDisk = summary.disk_total != null && summary.disk_free != null;
  const total = summary.disk_total ?? summary.total;
  const used = hasDisk ? total - summary.disk_free! : summary.total;
  const maxMonth = Math.max(1, ...freed.months.map((m) => m.bytes));

  const [bg, setBg] = useState<BackgroundStatus | null>(null);
  useEffect(() => {
    if (background) api.backgroundStatus().then(setBg).catch(() => {});
  }, [background]);

  return (
    <div className="page">
      <div className="page-head" style={{ marginBottom: 0 }}>
        <div>
          <span className="label">
            {summary.root_path} · {count(st.files)} files · {(st.elapsed_ms / 1000).toFixed(1)}s
          </span>
          <h1 className="page-title">{safe > 0 ? `You can free up ${bytes(safe)}` : "Nothing urgent to clear"}</h1>
        </div>
      </div>

      <section className="card hero sweep">
        <div className="hero-top">
          <div className="readout">
            <span className="label">Safe to clear</span>
            <b className="xl" style={{ color: "var(--ok)" }}>
              <Bytes n={safe} />
            </b>
          </div>
          <div className="hero-side">
            <div className="readout">
              <span className="label">With review</span>
              <b className="lg" style={{ color: "var(--warn)" }}>
                <Bytes n={review} />
              </b>
            </div>
            {hasDisk && (
              <div className="readout">
                <span className="label">Drive free</span>
                <b className="lg">
                  <Bytes n={summary.disk_free!} />
                  <span className="unit"> / {bytes(total)}</span>
                </b>
              </div>
            )}
            <button className="btn btn-ok btn-lg" onClick={goSuggestions} disabled={actionable.length === 0}>
              Review &amp; clean ▸
            </button>
          </div>
        </div>
        <Meter
          total={total}
          parts={[
            [Math.max(0, used - safe), "color-mix(in srgb, var(--accent) 78%, transparent)"],
            [safe, "var(--ok)"],
          ]}
        />
        <div className="meter-legend">
          <span>0</span>
          <span>■ USED {bytes(Math.max(0, used - safe))}</span>
          <span style={{ color: "var(--ok)" }}>■ SAFE {bytes(safe)}</span>
          {hasDisk && <span>□ FREE {bytes(summary.disk_free!)}</span>}
          <span>{bytes(total)}</span>
        </div>
      </section>

      {(st.errors > 0 || st.cloud_skipped > 0) && (
        <div className="notes">
          {st.errors > 0 && (
            <div className="notice notice-row">
              {count(st.errors)} items couldn't be read{admin ? " (in use or protected by Windows)." : ". Run Sift as administrator to include them."}
              {!admin && (
                <button className="link" onClick={onAdmin}>
                  Restart as admin
                </button>
              )}
            </div>
          )}
          {st.cloud_skipped > 0 && <div className="notice">{count(st.cloud_skipped)} OneDrive online-only files skipped — they don't use local space.</div>}
        </div>
      )}

      <div className="ov-grid">
        <section className="card span-7 targets">
          <div className="panel-head">
            <span className="label">Top targets</span>
            <button className="link" onClick={goSuggestions}>
              All {actionable.length} →
            </button>
          </div>
          <div className="list stagger">
            {actionable.slice(0, 7).map((s, i) => (
              <div className="row clickable" key={s.id} style={si(i)} onClick={goSuggestions}>
                <span className="idx">{String(i + 1).padStart(2, "0")}</span>
                <span className="row-main">{s.title}</span>
                <SafetyBadge safety={s.safety} />
                <span className="num">{bytes(s.bytes)}</span>
              </div>
            ))}
            {actionable.length === 0 && <div className="row muted">Nothing worth clearing — tidy drive.</div>}
          </div>
        </section>

        <div className="span-5 col-stack">
          <section className="card freed">
            <div className="section-head">
              <span className="label">Freed / month</span>
              <span className="mono small" style={{ color: "var(--accent2)" }}>
                TOTAL {bytes(freed.total)}
              </span>
            </div>
            {freed.months.map((m, i) => (
              <div key={m.key} className={`month-row ${i === freed.months.length - 1 ? "now" : ""}`} title={`${count(m.cleanups)} cleanup${m.cleanups === 1 ? "" : "s"}`}>
                <span className="dim">{m.label.toUpperCase()}</span>
                <span className="bar">
                  <i style={{ width: `${(m.bytes / maxMonth) * 100}%`, animationDelay: `${i * 50}ms` }} />
                </span>
                <span className="num">{bytes(m.bytes)}</span>
              </div>
            ))}
            <span className="small muted">
              {count(freed.cleanups)} cleanup{freed.cleanups === 1 ? "" : "s"}
              {inTrash > 0 ? ` · ${bytes(inTrash)} still in .sift-trash` : ""}
            </span>
          </section>

          <section className="card watch">
            <div className="readout" style={{ gap: 4 }}>
              <span className="label">Watch</span>
              {background ? (
                <span>{bg?.next_run ? `Next scan ${when(bg.next_run)}` : "Background scan on"}</span>
              ) : (
                <span className="muted">Background scan is off</span>
              )}
            </div>
            {background ? (
              <span className="armed">● ARMED</span>
            ) : (
              <button className="btn btn-ghost btn-sm" onClick={openSettings}>
                Set up
              </button>
            )}
          </section>

          {info.length > 0 && (
            <section className="card freed">
              <span className="label" style={{ color: "var(--accent2)" }}>
                Good to know
              </span>
              {info.map((s) => (
                <div key={s.id} className="section-head">
                  <span>{s.title.split(" — ")[0]}</span>
                  <span className="num">{bytes(s.bytes)}</span>
                </div>
              ))}
              <span className="small muted">Managed by Windows. Nothing to do.</span>
            </section>
          )}
        </div>

        <section className="card span-12">
          <div className="panel-head">
            <span className="label">By file type</span>
          </div>
          <div className="list cat-list stagger">
            {summary.categories.map(([cat, b], i) => (
              <div className="row" key={cat} style={si(i)}>
                <span>{cat}</span>
                <Bar value={pct(b, maxCat)} />
                <span className="num">{bytes(b)}</span>
              </div>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}
