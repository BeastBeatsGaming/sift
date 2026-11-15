import { useEffect, useRef, useState } from "react";
import type { ScanProgress } from "../api";
import { ScanGraphic } from "../components/graphics";
import { bytes, count } from "../format";

export function Scanning({ root, progress, onCancel }: { root: string; progress: ScanProgress | null; onCancel: () => void }) {
  const started = useRef(Date.now());
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(t);
  }, []);

  const secs = Math.max(0.001, (now - started.current) / 1000);
  const files = progress?.files ?? 0;
  const analyzing = progress?.phase === "analyzing";

  return (
    <div className="scanning view">
      <ScanGraphic />
      <span className="label">
        {analyzing ? "Analyzing" : "Scanning"} <span className="blink">_</span>
      </span>
      <h2>{analyzing ? "Working out suggestions…" : root}</h2>
      <div className="scan-nums">
        <div>
          <span className="label">Files</span>
          <b>{count(files)}</b>
        </div>
        <div>
          <span className="label">Counted</span>
          <b>{bytes(progress?.bytes ?? 0)}</b>
        </div>
        <div>
          <span className="label">Files / s</span>
          <b>{count(Math.round(files / secs))}</b>
        </div>
        <div>
          <span className="label">Elapsed</span>
          <b>{secs.toFixed(0)}s</b>
        </div>
      </div>
      <div className="scan-path" title={progress?.current}>
        {analyzing ? "Checking caches and comparing large files for duplicates" : progress?.current || "…"}
      </div>
      {!analyzing && (
        <button className="btn btn-ghost" onClick={onCancel}>
          Cancel
        </button>
      )}
    </div>
  );
}
