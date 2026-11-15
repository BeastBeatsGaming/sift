import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Logo } from "./Logo";

export interface TabDef<K extends string> {
  key: K;
  label: string;
  count?: number;
}

const win = () => getCurrentWindow();

/** One bar for everything: logo, view tabs, status and (when the custom
 *  title bar is on) window controls. Empty space drags the window. */
export function Topbar<K extends string>({
  tabs,
  tab,
  onTab,
  right,
  windowControls,
}: {
  tabs: TabDef<K>[] | null;
  tab: K | null;
  onTab: (k: K) => void;
  right: ReactNode;
  windowControls: boolean;
}) {
  const refs = useRef<Partial<Record<K, HTMLButtonElement | null>>>({});
  const [ink, setInk] = useState<{ left: number; width: number } | null>(null);

  useLayoutEffect(() => {
    const measure = () => {
      const el = tab ? refs.current[tab] : null;
      setInk(el ? { left: el.offsetLeft, width: el.offsetWidth } : null);
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [tab, tabs?.length]);

  const drag = windowControls ? { "data-tauri-drag-region": true } : {};

  return (
    <header className="topbar" {...drag}>
      <span className="brand" {...drag}>
        <Logo size={20} />
        <span className="brand-word">SIFT</span>
      </span>
      {tabs && (
        <nav className="tabs">
          {tabs.map((t) => (
            <button key={t.key} ref={(el) => void (refs.current[t.key] = el)} className={`tab ${tab === t.key ? "on" : ""}`} onClick={() => onTab(t.key)}>
              {t.label}
              {t.count ? <span className="tab-count">{t.count}</span> : null}
            </button>
          ))}
          {ink && <span className="tab-ink" style={{ left: ink.left, width: ink.width }} />}
        </nav>
      )}
      <span className="grow" {...drag} />
      <div className="top-right">{right}</div>
      {windowControls && (
        <div className="win-btns">
          <button className="win-btn" onClick={() => win().minimize()} aria-label="Minimize">
            <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 5h8" stroke="currentColor" /></svg>
          </button>
          <button className="win-btn" onClick={() => win().toggleMaximize()} aria-label="Maximize">
            <svg width="10" height="10" viewBox="0 0 10 10"><rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" /></svg>
          </button>
          <button className="win-btn win-close" onClick={() => win().close()} aria-label="Close">
            <svg width="10" height="10" viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" /></svg>
          </button>
        </div>
      )}
    </header>
  );
}

