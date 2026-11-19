import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, onDataEvents, onScanEvents, onThemeChanged, type AppInfo, type Palette, type ScanProgress, type Settings, type Suggestion, type Summary } from "./api";
import { checkForUpdate, markUpdateSeen, notesPreview, updateSeen, type Release } from "./meta";
import { applyPalette } from "./theme";
import { Topbar, type TabDef } from "./components/Topbar";
import { useFeedback } from "./components/feedback";
import { SettingsView } from "./views/SettingsView";
import { bytes, since } from "./format";
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

const TAB_ORDER: Tab[] = ["overview", "suggestions", "explorer", "files", "duplicates", "history"];
/** Results older than this get a "from your last scan" note. */
const OLD_SCAN_SECS = 30 * 60;

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
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [update, setUpdate] = useState<Release | null>(null);

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
    api.isAdmin().then(setAdmin).catch(() => {});
    // reopened from the tray, or a background scan is loaded: show it
    Promise.all([api.summary(), api.suggestions()])
      .then(([s, sg]) => showResults(s, sg))
      .catch(() => {});
    // listen first, then read, so a change in between isn't missed
    const off = onThemeChanged((p) => p && setPalette(p));
    const refreshPalette = () => api.palette().then((p) => p && setPalette(p)).catch(() => {});
    refreshPalette();
    // started hidden with Windows: the webview can sit throttled in the tray,
    // so re-read whenever the window comes back
    const onVisible = () => document.visibilityState === "visible" && refreshPalette();
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", refreshPalette);
    return () => {
      off.then((f) => f());
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", refreshPalette);
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

  function showUpdate(rel: Release, current: string) {
    markUpdateSeen(rel.version);
    const notes = notesPreview(rel.notes);
    fb.confirm({
      title: `Sift ${rel.version} is out`,
      body: (
        <>
          <p>
            You're on {current}. Download the new installer from GitHub and run it — your settings, history and .sift-trash stay as they are.
          </p>
          {notes.length > 0 && (
            <div className="release-notes">
              {notes.map((l, i) => (
                <span key={i}>{l}</span>
              ))}
            </div>
          )}
        </>
      ),
      confirm: "Get the update",
    }).then((ok) => {
      if (ok) api.openUrl(rel.url).catch((e) => fb.toast({ text: String(e), tone: "warn" }));
    });
  }

  async function checkUpdate(current: string, manual = false) {
    try {
      const rel = await checkForUpdate(current, manual);
      setUpdate(rel);
      if (rel && (manual || !updateSeen(rel.version))) showUpdate(rel, current);
      else if (manual) fb.toast({ text: `You're on the latest version (${current})` });
    } catch (e) {
      if (manual) fb.toast({ text: "Couldn't check for updates", detail: [String(e)], tone: "warn" });
    }
  }

  // check on start (cached for a few hours) and again while Sift stays open
  useEffect(() => {
    let timer = 0;
    api
      .appInfo()
      .then((i) => {
        setInfo(i);
        // let the first paint settle; a popup on top of the splash feels rude
        setTimeout(() => checkUpdate(i.version), 2500);
        timer = window.setInterval(() => checkUpdate(i.version), 6 * 60 * 60 * 1000);
      })
      .catch(() => {});
    return () => clearInterval(timer);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

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

  // keyboard: Ctrl+R rescan, Ctrl+F search, Ctrl+1…6 tabs, Ctrl+, settings,
  // Esc closes settings. Also stops the webview's own reload / find.
  const keys = useRef<(e: KeyboardEvent) => void>(() => {});
  keys.current = (e: KeyboardEvent) => {
    const ctrl = e.ctrlKey || e.metaKey;
    const k = e.key.toLowerCase();
    if (k === "f5" || (ctrl && k === "r")) {
      e.preventDefault();
      if (phase.kind === "results" && summary) startScan(lastRoot.current ?? summary.root_path);
      return;
    }
    if (ctrl && k === "f") {
      e.preventDefault();
      if (phase.kind !== "results") return;
      const focus = () => document.querySelector<HTMLInputElement>("input[data-search]")?.focus();
      if (!showSettings && (tab === "explorer" || tab === "files")) focus();
      else {
        go("explorer");
        setTimeout(focus, 60);
      }
      return;
    }
    if (ctrl && k >= "1" && k <= "6" && phase.kind === "results") {
      e.preventDefault();
      go(TAB_ORDER[Number(k) - 1]);
      return;
    }
    if (ctrl && k === "," && phase.kind !== "scanning") {
      e.preventDefault();
      setShowSettings((v) => !v);
      return;
    }
    if (k === "escape" && showSettings && !document.querySelector(".modal-back, .ctx-menu")) {
      setShowSettings(false);
    }
  };
  useEffect(() => {
    const h = (e: KeyboardEvent) => keys.current(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const right = (
    <>
      {update && info && (
        <button className="chip-update" onClick={() => showUpdate(update, info.version)} title={`Sift ${update.version} is available — you're on ${info.version}`}>
          <span className="long">UPDATE TO v{update.version}</span>
          <span className="short">UPDATE</span>
        </button>
      )}
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
            <SettingsView
              settings={settings}
              onSaved={setSettings}
              hasPalette={palette !== null}
              info={info}
              update={update}
              onCheckUpdate={() => info && checkUpdate(info.version, true)}
            />
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
            {tab === "overview" && summary.scanned_at > 0 && Date.now() / 1000 - summary.scanned_at > OLD_SCAN_SECS && (
              <div className="notice last-scan">
                <span>
                  Showing your last scan of <span className="mono">{summary.root_path}</span> from {since(summary.scanned_at)}.
                </span>
                <span className="grow" />
                <button className="link" onClick={() => startScan(lastRoot.current ?? summary.root_path)}>
                  Rescan · Ctrl+R
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
