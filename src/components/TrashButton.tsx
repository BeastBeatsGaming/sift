import { useState } from "react";
import { api } from "../api";
import { bytes } from "../format";
import { useFeedback } from "./feedback";

/** Small row button: move one path to .sift-trash after confirming. */
export function TrashButton({ path, size, label }: { path: string; size: number; label: string }) {
  const fb = useFeedback();
  const [busy, setBusy] = useState(false);
  return (
    <button
      className="icon-btn trash-btn"
      title="Move to .sift-trash"
      disabled={busy}
      onClick={async (e) => {
        e.stopPropagation();
        const ok = await fb.confirm({
          title: `Move to .sift-trash?`,
          body: (
            <>
              <p className="path wrap">{path}</p>
              <p className="muted">
                {bytes(size)} · You can undo this from History. The space is freed when the trash is emptied.
              </p>
            </>
          ),
          confirm: "Move to trash",
        });
        if (!ok) return;
        setBusy(true);
        try {
          fb.report(await api.quarantinePaths([path], label));
        } catch (err) {
          fb.toast({ text: String(err), tone: "alert" });
        } finally {
          setBusy(false);
        }
      }}
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14M10 11v6M14 11v6" />
      </svg>
    </button>
  );
}
