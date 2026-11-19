import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useRef, useState, type ReactNode, type MouseEvent as ReactMouseEvent } from "react";

export interface MenuItem {
  label: string;
  onClick: () => void;
  danger?: boolean;
  disabled?: boolean;
}

type Open = (e: ReactMouseEvent, items: (MenuItem | null | false)[]) => void;

const Ctx = createContext<Open>(() => {});
/** Right-click menu: `onContextMenu={(e) => menu(e, [...items])}`. */
export const useContextMenu = () => useContext(Ctx);

export function ContextMenuProvider({ children }: { children: ReactNode }) {
  const [m, setM] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  const open: Open = useCallback((e, items) => {
    e.preventDefault();
    e.stopPropagation();
    const list = items.filter(Boolean) as MenuItem[];
    if (list.length) setM({ x: e.clientX, y: e.clientY, items: list });
  }, []);

  // keep the menu on screen
  useLayoutEffect(() => {
    if (!m || !ref.current) return setPos(null);
    const r = ref.current.getBoundingClientRect();
    setPos({ left: Math.min(m.x, window.innerWidth - r.width - 6), top: Math.min(m.y, window.innerHeight - r.height - 6) });
  }, [m]);

  useEffect(() => {
    if (!m) return;
    const close = () => setM(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && (e.stopPropagation(), close());
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    window.addEventListener("wheel", close, { passive: true });
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("wheel", close);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [m]);

  // The webview's own menu (Back / Refresh / Inspect) means nothing here;
  // keep it only where text can be edited, for copy and paste.
  useEffect(() => {
    const onCtx = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (!t?.closest("input, textarea, [contenteditable]")) e.preventDefault();
    };
    window.addEventListener("contextmenu", onCtx);
    return () => window.removeEventListener("contextmenu", onCtx);
  }, []);

  return (
    <Ctx.Provider value={open}>
      {children}
      {m && (
        <div
          ref={ref}
          className="ctx-menu"
          role="menu"
          style={{ left: pos?.left ?? m.x, top: pos?.top ?? m.y, visibility: pos ? "visible" : "hidden" }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {m.items.map((it, i) => (
            <button
              key={i}
              role="menuitem"
              className={`ctx-item ${it.danger ? "danger-text" : ""}`}
              disabled={it.disabled}
              onClick={() => {
                setM(null);
                it.onClick();
              }}
            >
              {it.label}
            </button>
          ))}
        </div>
      )}
    </Ctx.Provider>
  );
}
