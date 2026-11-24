import { useCallback, useState } from "react";
import { api } from "../api";
import { bytes, count } from "../format";
import { useFeedback } from "./feedback";

/** Confirm, then move paths to .sift-trash and report the result. */
export function useTrash() {
  const fb = useFeedback();
  return useCallback(
    async (paths: string[], size: number, label: string): Promise<boolean> => {
      if (paths.length === 0) return false;
      const one = paths.length === 1;
      // OneDrive removes a file from the cloud when it's moved out of its folder
      const synced = await api.syncedAmong(paths).catch(() => [] as string[]);
      const ok = await fb.confirm({
        title: one ? "Move to .sift-trash?" : `Move ${count(paths.length)} items to .sift-trash?`,
        body: (
          <>
            {one ? <p className="path wrap">{paths[0]}</p> : <p>{label}</p>}
            <p className="muted">
              {bytes(size)} · You can undo this from History. The space is freed when the trash is emptied.
            </p>
            {synced.length > 0 && (
              <p className="warn-text">
                {synced.length === paths.length ? (one ? "This file is" : `All ${count(synced.length)} items are`) : `${count(synced.length)} of these are`} in
                OneDrive. Moving {synced.length === 1 ? "it" : "them"} out deletes {synced.length === 1 ? "it" : "them"} from OneDrive too — on the web and
                your other devices — until Undo puts {synced.length === 1 ? "it" : "them"} back, which uploads {synced.length === 1 ? "it" : "them"} again.
              </p>
            )}
          </>
        ),
        confirm: synced.length > 0 ? "Move and delete from OneDrive" : "Move to trash",
        danger: synced.length > 0,
      });
      if (!ok) return false;
      try {
        fb.report(await api.quarantinePaths(paths, label, synced.length > 0));
        return true;
      } catch (err) {
        fb.toast({ text: String(err), tone: "alert" });
        return false;
      }
    },
    [fb],
  );
}

/** Small row button: move one path to .sift-trash after confirming. */
export function TrashButton({ path, size, label }: { path: string; size: number; label: string }) {
  const trash = useTrash();
  const [busy, setBusy] = useState(false);
  return (
    <button
      className="icon-btn trash-btn"
      title="Move to .sift-trash"
      disabled={busy}
      onClick={async (e) => {
        e.stopPropagation();
        setBusy(true);
        await trash([path], size, label);
        setBusy(false);
      }}
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d="M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14M10 11v6M14 11v6" />
      </svg>
    </button>
  );
}
