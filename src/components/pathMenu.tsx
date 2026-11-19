import type { MouseEvent } from "react";
import { api } from "../api";
import { copyText } from "../meta";
import { useFeedback } from "./feedback";
import { useContextMenu, type MenuItem } from "./menu";
import { useTrash } from "./TrashButton";

/** Right-click menu for any file or folder row. */
export function usePathMenu() {
  const menu = useContextMenu();
  const trash = useTrash();
  const fb = useFeedback();
  return (e: MouseEvent, p: { path: string; size: number; label: string; before?: MenuItem[] }) =>
    menu(e, [
      ...(p.before ?? []),
      { label: "Show in Explorer", onClick: () => api.reveal(p.path).catch((x) => fb.toast({ text: String(x), tone: "warn" })) },
      {
        label: "Copy path",
        onClick: () =>
          copyText(p.path)
            .then(() => fb.toast({ text: "Path copied" }))
            .catch((x) => fb.toast({ text: String(x), tone: "warn" })),
      },
      {
        label: "Never suggest this",
        onClick: () =>
          api
            .ignorePath(p.path)
            .then(() => fb.toast({ text: `Ignoring ${p.path}`, detail: ["Undo in Settings → Ignored paths"] }))
            .catch((x) => fb.toast({ text: String(x), tone: "warn" })),
      },
      { label: "Move to .sift-trash…", danger: true, onClick: () => void trash([p.path], p.size, p.label) },
    ]);
}
