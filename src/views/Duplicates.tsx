import { useEffect, useState } from "react";
import { api, type DuplicateGroup } from "../api";
import { useFeedback } from "../components/feedback";
import { RevealButton, Empty, si } from "../components/ui";
import { bytes, count } from "../format";

type Pick = { keep: string; remove: string[] };

function when(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString("en-IN", { day: "numeric", month: "short", year: "numeric" });
}

export function Duplicates({ version }: { version: number }) {
  const fb = useFeedback();
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null);
  // keeper per group, keyed by the group's first (oldest) path; default = oldest
  const [keep, setKeep] = useState<Record<string, string>>({});
  // groups the user left out of "Clean all"
  const [skip, setSkip] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    api.duplicates().then(setGroups);
  }, [version]);

  if (!groups) return <div className="page muted">Loading…</div>;
  if (groups.length === 0) return <Empty>No duplicate files left. Nice and tidy.</Empty>;

  const keyOf = (g: DuplicateGroup) => g.paths[0];
  const keeperOf = (g: DuplicateGroup) => {
    const k = keep[keyOf(g)];
    return k && g.paths.includes(k) ? k : g.paths[0];
  };
  const pickOf = (g: DuplicateGroup): Pick => {
    const k = keeperOf(g);
    return { keep: k, remove: g.paths.filter((p) => p !== k) };
  };
  const active = groups.filter((g) => !skip.has(keyOf(g)));
  const activeBytes = active.reduce((a, g) => a + g.wasted, 0);

  async function clean(gs: DuplicateGroup[], key: string) {
    const picks = gs.map(pickOf);
    const n = picks.reduce((a, p) => a + p.remove.length, 0);
    const size = gs.reduce((a, g) => a + g.wasted, 0);
    const ok = await fb.confirm({
      title: `Move ${count(n)} duplicate${n === 1 ? "" : "s"} to .sift-trash?`,
      body: (
        <>
          <p>
            <b>{bytes(size)}</b> from {count(gs.length)} group{gs.length === 1 ? "" : "s"}. The copy you picked in each group stays.
          </p>
          <p className="muted">Every copy is compared byte-for-byte with the one you keep first; anything that differs is left alone. Undo from History.</p>
        </>
      ),
      confirm: "Verify and move",
    });
    if (!ok) return;
    setBusy(key);
    try {
      fb.report(await api.cleanDuplicates(picks));
    } catch (e) {
      fb.toast({ text: String(e), tone: "alert" });
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="page">
      <div className="dupe-head">
        <p className="muted">
          {groups.length} set{groups.length === 1 ? "" : "s"} of matching files waste <b className="fg">{bytes(groups.reduce((a, g) => a + g.wasted, 0))}</b>. Pick the copy to keep in each — the oldest is chosen
          by default. Nothing moves until it's been fully compared.
        </p>
        <button className="btn btn-ok" disabled={active.length === 0 || busy !== null} onClick={() => clean(active, "all")}>
          {busy === "all" ? "Verifying…" : `Clean ${active.length === groups.length ? "all" : count(active.length)} · ${bytes(activeBytes)}`}
        </button>
      </div>
      <div className="dupe-list stagger">
        {groups.map((g, gi) => {
          const key = keyOf(g);
          const k = keeperOf(g);
          const included = !skip.has(key);
          return (
            <div className={`card dupe ${included ? "" : "dupe-off"}`} key={key} style={si(gi)}>
              <div className="section-head">
                <label className="dupe-include">
                  <input
                    type="checkbox"
                    checked={included}
                    onChange={() => {
                      const next = new Set(skip);
                      next.has(key) ? next.delete(key) : next.add(key);
                      setSkip(next);
                    }}
                  />
                  <span className="muted small">
                    {g.paths.length} × {bytes(g.size)}
                  </span>
                </label>
                <span className="dupe-right">
                  <span className="num">{bytes(g.wasted)} wasted</span>
                  <button className="btn btn-ghost btn-sm" disabled={busy !== null} onClick={() => clean([g], key)}>
                    {busy === key ? "Verifying…" : "Clean this group"}
                  </button>
                </span>
              </div>
              {g.paths.map((p, i) => {
                const isKeep = p === k;
                return (
                  <label className={`row dupe-row ${isKeep ? "keep" : ""}`} key={p}>
                    <input type="radio" name={key} checked={isKeep} onChange={() => setKeep({ ...keep, [key]: p })} />
                    <span className={`tag ${isKeep ? "" : "tag-move"}`}>{isKeep ? "keep" : "move"}</span>
                    <span className="row-main">
                      <span className="path" title={p}>
                        {p}
                      </span>
                    </span>
                    <span className="muted small">{g.modified?.[i] ? when(g.modified[i]) : ""}</span>
                    <RevealButton path={p} />
                  </label>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
