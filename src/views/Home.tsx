import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type Drive } from "../api";
import { Meter, si } from "../components/ui";
import { Logo } from "../components/Logo";
import { bytes, pct } from "../format";

export function Home({ onScan, error, admin, onAdmin }: { onScan: (path: string) => void; error: string | null; admin: boolean; onAdmin: () => void }) {
  const [drives, setDrives] = useState<Drive[] | null>(null);

  useEffect(() => {
    api.listDrives().then(setDrives).catch(() => setDrives([]));
    // dev/test hook: VITE_SIFT_AUTOSCAN=<folder> scans on launch
    const auto = import.meta.env.VITE_SIFT_AUTOSCAN as string | undefined;
    if (auto && !sessionStorage.getItem("sift-autoscanned")) {
      sessionStorage.setItem("sift-autoscanned", "1");
      onScan(auto);
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  async function pickFolder() {
    const dir = await open({ directory: true, multiple: false, title: "Choose a folder to scan" });
    if (typeof dir === "string") onScan(dir);
  }

  return (
    <div className="home view">
      <header className="home-hero">
        <Logo size={72} />
        <div className="home-head">
          <span className="label">Disk analysis · cleanup</span>
          <h1>What's filling your drive?</h1>
          <p className="muted">Sift scans a drive, shows where the space went, and suggests what's safe to clear. Nothing is removed without your say-so.</p>
        </div>
      </header>

      {error && <div className="notice notice-alert">{error}</div>}

      <div className="drive-grid stagger">
        {drives === null && <div className="muted mono small">LOOKING FOR DRIVES…</div>}
        {drives?.map((d, i) => {
          const used = d.total - d.free;
          const p = pct(used, d.total);
          const tone = p > 90 ? "var(--alert)" : p > 75 ? "var(--warn)" : "color-mix(in srgb, var(--accent) 80%, transparent)";
          return (
            <button key={d.path} className="drive-card" style={si(i)} onClick={() => onScan(d.path)}>
              <div className="drive-top">
                <span className="drive-letter">{d.path}</span>
                <span className="muted small">{d.name || "Local disk"}</span>
              </div>
              <Meter total={d.total} parts={[[used, tone]]} segments={30} delay={14} />
              <div className="drive-nums">
                <span>
                  <b>{bytes(d.free)}</b> free
                </span>
                <span className="muted">{p.toFixed(0)}% of {bytes(d.total)}</span>
              </div>
              <span className="drive-cta">SCAN ▸</span>
            </button>
          );
        })}
        <button className="drive-card drive-folder" style={si(drives?.length ?? 0)} onClick={pickFolder}>
          <span className="drive-letter">+ FOLDER</span>
          <span className="muted small">Faster — e.g. just your user folder</span>
        </button>
      </div>

      {!admin && (
        <p className="muted small hint">
          System folders are skipped without admin rights.{" "}
          <button className="link" onClick={onAdmin}>
            Restart as admin
          </button>
        </p>
      )}
    </div>
  );
}
