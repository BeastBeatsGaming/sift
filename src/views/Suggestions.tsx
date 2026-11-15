import { useState } from "react";
import { api, type Action, type Safety, type Suggestion } from "../api";
import { useFeedback } from "../components/feedback";
import { RevealButton, SafetyBadge, Empty, si } from "../components/ui";
import { bytes, count } from "../format";

const FILTERS: { key: "all" | Safety; label: string }[] = [
  { key: "all", label: "All" },
  { key: "Safe", label: "Safe" },
  { key: "Review", label: "Review" },
  { key: "TradeOff", label: "Trade-off" },
];

function actionLabel(a: Action): string {
  switch (a.kind) {
    case "quarantine":
      return a.contents ? "Moves what's inside to .sift-trash · undoable" : "Moves to .sift-trash · undoable";
    case "command":
      return `Runs ${[a.program, ...a.args].join(" ")} · can't undo`;
    case "open_settings":
      return "Opens Windows Settings";
    case "manual":
      return "Do it yourself — see above";
  }
}

/** Selection per suggestion: "all" covers items the UI wasn't sent too. */
type Sel = "all" | Set<string>;

export function Suggestions({ suggestions: all, onOpenDuplicates }: { suggestions: Suggestion[]; onOpenDuplicates: () => void }) {
  const fb = useFeedback();
  // "Leave" items (page file etc.) aren't actionable: shown apart as info.
  const suggestions = all.filter((s) => s.safety !== "Leave");
  const info = all.filter((s) => s.safety === "Leave");
  const [filter, setFilter] = useState<"all" | Safety>("all");
  const [open, setOpen] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  // Safe suggestions start fully selected; everything else starts empty so
  // nothing that needs a look goes by accident.
  const [sel, setSel] = useState<Record<string, Sel>>({});
  const selOf = (s: Suggestion): Sel => sel[s.id] ?? (s.safety === "Safe" ? "all" : new Set());
  const shown = suggestions.filter((s) => filter === "all" || s.safety === filter);

  function picked(s: Suggestion): { n: number; size: number } {
    const v = selOf(s);
    if (v === "all") return { n: s.item_count, size: s.bytes };
    const items = s.items.filter((i) => v.has(i.path));
    return { n: items.length, size: items.reduce((a, i) => a + i.bytes, 0) };
  }

  function toggle(s: Suggestion, path: string) {
    const v = selOf(s);
    const next = new Set(v === "all" ? s.items.map((i) => i.path) : v);
    next.has(path) ? next.delete(path) : next.add(path);
    setSel({ ...sel, [s.id]: next.size === s.item_count ? "all" : next });
  }

  async function run(s: Suggestion) {
    const a = s.action;
    if (a.kind === "open_settings") {
      fb.report(await api.cleanSuggestion(s.id, null));
      return;
    }
    const p = picked(s);
    const isCmd = a.kind === "command";
    const ok = await fb.confirm({
      title: isCmd ? `Run ${a.program}?` : `Move ${count(p.n)} item${p.n === 1 ? "" : "s"} to .sift-trash?`,
      body: isCmd ? (
        <>
          <code className="cmd">{[a.program, ...a.args].join(" ")}</code>
          <p className="muted">
            Clears about {bytes(s.bytes)}. <b className="fg">This can't be undone</b>, but the tool re-downloads whatever it needs later.
          </p>
        </>
      ) : (
        <>
          <p>
            <b>{bytes(p.size)}</b> from “{s.title.split(" — ")[0]}”.
          </p>
          <p className="muted">
            {a.kind === "quarantine" && a.contents ? "The folders themselves stay; files in use are skipped. " : ""}
            You can undo this from History. The space is freed when the trash is emptied (automatically after 14 days).
          </p>
        </>
      ),
      confirm: isCmd ? "Run it" : "Move to trash",
      danger: isCmd,
    });
    if (!ok) return;
    setBusy(s.id);
    try {
      const v = selOf(s);
      fb.report(await api.cleanSuggestion(s.id, v === "all" ? null : [...v]));
      const { [s.id]: _, ...rest } = sel;
      setSel(rest);
    } catch (e) {
      fb.toast({ text: String(e), tone: "alert" });
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="page">
      <div className="section-head">
        <div className="filters">
          {FILTERS.map((f) => {
            const n = f.key === "all" ? suggestions.length : suggestions.filter((s) => s.safety === f.key).length;
            return (
              <button key={f.key} className={`filter ${filter === f.key ? "on" : ""}`} onClick={() => setFilter(f.key)}>
                {f.label}
                <span>{n}</span>
              </button>
            );
          })}
        </div>
        <span className="mono small dim">{bytes(shown.reduce((a, s) => a + s.bytes, 0))} IN VIEW</span>
      </div>

      {shown.length === 0 ? (
        <Empty>No suggestions in this group.</Empty>
      ) : (
        <div className="card sugg-table">
          <div className="thead">
            <span />
            <span className="label">Target</span>
            <span className="label">Class</span>
            <span className="label" style={{ textAlign: "right" }}>
              Size
            </span>
            <span />
          </div>
          <div className="stagger">
            {shown.map((s, i) => {
              const isOpen = open === s.id;
              const v = selOf(s);
              const p = picked(s);
              // duplicates are handled in their own view, where you pick the keeper
              const isDupes = s.id === "duplicates";
              const selectable = s.action.kind === "quarantine" && !isDupes;
              // with nothing ticked the button opens the item list instead of running
              const needsPick = selectable && p.n === 0;
              return (
                <div key={s.id} className={`sugg ${isOpen ? "open" : ""}`} style={si(i)}>
                  <div className="sugg-head" onClick={() => setOpen(isOpen ? null : s.id)}>
                    <span className="chev">▸</span>
                    <span className="sugg-title">
                      <b>{s.title}</b>
                      <span className="sugg-how">{s.how}</span>
                    </span>
                    <SafetyBadge safety={s.safety} />
                    <span className="num big">{bytes(s.bytes)}</span>
                    <span className="sugg-act" onClick={(e) => e.stopPropagation()}>
                      {isDupes ? (
                        <button className="btn btn-ghost" onClick={onOpenDuplicates}>
                          Choose copies ▸
                        </button>
                      ) : s.action.kind === "manual" ? null : (
                        <button
                          className={`btn ${needsPick ? "btn-ghost" : s.safety === "Safe" ? "btn-ok" : ""}`}
                          disabled={busy === s.id || (needsPick && isOpen)}
                          onClick={() => (needsPick ? setOpen(s.id) : run(s))}
                        >
                          {busy === s.id
                            ? "Working…"
                            : s.action.kind === "open_settings"
                              ? "Open Settings"
                              : s.action.kind === "command"
                                ? "Run"
                                : needsPick
                                  ? isOpen
                                    ? "Tick items below"
                                    : "Select items"
                                  : `Clean ${p.n === s.item_count ? "all" : count(p.n)} · ${bytes(p.size)}`}
                        </button>
                      )}
                    </span>
                  </div>
                  {isOpen && (
                    <>
                      <div className="sugg-meta">{actionLabel(s.action).toUpperCase()}</div>
                      <div className="items">
                        {selectable && s.items.length > 0 && (
                          <label className="select-all">
                            <input
                              type="checkbox"
                              checked={v === "all"}
                              ref={(el) => {
                                if (el) el.indeterminate = v !== "all" && v.size > 0;
                              }}
                              onChange={() => setSel({ ...sel, [s.id]: v === "all" ? new Set() : "all" })}
                            />
                            SELECT ALL {count(s.item_count)}
                          </label>
                        )}
                        {s.items.map((it) => (
                          <div className="row" key={it.path}>
                            {selectable ? (
                              <label className="row-main item-pick">
                                <input type="checkbox" checked={v === "all" || v.has(it.path)} onChange={() => toggle(s, it.path)} />
                                <span className="path" title={it.path}>
                                  {it.path}
                                </span>
                              </label>
                            ) : (
                              <span className="row-main path" title={it.path}>
                                {it.path}
                              </span>
                            )}
                            <span className="num">{bytes(it.bytes)}</span>
                            <RevealButton path={it.path} />
                            <button
                              className="icon-btn"
                              title="Ignore — never suggest this again"
                              aria-label="Ignore this path"
                              onClick={async () => {
                                await api.ignorePath(it.path);
                                fb.toast({ text: `Ignoring ${it.path}`, detail: ["Undo in Settings → Ignored paths"] });
                              }}
                            >
                              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden>
                                <path d="M3 3l18 18M10.6 5.1A10 10 0 0 1 12 5c6 0 9.5 7 9.5 7a17 17 0 0 1-3 3.9M6.6 6.6C3.9 8.4 2.5 12 2.5 12s3.5 7 9.5 7a9.6 9.6 0 0 0 5.4-1.6M9.9 9.9a3 3 0 0 0 4.2 4.2" />
                              </svg>
                            </button>
                          </div>
                        ))}
                        {s.item_count > s.items.length && (
                          <div className="row muted small">
                            + {count(s.item_count - s.items.length)} more (smaller){selectable ? " — included by “Select all”" : ""}
                          </div>
                        )}
                        {s.items.length === 0 && <div className="row muted small">No individual paths for this one.</div>}
                      </div>
                    </>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {info.length > 0 && filter === "all" && (
        <div className="info-strip">
          <span className="label">Info · no action</span>
          {info.map((s) => (
            <span key={s.id} className="info-item" title={s.how}>
              {s.title.split(" — ")[0]}
              <b>{bytes(s.bytes)}</b>
            </span>
          ))}
          <span className="small muted">Managed by Windows — shown so you know where the space went.</span>
        </div>
      )}
    </div>
  );
}
