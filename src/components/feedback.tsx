import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, type CleanResult } from "../api";
import { bytes, count } from "../format";

// ---------------------------------------------------------------- confirm

export interface ConfirmOptions {
  title: string;
  body: ReactNode;
  confirm: string;
  danger?: boolean;
}

// ------------------------------------------------------------------ toast

interface Toast {
  id: number;
  text: string;
  detail?: string[];
  undoBatch?: string;
  tone?: "ok" | "warn" | "alert";
}

interface Feedback {
  confirm: (o: ConfirmOptions) => Promise<boolean>;
  toast: (t: Omit<Toast, "id">) => void;
  /** Turn a cleanup result into a toast (with Undo when possible). */
  report: (r: CleanResult) => void;
}

const Ctx = createContext<Feedback | null>(null);
export const useFeedback = () => useContext(Ctx)!;

export function FeedbackProvider({ children }: { children: ReactNode }) {
  const [dialog, setDialog] = useState<(ConfirmOptions & { resolve: (v: boolean) => void }) | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);

  const confirm = useCallback((o: ConfirmOptions) => new Promise<boolean>((resolve) => setDialog({ ...o, resolve })), []);

  const dismiss = useCallback((id: number) => setToasts((ts) => ts.filter((t) => t.id !== id)), []);

  const toast = useCallback(
    (t: Omit<Toast, "id">) => {
      const id = nextId.current++;
      setToasts((ts) => [...ts.slice(-2), { ...t, id }]);
      setTimeout(() => dismiss(id), t.undoBatch ? 12000 : 7000);
    },
    [dismiss],
  );

  const report = useCallback(
    (r: CleanResult) => {
      const detail = r.skipped.slice(0, 4).map((s) => `${s.reason}: ${s.path}`);
      if (r.skipped_count > 4) detail.push(`+ ${count(r.skipped_count - 4)} more skipped`);
      const skippedNote = r.skipped_count ? ` · ${count(r.skipped_count)} skipped` : "";
      switch (r.kind) {
        case "quarantine":
          if (r.done === 0) return toast({ text: `Nothing moved${skippedNote}`, detail, tone: "warn" });
          return toast({
            text: `Moved ${count(r.done)} item${r.done === 1 ? "" : "s"} · ${bytes(r.bytes)} to .sift-trash${skippedNote}`,
            detail,
            undoBatch: r.batch_id ?? undefined,
            tone: r.skipped_count ? "warn" : "ok",
          });
        case "command":
          return toast({
            text: r.exit_code === 0 ? `Done — freed ${bytes(r.bytes)}` : `Command failed (exit ${r.exit_code ?? "?"})`,
            detail: r.output ? [r.output.split("\n").slice(-3).join("\n")] : undefined,
            tone: r.exit_code === 0 ? "ok" : "alert",
          });
        case "restore":
          return toast({ text: `Restored ${count(r.done)} item${r.done === 1 ? "" : "s"}${skippedNote}`, detail, tone: r.skipped_count ? "warn" : "ok" });
        case "purge":
          // purged batches can't be undone any more: drop stale Undo toasts
          setToasts((ts) => ts.filter((t) => !t.undoBatch));
          return toast({ text: `Freed ${bytes(r.bytes)} permanently${skippedNote}`, detail, tone: r.skipped_count ? "warn" : "ok" });
        case "settings":
          return toast({ text: "Opened Windows Settings" });
      }
    },
    [toast],
  );

  const value = useMemo(() => ({ confirm, toast, report }), [confirm, toast, report]);

  async function undo(t: Toast) {
    dismiss(t.id);
    try {
      report(await api.restoreBatch(t.undoBatch!));
    } catch (e) {
      toast({ text: String(e), tone: "alert" });
    }
  }

  return (
    <Ctx.Provider value={value}>
      {children}
      {dialog && <ConfirmDialog {...dialog} close={(v) => (dialog.resolve(v), setDialog(null))} />}
      <div className="toasts">
        {toasts.map((t) => (
          <div key={t.id} className={`toast toast-${t.tone ?? "ok"}`}>
            <div className="toast-main">
              <span>{t.text}</span>
              {t.undoBatch && (
                <button className="link" onClick={() => undo(t)}>
                  Undo
                </button>
              )}
              <button className="icon-btn" onClick={() => dismiss(t.id)} aria-label="Dismiss">
                ×
              </button>
            </div>
            {t.detail?.map((d, i) => (
              <div key={i} className="toast-detail">
                {d}
              </div>
            ))}
          </div>
        ))}
      </div>
    </Ctx.Provider>
  );
}

function ConfirmDialog({ title, body, confirm, danger, close }: ConfirmOptions & { close: (v: boolean) => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);
  return (
    <div className="modal-back" onClick={() => close(false)}>
      <div className={`modal ${danger ? "modal-danger" : ""}`} onClick={(e) => e.stopPropagation()} role="dialog" aria-modal>
        <h3 className="modal-title">{title}</h3>
        <div className="modal-body">{body}</div>
        <div className="modal-actions">
          <button className="btn btn-ghost" onClick={() => close(false)}>
            Cancel
          </button>
          <button className={`btn ${danger ? "btn-danger" : ""}`} onClick={() => close(true)} autoFocus>
            {confirm}
          </button>
        </div>
      </div>
    </div>
  );
}
