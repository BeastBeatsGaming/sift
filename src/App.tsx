import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, onDataEvents, onScanEvents, onThemeChanged, type Palette, type ScanProgress, type Settings, type Suggestion, type Summary } from "./api";
import { applyPalette } from "./theme";
import { Topbar, type TabDef } from "./components/Topbar";
import { useFeedback } from "./components/feedback";
import { SettingsView } from "./views/SettingsView";
import { bytes } from "./format";
import { summarizeFreed, type FreedSummary } from "./freed";
import { Home } from "./views/Home";
import { Scanning } from "./views/Scanning";
import { Overview } from "./views/Overview";
import { Suggestions } from "./views/Suggestions";
import { Explorer } from "./views/Explorer";
import { LargeFiles } from "./views/LargeFiles";
import { Duplicates } from "./views/Duplicates";
import { History } from "./views/History";

type Tab = "overview" | "suggestions" | "explorer" | "files" | "duplicates" | "history";
type Phase = { kind: "home" } | { kind: "scanning"; root: string } | { kind: "results" };

const TITLES: Record<Tab, string> = {
  overview: "Overview",
  suggestions: "Suggestions",
  explorer: "Explorer",
  files: "Large files",
  duplicates: "Duplicates",
  history: "History",
};

export default function App() {
  const [phase, setPhase] = useState<Phase>({ kind: "home" });
  const [tab, setTab] = useState<Tab>("overview");
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [error, setError] = useState<string | null>(null);
  // bumps after every cleanup so open views refetch
  const [version, setVersion] = useState(0);
  const [stale, setStale] = useState(false);
  const [inTrash, setInTrash] = useState(0);
  const [freed, setFreed] = useState<FreedSummary>(() => summarizeFreed(null));
  const lastRoot = useRef<string | null>(null);
  // remounts result views after each scan so they refetch
  const scanId = useRef(0);
  const fb = useFeedback();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [palette, setPalette] = useState<Palette | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [admin, setAdmin] = useState(false);

  function showResults(s: Summary, sg: Suggestion[]) {
    scanId.current++;
    setSummary(s);
    setSuggestions(sg);
    lastRoot.current = s.root_path;
    setTab("overview");
    setStale(false);
    setPhase({ kind: "results" });
  }

  useEffect(() => {
    api.settings().then(setSettings).catch(() => {});
    api.palette().then(setPalette).catch(() => {});
    api.isAdmin().then(setAdmin).catch(() => {});
    // reopened from the tray, or a background scan is loaded: show it
    Promise.all([api.summary(), api.suggestions()])
      .then(([s, sg]) => showResults(s, sg))
      .catch(() => {});
    const off = onThemeChanged(setPalette);
    return () => {
      off.then((f) => f());
    };
  }, []);

  // live rice colours
  useEffect(() => {
    applyPalette(settings?.rice_theme ? palette : null);
  }, [settings?.rice_theme, palette]);

  // frameless window with our own controls, or the standard title bar
  useEffect(() => {
    if (settings) getCurrentWindow().setDecorations(!settings.custom_titlebar).catch(() => {});
  }, [settings?.custom_titlebar]); // eslint-disable-line react-hooks/exhaustive-deps

  async function runAsAdmin() {
    const ok = await fb.confirm({
      title: "Restart as administrator?",
      body: <p className="muted">Sift closes and reopens with admin rights so it can read Windows system folders. The current results are discarded.</p>,
      confirm: "Restart",
    });
    if (!ok) return;
    try {
      await api.relaunchAsAdmin();
    } catch (e) {
      fb.toast({ text: String(e), tone: "warn" });
    }
  }

  useEffect(() => {
    const off = onScanEvents({
      progress: setProgress,
      done: async () => {
        const [s, sg] = await Promise.all([api.summary(), api.suggestions()]);
        showResults(s, sg);
      },
      cancelled: () => setPhase({ kind: "home" }),
    });
    const offData = onDataEvents({
      changed: async () => {
        setVersion((v) => v + 1);
        api.settings().then(setSettings).catch(() => {});
        try {
          const [s, sg] = await Promise.all([api.summary(), api.suggestions()]);
          setSummary(s);
          setSuggestions(sg);
        } catch {
          /* no scan loaded: nothing to refresh */
        }
      },
      stale: () => setStale(true),
    });
    return () => {
      off.then((f) => f());
      offData.then((f) => f());
    };
  }, []);

  useEffect(() => {
    api
      .history()
      .then((h) => {
        setInTrash(h.in_trash);
        setFreed(summarizeFreed(h));
      })
      .catch(() => {});
  }, [version]);

  async function startScan(path: string) {
    setError(null);
    setProgress(null);
    setShowSettings(false);
    try {
      await api.startScan(path);
      lastRoot.current = path;
      setPhase({ kind: "scanning", root: path });
    } catch (e) {
      setError(String(e));
    }
  }

  const actionable = suggestions.filter((s) => s.safety !== "Leave");
  const results = phase.kind === "results" && summary;
  const tabs: TabDef<Tab>[] | null = results
    ? [
        { key: "overview", label: "Overview" },
        { key: "suggestions", label: "Suggestions", count: actionable.length },
        { key: "explorer", label: "Explorer" },
        { key: "files", label: "Files" },
        { key: "duplicates", label: "Dupes", count: summary.duplicate_groups },
        { key: "history", label: "History" },
      ]
    : null;
  const go = (t: Tab) => {
    setTab(t);
    setShowSettings(false);
  };

  const right = (
    <>
      {inTrash > 0 && phase.kind !== "scanning" && (
        <button className="chip-trash" onClick={() => go("history")} title="Moved items waiting in .sift-trash">
          {bytes(inTrash)} IN TRASH
        </button>
      )}
      {admin ? (
        <span className="chip-admin" title="Running as administrator — system folders are included">
          ADMIN
        </span>
      ) : (
        <button className="top-btn" onClick={runAsAdmin} aria-label="Run as administrator" title="Restart Sift as administrator to read system folders">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="square" aria-hidden>
            <path d="M12 3l8 3v6c0 4.5-3.4 8-8 9-4.6-1-8-4.5-8-9V6z" />
          </svg>
        </button>
      )}
      {phase.kind !== "scanning" && (
        <>
          <button className={`top-btn ${showSettings ? "on" : ""}`} onClick={() => setShowSettings(!showSettings)} aria-label="Settings" title="Settings">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
              <circle cx="12" cy="12" r="3" />
            </svg>
            <span className="top-btn-text">Settings</span>
          </button>
          {phase.kind === "results" && (
            <button
              className="top-btn"
              onClick={() => {
                setShowSettings(false);
                setPhase({ kind: "home" });
              }}
            >
              New scan
            </button>
          )}
        </>
      )}
    </>
  );

  return (
    <div className="shell">
      <Topbar tabs={showSettings ? null : tabs} tab={showSettings ? null : tab} onTab={go} right={right} windowControls={settings?.custom_titlebar ?? true} />
      <main className="main">
        {showSettings && settings && phase.kind !== "scanning" && (
          <div className="results view" key="settings">
            <div className="page-head">
              <h1 className="page-title">Settings</h1>
            </div>
            <SettingsView settings={settings} onSaved={setSettings} hasPalette={palette !== null} />
          </div>
        )}
        {!showSettings && phase.kind === "home" && <Home onScan={startScan} error={error} admin={admin} onAdmin={runAsAdmin} />}
        {phase.kind === "scanning" && <Scanning root={phase.root} progress={progress} onCancel={() => api.cancelScan()} />}
        {!showSettings && results && (
          <div key={`${scanId.current}-${tab}`} className="results view">
            {tab !== "overview" && (
              <div className="page-head">
                <h1 className="page-title">{TITLES[tab]}</h1>
              </div>
            )}
            {stale && (
              <div className="notice stale">
                Restored items aren't in these results yet.
                <button className="link" onClick={() => startScan(lastRoot.current ?? summary.root_path)}>
                  Rescan
                </button>
              </div>
            )}
            {tab === "overview" && (
              <Overview
                summary={summary}
                suggestions={suggestions}
                freed={freed}
                inTrash={inTrash}
                background={settings?.background_scan ?? false}
                openSettings={() => setShowSettings(true)}
                goSuggestions={() => go("suggestions")}
                admin={admin}
                onAdmin={runAsAdmin}
              />
            )}
            {tab === "suggestions" && <Suggestions suggestions={suggestions} onOpenDuplicates={() => go("duplicates")} />}
            {tab === "explorer" && <Explorer version={version} />}
            {tab === "files" && <LargeFiles version={version} />}
            {tab === "duplicates" && <Duplicates version={version} />}
            {tab === "history" && <History version={version} />}
          </div>
        )}
      </main>
    </div>
  );
}
