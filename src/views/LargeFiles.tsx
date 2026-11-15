import { useEffect, useState } from "react";
import { api, type FileEntry } from "../api";
import { RevealButton, Empty, si } from "../components/ui";
import { TrashButton } from "../components/TrashButton";
import { ago, basename, bytes, dirname } from "../format";

export function LargeFiles({ version }: { version: number }) {
  const [files, setFiles] = useState<FileEntry[] | null>(null);
  const [q, setQ] = useState("");

  useEffect(() => {
    api.bigFiles(500).then(setFiles);
  }, [version]);

  if (!files) return <div className="page muted">Loading…</div>;
  const shown = q ? files.filter((f) => f.path.toLowerCase().includes(q.toLowerCase())) : files;

  return (
    <div className="page">
      <input className="search" placeholder="FILTER BY NAME OR FOLDER…" value={q} onChange={(e) => setQ(e.target.value)} />
      {shown.length === 0 && <Empty>No files match.</Empty>}
      <div className="card list files-list stagger">
        {shown.map((f, i) => (
          <div className="row" key={f.path} style={si(i)}>
            <span className="row-main">
              <span>{basename(f.path)}</span>
              <span className="path muted small" title={f.path}>
                {dirname(f.path)}
              </span>
            </span>
            <span className="muted small num">{ago(f.modified)}</span>
            <span className="num">{bytes(f.size)}</span>
            <RevealButton path={f.path} />
            <TrashButton path={f.path} size={f.size} label={basename(f.path)} />
          </div>
        ))}
      </div>
    </div>
  );
}
