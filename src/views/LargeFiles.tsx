import { useEffect, useMemo, useState } from "react";
import { api, type FileEntry } from "../api";
import { RevealButton, Empty, si } from "../components/ui";
import { TrashButton, useTrash } from "../components/TrashButton";
import { usePathMenu } from "../components/pathMenu";
import { ago, basename, bytes, category, count, dirname } from "../format";

const AGES: { key: string; label: string; days: number }[] = [
  { key: "any", label: "Any age", days: 0 },
  { key: "90", label: "Untouched 3+ months", days: 90 },
  { key: "365", label: "Untouched 1+ year", days: 365 },
  { key: "730", label: "Untouched 2+ years", days: 730 },
];

export function LargeFiles({ version }: { version: number }) {
  const [files, setFiles] = useState<FileEntry[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [type, setType] = useState("all");
  const [age, setAge] = useState("any");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const trash = useTrash();
  const pathMenu = usePathMenu();

  useEffect(() => {
    api
      .bigFiles(500)
      .then((f) => {
        setFiles(f);
        // drop picks that were moved meanwhile
        setPicked((p) => new Set([...p].filter((x) => f.some((y) => y.path === x))));
      })
      .catch((e) => setErr(String(e)));
  }, [version]);

  const types = useMemo(() => {
    const m = new Map<string, number>();
    for (const f of files ?? []) m.set(category(f.path), (m.get(category(f.path)) ?? 0) + f.size);
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  }, [files]);

  if (err) return <div className="page notice notice-alert">{err}</div>;
  if (!files) return <div className="page muted">Loading…</div>;

  const needle = q.toLowerCase();
  const cutoff = Date.now() / 1000 - (AGES.find((a) => a.key === age)?.days ?? 0) * 86400;
  const shown = files.filter(
    (f) =>
      (!needle || f.path.toLowerCase().includes(needle)) &&
      (type === "all" || category(f.path) === type) &&
      (age === "any" || (f.modified > 0 && f.modified < cutoff)),
  );
  const pickedFiles = files.filter((f) => picked.has(f.path));
  const pickedSize = pickedFiles.reduce((a, f) => a + f.size, 0);
  const allShownPicked = shown.length > 0 && shown.every((f) => picked.has(f.path));

  function toggle(path: string) {
    const next = new Set(picked);
    next.has(path) ? next.delete(path) : next.add(path);
    setPicked(next);
  }

  function toggleShown() {
    const next = new Set(picked);
    for (const f of shown) allShownPicked ? next.delete(f.path) : next.add(f.path);
    setPicked(next);
  }

  async function trashPicked() {
    const label = `${count(pickedFiles.length)} large file${pickedFiles.length === 1 ? "" : "s"}`;
    if (await trash(pickedFiles.map((f) => f.path), pickedSize, label)) setPicked(new Set());
  }

  return (
    <div className="page">
      <div className="toolbar">
        <input className="search grow" data-search placeholder="FILTER BY NAME OR FOLDER…  (CTRL+F)" value={q} onChange={(e) => setQ(e.target.value)} />
        <select className="select" value={type} onChange={(e) => setType(e.target.value)} aria-label="File type">
          <option value="all">All types</option>
          {types.map(([t, b]) => (
            <option key={t} value={t}>
              {t} · {bytes(b)}
            </option>
          ))}
        </select>
        <select className="select" value={age} onChange={(e) => setAge(e.target.value)} aria-label="Last modified">
          {AGES.map((a) => (
            <option key={a.key} value={a.key}>
              {a.label}
            </option>
          ))}
        </select>
      </div>

      <div className="section-head">
        <label className="select-all">
          <input
            type="checkbox"
            checked={allShownPicked}
            ref={(el) => {
              if (el) el.indeterminate = !allShownPicked && shown.some((f) => picked.has(f.path));
            }}
            onChange={toggleShown}
            disabled={shown.length === 0}
          />
          SELECT {count(shown.length)} SHOWN
        </label>
        <span className="mono small dim">{bytes(shown.reduce((a, f) => a + f.size, 0))} IN VIEW</span>
      </div>

      {shown.length === 0 && <Empty>No files match.</Empty>}
      <div className="card list files-list stagger">
        {shown.map((f, i) => (
          <div
            className={`row ${picked.has(f.path) ? "picked" : ""}`}
            key={f.path}
            style={si(i)}
            onContextMenu={(e) => pathMenu(e, { path: f.path, size: f.size, label: basename(f.path) })}
          >
            <label className="row-main item-pick">
              <input type="checkbox" checked={picked.has(f.path)} onChange={() => toggle(f.path)} />
              <span className="row-main">
                <span>{basename(f.path)}</span>
                <span className="path muted small" title={f.path}>
                  {dirname(f.path)}
                </span>
              </span>
            </label>
            <span className="muted small num">{ago(f.modified)}</span>
            <span className="num">{bytes(f.size)}</span>
            <RevealButton path={f.path} />
            <TrashButton path={f.path} size={f.size} label={basename(f.path)} />
          </div>
        ))}
      </div>

      {pickedFiles.length > 0 && (
        <div className="bulk-bar">
          <span>
            <b>{count(pickedFiles.length)}</b> selected · {bytes(pickedSize)}
          </span>
          <button className="btn btn-ghost btn-sm" onClick={() => setPicked(new Set())}>
            Clear
          </button>
          <button className="btn btn-sm" onClick={trashPicked}>
            Move to .sift-trash
          </button>
        </div>
      )}
    </div>
  );
}
