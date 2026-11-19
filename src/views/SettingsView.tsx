import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type AppInfo, type BackgroundStatus, type Settings } from "../api";
import { bugReportUrl, copyText, REPO, type Release } from "../meta";
import { bytes } from "../format";
import { useFeedback } from "../components/feedback";
import { Empty } from "../components/ui";

function NumberField({ label, hint, value, onChange, min = 0, suffix }: { label: string; hint?: string; value: number; onChange: (v: number) => void; min?: number; suffix: string }) {
  return (
    <label className="field">
      <span className="field-text">
        <span>{label}</span>
        {hint && <span className="muted small">{hint}</span>}
      </span>
      <span className="field-input">
        <input type="number" min={min} value={value} onChange={(e) => onChange(Math.max(min, Number(e.target.value) || 0))} />
        <span className="muted small">{suffix}</span>
      </span>
    </label>
  );
}

function Toggle({ label, hint, value, onChange }: { label: string; hint?: string; value: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="field">
      <span className="field-text">
        <span>{label}</span>
        {hint && <span className="muted small">{hint}</span>}
      </span>
      <button type="button" role="switch" aria-checked={value} className={`switch ${value ? "on" : ""}`} onClick={() => onChange(!value)}>
        <i />
      </button>
    </label>
  );
}

const SHORTCUTS: [string, string][] = [
  ["Ctrl+R", "Rescan"],
  ["Ctrl+F", "Search"],
  ["Ctrl+1…6", "Switch tab"],
  ["Ctrl+,", "Settings"],
  ["Esc", "Close"],
  ["Right-click", "File / folder actions"],
];

