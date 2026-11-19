import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Safety = "Safe" | "Review" | "TradeOff" | "Leave";

export type Action =
  | { kind: "quarantine"; contents: boolean }
  | { kind: "command"; program: string; args: string[] }
  | { kind: "open_settings"; uri: string }
  | { kind: "manual" };

export interface Located { path: string; bytes: number }

export interface Suggestion {
  id: string;
  title: string;
  bytes: number;
  safety: Safety;
  how: string;
  action: Action;
  /** Can't be undone from Sift (command, Windows Settings, emptying a bin). */
  permanent: boolean;
  items: Located[];
  item_count: number;
}

export interface Drive { path: string; name: string; total: number; free: number }

export interface ScanStats { files: number; dirs: number; errors: number; cloud_skipped: number; elapsed_ms: number }

export interface Summary {
  root_path: string;
  /** Unix seconds; an old value means the saved last scan was loaded. */
  scanned_at: number;
  total: number;
  stats: ScanStats;
  categories: [string, number][];
  disk_total: number | null;
  disk_free: number | null;
  duplicate_groups: number;
}

export interface ChildView {
  index: number;
  name: string;
  size: number;
  file_count: number;
  regen_rule: string | null;
  has_children: boolean;
  denied: boolean;
}

export interface DirView {
  full_path: string;
  crumbs: string[];
  size: number;
  own_bytes: number;
  file_count: number;
  regen_rule: string | null;
  children: ChildView[];
  child_count: number;
}

export interface FileEntry { path: string; size: number; modified: number }
export interface DuplicateGroup { size: number; wasted: number; paths: string[]; modified: number[] }

export interface ScanProgress { files: number; dirs: number; bytes: number; current: string; phase: "scanning" | "analyzing" }

export interface Skip { path: string; reason: string }

export interface CleanResult {
  kind: "quarantine" | "command" | "settings" | "restore" | "purge";
  batch_id: string | null;
  done: number;
  bytes: number;
  skipped: Skip[];
  skipped_count: number;
  exit_code: number | null;
  output: string | null;
}

export type EntryState = "in_trash" | "restored" | "purged";

export interface BatchView {
  id: string;
  created: number;
  label: string;
  kind: "quarantine" | "command";
  in_trash: number;
  total: number;
  entry_count: number;
  /** Space this cleanup took off the drive (not counting restored items). */
  freed: number;
  restored: number;
  purged: number;
  entries: { original: string; bytes: number; state: EntryState }[];
  command: string | null;
  exit_code: number | null;
  output: string | null;
}

export interface HistoryView { in_trash: number; auto_purge_days: number; batches: BatchView[]; freed: { created: number; freed: number }[] }

export interface Settings {
  auto_purge_days: number;
  min_suggestion_mb: number;
  stale_days: number;
  downloads_days: number;
  find_duplicates: boolean;
  duplicate_min_mb: number;
  ignored: string[];
  rice_theme: boolean;
  custom_titlebar: boolean;
  background_scan: boolean;
  scan_every_days: number;
  /** "" = the Windows drive */
  scan_path: string;
  notify_gb: number;
}

export interface BackgroundStatus {
  last_run: number;
  last_root: string;
  last_safe: number;
  next_run: number | null;
  default_root: string;
  running: boolean;
}

export type Palette = Record<string, string>;

export interface AppInfo { version: string; os: string; repo: string; log_path: string | null }

export interface SearchHit { path: string; size: number; is_dir: boolean; index_path: number[] | null }

export const api = {
  listDrives: () => invoke<Drive[]>("list_drives"),
  startScan: (path: string) => invoke<void>("start_scan", { path }),
  cancelScan: () => invoke<void>("cancel_scan"),
  summary: () => invoke<Summary>("get_summary"),
  suggestions: () => invoke<Suggestion[]>("get_suggestions"),
  dir: (path: number[]) => invoke<DirView>("get_dir", { path }),
  bigFiles: (limit: number) => invoke<FileEntry[]>("get_big_files", { limit }),
  duplicates: () => invoke<DuplicateGroup[]>("get_duplicates"),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  cleanSuggestion: (id: string, paths: string[] | null) => invoke<CleanResult>("clean_suggestion", { id, paths }),
  cleanDuplicates: (picks: { keep: string; remove: string[] }[]) => invoke<CleanResult>("clean_duplicates", { picks }),
  quarantinePaths: (paths: string[], label: string) => invoke<CleanResult>("quarantine_paths", { paths, label }),
  history: () => invoke<HistoryView>("get_history"),
  restoreBatch: (id: string) => invoke<CleanResult>("restore_batch", { id }),
  purgeBatch: (id: string) => invoke<CleanResult>("purge_batch", { id }),
  emptyTrash: () => invoke<CleanResult>("empty_trash"),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  ignorePath: (path: string) => invoke<void>("ignore_path", { path }),
  isAdmin: () => invoke<boolean>("is_admin"),
  relaunchAsAdmin: () => invoke<void>("relaunch_as_admin"),
  backgroundStatus: () => invoke<BackgroundStatus>("get_background_status"),
  runBackgroundScan: () => invoke<void>("run_background_scan_now"),
  palette: () => invoke<Palette | null>("get_palette"),
  search: (query: string, limit = 200) => invoke<SearchHit[]>("search", { query, limit }),
  appInfo: () => invoke<AppInfo>("get_app_info"),
  diagnostics: () => invoke<string>("get_diagnostics"),
  logUi: (level: "warn" | "error", message: string) => invoke<void>("log_ui", { level, message }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};

export function onThemeChanged(h: (p: Palette | null) => void): Promise<UnlistenFn> {
  return listen<Palette | null>("theme://changed", (e) => h(e.payload));
}

export function onDataEvents(h: { changed: () => void; stale: () => void }): Promise<UnlistenFn> {
  return Promise.all([listen("data://changed", () => h.changed()), listen("data://stale", () => h.stale())]).then(
    (fns) => () => fns.forEach((f) => f()),
  );
}

export function onScanEvents(h: {
  progress: (p: ScanProgress) => void;
  done: () => void;
  cancelled: () => void;
}): Promise<UnlistenFn> {
  const subs = [
    listen<ScanProgress>("scan://progress", (e) => h.progress(e.payload)),
    listen("scan://done", () => h.done()),
    listen("scan://cancelled", () => h.cancelled()),
  ];
  return Promise.all(subs).then((fns) => () => fns.forEach((f) => f()));
}

export const SAFETY_LABEL: Record<Safety, string> = {
  Safe: "Safe",
  Review: "Review",
  TradeOff: "Trade-off",
  Leave: "Leave",
};
