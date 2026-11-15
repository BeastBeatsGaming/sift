import { useEffect, useState } from "react";
import { api, type BatchView, type HistoryView } from "../api";
import { useFeedback } from "../components/feedback";
import { Bytes, Empty, si } from "../components/ui";
import { bytes, count } from "../format";
import { summarizeFreed } from "../freed";

function when(unix: number): string {
  return new Date(unix * 1000).toLocaleString("en-IN", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

function status(b: BatchView): { text: string; tone: string } {
  if (b.kind === "command") return b.exit_code === 0 ? { text: "Ran", tone: "ok" } : { text: "Failed", tone: "alert" };
  if (b.in_trash > 0) return { text: "In trash", tone: "warn" };
  if (b.restored === b.entry_count) return { text: "Restored", tone: "accent2" };
  if (b.purged === b.entry_count) return { text: "Deleted", tone: "muted" };
  return { text: "Partly restored", tone: "accent2" };
}

export function History({ version }: { version: number }) {
  const fb = useFeedback();
  const [h, setH] = useState<HistoryView | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    api.history().then(setH).catch((e) => fb.toast({ text: String(e), tone: "alert" }));
  }, [version, fb]);

  async function act(key: string, f: () => Promise<Parameters<typeof fb.report>[0]>) {
    setBusy(key);
    try {
      fb.report(await f());
    } catch (e) {
      fb.toast({ text: String(e), tone: "alert" });
    } finally {
      setBusy(null);
    }
  }

  async function purge(b: BatchView) {
    const ok = await fb.confirm({
      title: "Delete permanently?",
      body: <p>“{b.label}” — {bytes(b.in_trash)}. This frees the space and can't be undone.</p>,
      confirm: "Delete",
      danger: true,
    });
    if (ok) act(b.id, () => api.purgeBatch(b.id));
  }

  async function emptyAll() {
    const ok = await fb.confirm({
      title: "Empty .sift-trash?",
      body: <p>Permanently deletes {bytes(h!.in_trash)} of moved items. Nothing in it can be restored afterwards.</p>,
      confirm: "Empty trash",
      danger: true,
    });
    if (ok) act("all", () => api.emptyTrash());
  }

  if (!h) return <div className="page muted">Loading…</div>;
  const f = summarizeFreed(h);
  const maxMonth = Math.max(1, ...f.months.map((m) => m.bytes));

  return (
    <div className="page">
      <section className="card hist-summary">
        <div className="hist-readouts">
          <div className="readout">
            <span className="label">Freed with Sift</span>
            <b className="lg" style={{ color: "var(--accent2)" }}>
              <Bytes n={f.total} />
            </b>
          </div>
          <div className="readout">
            <span className="label">This month</span>
            <b className="lg">
              <Bytes n={f.thisMonth} />
            </b>
          </div>
          <div className="readout">
            <span className="label">Cleanups</span>
            <b className="lg">{count(f.cleanups)}</b>
          </div>
          <div className="readout">
            <span className="label">In .sift-trash</span>
            <b className="lg" style={{ color: h.in_trash > 0 ? "var(--warn)" : undefined }}>
              <Bytes n={h.in_trash} />
            </b>
          </div>
          <div className="hist-empty">
            <button className="btn btn-danger" disabled={h.in_trash === 0 || busy === "all"} onClick={emptyAll}>
              {busy === "all" ? "Emptying…" : "Empty trash"}
            </button>
            <span className="small muted">{h.auto_purge_days > 0 ? `Auto-empties after ${h.auto_purge_days} days` : "Never auto-emptied"}</span>
          </div>
        </div>
        <div className="month-chart" aria-label="Freed per month">
          {f.months.map((m, i) => {
            const now = i === f.months.length - 1;
            return (
              <div key={m.key} className={`month-col ${now ? "now" : ""}`} title={`${count(m.cleanups)} cleanup${m.cleanups === 1 ? "" : "s"}`}>
                <div className="month-track">
                  <i style={{ height: m.bytes ? `${Math.max(4, (m.bytes / maxMonth) * 100)}%` : 0, animationDelay: `${i * 50}ms` }} />
                </div>
                <span className="month-label">{m.label.toUpperCase()}</span>
                <span className={`month-value ${m.bytes ? "" : "zero"}`}>{m.bytes ? bytes(m.bytes) : "—"}</span>
              </div>
            );
          })}
        </div>
      </section>

      {h.batches.length === 0 ? (
        <Empty>Nothing cleaned yet. Actions from Suggestions, Explorer and Large files show up here.</Empty>
      ) : (
        <section className="card history">
          <div className="hist-row hist-head">
            <span />
            <span className="label">Cleanup</span>
            <span className="label">When</span>
            <span className="label">Status</span>
            <span className="label" style={{ textAlign: "right" }}>
              Freed
            </span>
            <span />
          </div>
          <div className="stagger">
            {h.batches.map((b, i) => {
              const st = status(b);
              const isOpen = open === b.id;
              return (
                <div key={b.id} className={`hist ${isOpen ? "open" : ""}`} style={si(i)}>
                  <div className="hist-row" onClick={() => setOpen(isOpen ? null : b.id)}>
                    <span className="chev">▸</span>
                    <span className="hist-main">
                      <span>{b.label}</span>
                      <span className="muted small">{b.kind === "command" ? b.command : `${count(b.entry_count)} item${b.entry_count === 1 ? "" : "s"}`}</span>
                    </span>
                    <span className="mono small muted">{when(b.created)}</span>
                    <span className="hist-status" style={{ color: `var(--${st.tone})` }}>
                      {st.text}
                    </span>
                    <span className="num">{bytes(b.freed)}</span>
                    <span className="hist-actions" onClick={(e) => e.stopPropagation()}>
                      {b.in_trash > 0 && (
                        <>
                          <button className="btn btn-ghost btn-sm" disabled={busy === b.id} onClick={() => act(b.id, () => api.restoreBatch(b.id))}>
                            Undo
                          </button>
                          <button className="btn btn-ghost btn-sm danger-text" disabled={busy === b.id} onClick={() => purge(b)}>
                            Delete
                          </button>
                        </>
                      )}
                    </span>
                  </div>
                  {isOpen && (
                    <div className="hist-detail">
                      {b.kind === "command" ? (
                        <pre className="cmd-out">{b.output || "(no output)"}</pre>
                      ) : (
                        <>
                          {b.entries.map((e) => (
                            <div key={e.original} className="hist-entry">
                              <span className="path" title={e.original}>
                                {e.original}
                              </span>
                              <span className="mono small muted">{e.state.replace("_", " ").toUpperCase()}</span>
                              <span className="num small">{bytes(e.bytes)}</span>
                            </div>
                          ))}
                          {b.entry_count > b.entries.length && <div className="muted small">+ {count(b.entry_count - b.entries.length)} more</div>}
                        </>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      )}
    </div>
  );
}
