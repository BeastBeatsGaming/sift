import { useEffect, useMemo, useState } from "react";
import { api, type DirView, type SearchHit } from "../api";
import { usePathMenu } from "../components/pathMenu";
import { Treemap, type TreemapItem } from "../components/Treemap";
import { Bar, RevealButton, si } from "../components/ui";
import { TrashButton } from "../components/TrashButton";
import { basename, bytes, count, dirname, pct } from "../format";

function joinPath(dir: string, name: string): string {
  const sep = dir.includes("\\") || /^[A-Za-z]:/.test(dir) ? "\\" : "/";
  return dir.endsWith(sep) ? dir + name : dir + sep + name;
}

export function Explorer({ version }: { version: number }) {
  const [path, setPath] = useState<number[]>([]);
  const [dir, setDir] = useState<DirView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const pathMenu = usePathMenu();

  // search the whole scan, debounced
  useEffect(() => {
    if (!q.trim()) return setHits(null);
    const t = setTimeout(() => {
      api.search(q).then(setHits).catch(() => setHits([]));
    }, 180);
    return () => clearTimeout(t);
  }, [q, version]);

  useEffect(() => {
    api
      .dir(path)
      .then(setDir)
      // after a cleanup the folder may be gone: fall back to the root
      .catch((e) => (path.length ? setPath([]) : setErr(String(e))));
  }, [path, version]);

  const items: TreemapItem[] = useMemo(() => {
    if (!dir) return [];
    const kids: TreemapItem[] = dir.children.map((c) => ({
      key: `d${c.index}`,
      label: c.name,
      size: c.size,
      tone: c.denied ? "denied" : c.regen_rule ? "regen" : "dir",
      onClick: c.has_children ? () => setPath([...path, c.index]) : undefined,
    }));
    if (dir.own_bytes > 0) kids.push({ key: "files", label: "Files here", size: dir.own_bytes, tone: "files" });
    return kids;
  }, [dir, path]);

  if (err) return <div className="page notice notice-alert">{err}</div>;
  if (!dir) return <div className="page muted">Loading…</div>;

  const search = (
    <input
      className="search"
      data-search
      placeholder="SEARCH FOLDERS AND LARGE FILES IN THIS SCAN…  (CTRL+F)"
      value={q}
      onChange={(e) => setQ(e.target.value)}
      onKeyDown={(e) => e.key === "Escape" && q && (e.stopPropagation(), setQ(""))}
    />
  );

  if (hits) {
    return (
      <div className="page explorer">
        {search}
        <div className="section-head">
          <span className="mono small dim">
            {count(hits.length)} MATCH{hits.length === 1 ? "" : "ES"}
            {hits.length >= 200 ? " · LARGEST 200" : ""}
          </span>
          <button className="link" onClick={() => setQ("")}>
            Clear search
          </button>
        </div>
        {hits.length === 0 ? (
          <div className="muted small">No folders or large files match. Small files aren't listed one by one, so they can't be searched.</div>
        ) : (
          <div className="card list explorer-list stagger">
            {hits.map((h, i) => (
              <div
                key={h.path}
                style={si(i)}
                className={`row ${h.index_path ? "clickable" : ""}`}
                title={h.index_path ? "Open this folder" : undefined}
                onClick={() => {
                  if (!h.index_path) return;
                  setPath(h.index_path);
                  setQ("");
                }}
                onContextMenu={(e) => pathMenu(e, { path: h.path, size: h.size, label: basename(h.path) })}
              >
                <span className="row-main">
                  <span>
                    {basename(h.path)}
                    <span className="tag tag-move">{h.is_dir ? "folder" : "file"}</span>
                  </span>
                  <span className="path muted small" title={h.path}>
                    {dirname(h.path)}
                  </span>
                </span>
                <span />
                <span className="num">{bytes(h.size)}</span>
                <TrashButton path={h.path} size={h.size} label={basename(h.path)} />
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="page explorer">
      {search}
      <div className="crumbs">
        {path.length > 0 && (
          <button className="icon-btn" onClick={() => setPath(path.slice(0, -1))} title="Up one level">
            ←
          </button>
        )}
        {dir.crumbs.map((c, i) => (
          <span key={i} className="crumb">
            {i > 0 && <span className="sep">›</span>}
            <button className="link" disabled={i === dir.crumbs.length - 1} onClick={() => setPath(path.slice(0, i))}>
              {c}
            </button>
          </span>
        ))}
        <span className="grow" />
        <span className="muted small">
          {bytes(dir.size)} · {count(dir.file_count)} files
        </span>
        <RevealButton path={dir.full_path} />
      </div>

      <div className="legend small muted">
        <span><i className="sw sw-dir" /> folder</span>
        <span><i className="sw sw-regen" /> rebuildable (node_modules, Unity Library…)</span>
        <span><i className="sw sw-files" /> files in this folder</span>
      </div>

      <Treemap items={items} />

      <div className="card list explorer-list stagger" key={dir.full_path}>
        {dir.children.map((c, i) => (
          <div
            key={c.index}
            style={si(i)}
            className={`row ${c.has_children ? "clickable" : ""}`}
            onClick={c.has_children ? () => setPath([...path, c.index]) : undefined}
            onContextMenu={(e) =>
              pathMenu(e, {
                path: joinPath(dir.full_path, c.name),
                size: c.size,
                label: c.name,
                before: c.has_children ? [{ label: "Open", onClick: () => setPath([...path, c.index]) }] : [],
              })
            }
          >
            <span className="row-main">
              <span className={c.regen_rule ? "regen-name" : ""}>
                {c.name}
                {c.denied && <span className="muted small"> · no access</span>}
                {c.regen_rule && <span className="tag">rebuildable</span>}
              </span>
              <Bar value={pct(c.size, dir.size)} tone={c.regen_rule ? "ok" : "accent"} />
            </span>
            <span className="num muted small">{pct(c.size, dir.size).toFixed(1)}%</span>
            <span className="num">{bytes(c.size)}</span>
            <TrashButton path={joinPath(dir.full_path, c.name)} size={c.size} label={c.name} />
          </div>
        ))}
        {dir.child_count > dir.children.length && (
          <div className="row muted small">+ {count(dir.child_count - dir.children.length)} smaller folders</div>
        )}
      </div>
    </div>
  );
}