function About({ info, update, onCheckUpdate }: { info: AppInfo | null; update: Release | null; onCheckUpdate: () => void }) {
  const fb = useFeedback();
  const open = (url: string) => api.openUrl(url).catch((e) => fb.toast({ text: String(e), tone: "warn" }));
  return (
    <section>
      <h3>About</h3>
      <div className="card about">
        <div className="about-block about-top">
          <div className="about-id">
            <span className="about-name">
              Sift <span className="mono">{info?.version ?? ""}</span>
            </span>
            {update ? <span className="about-pill pill-update">v{update.version} available</span> : <span className="about-pill">Up to date</span>}
          </div>
          {update ? (
            <button className="btn btn-sm" onClick={() => open(update.url)}>
              Get v{update.version}
            </button>
          ) : (
            <button className="btn btn-ghost btn-sm" onClick={onCheckUpdate}>
              Check for updates
            </button>
          )}
          <div className="about-meta muted small">
            <span>{info?.os ?? "Windows"}</span>
            <span>MIT license</span>
            <button className="link" onClick={() => open(REPO)}>
              GitHub ↗
            </button>
            <button className="link" onClick={() => open(`${REPO}/releases`)}>
              Releases ↗
            </button>
          </div>
        </div>

        <div className="about-block">
          <div className="label">Report a problem</div>
          <p className="muted small">Opens a GitHub issue with your version filled in. Copy diagnostics first and paste them in — they hold the end of Sift's log, never file contents.</p>
          <div className="about-actions">
            <button className="btn btn-sm" onClick={() => open(bugReportUrl(info))}>
              Report a bug
            </button>
            <button
              className="btn btn-ghost btn-sm"
              onClick={async () => {
                try {
                  await copyText(await api.diagnostics());
                  fb.toast({ text: "Diagnostics copied", detail: ["Paste them into the bug report"] });
                } catch (e) {
                  fb.toast({ text: String(e), tone: "warn" });
                }
              }}
            >
              Copy diagnostics
            </button>
            {info?.log_path && (
              <button className="btn btn-ghost btn-sm" onClick={() => api.reveal(info.log_path!).catch((e) => fb.toast({ text: String(e), tone: "warn" }))}>
                Show log file
              </button>
            )}
          </div>
        </div>

        <div className="about-block">
          <div className="label">Keyboard shortcuts</div>
          <div className="shortcuts">
            {SHORTCUTS.map(([k, what]) => (
              <div key={k} className="shortcut">
                <span className="kbd">{k}</span>
                <span className="muted small">{what}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

export function SettingsView({
  settings,
  onSaved,
  hasPalette,
  info,
  update,
  onCheckUpdate,
}: {
  settings: Settings;
  onSaved: (s: Settings) => void;
  hasPalette: boolean;
  info: AppInfo | null;
  update: Release | null;
  onCheckUpdate: () => void;
}) {
  const fb = useFeedback();
  const [s, setS] = useState(settings);
  useEffect(() => setS(settings), [settings]);
  const dirty = JSON.stringify(s) !== JSON.stringify(settings);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS({ ...s, [k]: v });
  const [bg, setBg] = useState<BackgroundStatus | null>(null);
  useEffect(() => {
    api.backgroundStatus().then(setBg).catch(() => {});
  }, [settings]);
  const fmt = (t: number) => new Date(t * 1000).toLocaleString("en-IN", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

  async function pickScanPath() {
    const dir = await open({ directory: true, multiple: false, title: "Folder or drive to scan in the background" });
    if (typeof dir === "string") set("scan_path", dir);
  }

  async function save(next = s) {
    try {
      await api.saveSettings(next);
      onSaved(next);
      fb.toast({ text: "Settings saved" });
    } catch (e) {
      fb.toast({ text: String(e), tone: "alert" });
    }
  }

  return (
    <div className="page settings">
      <section>
        <h3>Appearance</h3>
        <div className="card fields">
          <Toggle
            label="Match my rice palette"
            hint={hasPalette ? "Colours from your rice's palette.json, updated live when the wallpaper changes" : "No rice palette found — using Sift's default colours"}
            value={s.rice_theme}
            onChange={(v) => set("rice_theme", v)}
          />
          <Toggle label="Custom title bar" hint="Off uses the standard Windows title bar" value={s.custom_titlebar} onChange={(v) => set("custom_titlebar", v)} />
        </div>
      </section>

      <section>
        <h3>Background scan</h3>
        <p className="muted small">Sift starts with Windows, waits in the tray and rescans on a schedule. Closing the window keeps it in the tray; quit from the tray menu.</p>
        <div className="card fields">
          <Toggle
            label="Scan in the background"
            hint="Notifies you when there's enough safe to clear — nothing is ever cleaned on its own"
            value={s.background_scan}
            onChange={(v) => set("background_scan", v)}
          />
          <NumberField label="Scan every" value={s.scan_every_days} min={1} onChange={(v) => set("scan_every_days", v)} suffix="days" />
          <NumberField label="Notify when safe-to-clear reaches" value={s.notify_gb} min={1} onChange={(v) => set("notify_gb", v)} suffix="GB" />
          <div className="field">
            <span className="field-text">
              <span>What to scan</span>
              <span className="muted small path">{s.scan_path || `${bg?.default_root ?? "C:\\"} (the Windows drive)`}</span>
            </span>
            <span className="field-input">
              {s.scan_path && (
                <button className="btn btn-ghost btn-sm" onClick={() => set("scan_path", "")}>
                  Reset
                </button>
              )}
              <button className="btn btn-ghost btn-sm" onClick={pickScanPath}>
                Choose…
              </button>
            </span>
          </div>
          {bg && settings.background_scan && (
            <div className="field bg-status">
              <span className="field-text">
                <span>{bg.running ? "Scanning now…" : bg.last_run ? `Last: ${fmt(bg.last_run)} · ${bytes(bg.last_safe)} safe on ${bg.last_root}` : "Not run yet"}</span>
                {bg.next_run && !bg.running && <span className="muted small">Next: {fmt(bg.next_run)}</span>}
              </span>
              <button
                className="btn btn-ghost btn-sm"
                disabled={bg.running}
                onClick={async () => {
                  try {
                    await api.runBackgroundScan();
                    fb.toast({ text: "Background scan started", detail: ["You'll get a notification when it's done"] });
                    setBg({ ...bg, running: true });
                  } catch (e) {
                    fb.toast({ text: String(e), tone: "warn" });
                  }
                }}
              >
                Scan now
              </button>
            </div>
          )}
        </div>
      </section>

      <section>
        <h3>Trash</h3>
        <div className="card fields">
          <NumberField label="Empty .sift-trash automatically after" hint="Checked when Sift starts. 0 = never" value={s.auto_purge_days} onChange={(v) => set("auto_purge_days", v)} suffix="days" />
        </div>
      </section>

      <section>
        <h3>Suggestions</h3>
        <p className="muted small">These apply from the next scan.</p>
        <div className="card fields">
          <NumberField label="Hide suggestions smaller than" value={s.min_suggestion_mb} onChange={(v) => set("min_suggestion_mb", v)} suffix="MB" />
          <NumberField label="Flag large files untouched for" value={s.stale_days} min={1} onChange={(v) => set("stale_days", v)} suffix="days" />
          <NumberField label="Flag Downloads older than" value={s.downloads_days} min={1} onChange={(v) => set("downloads_days", v)} suffix="days" />
          <Toggle label="Find duplicate files" hint="Reads samples of every large file after the scan" value={s.find_duplicates} onChange={(v) => set("find_duplicates", v)} />
          <NumberField label="Only check duplicates of at least" value={s.duplicate_min_mb} min={1} onChange={(v) => set("duplicate_min_mb", v)} suffix="MB" />
        </div>
      </section>

      <section>
        <h3>Ignored paths</h3>
        <p className="muted small">Never suggested, including everything inside. Add from a suggestion's item list with the eye button.</p>
        <div className="card list">
          {s.ignored.length === 0 && <Empty>Nothing ignored.</Empty>}
          {s.ignored.map((p) => (
            <div className="row ignored-row" key={p}>
              <span className="row-main path" title={p}>
                {p}
              </span>
              <button className="btn btn-ghost btn-sm" onClick={() => set("ignored", s.ignored.filter((x) => x !== p))}>
                Remove
              </button>
            </div>
          ))}
        </div>
      </section>

      <About info={info} update={update} onCheckUpdate={onCheckUpdate} />

      <div className="settings-actions">
        <button className="btn btn-ghost" disabled={!dirty} onClick={() => setS(settings)}>
          Discard
        </button>
        <button className="btn" disabled={!dirty} onClick={() => save()}>
          Save
        </button>
      </div>
    </div>
  );
}
