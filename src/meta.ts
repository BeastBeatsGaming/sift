// App-level plumbing: update check, bug reports, clipboard, error logging.
import { api, type AppInfo } from "./api";

export const REPO = "https://github.com/BeastBeatsGaming/sift";
const LATEST_API = "https://api.github.com/repos/BeastBeatsGaming/sift/releases/latest";
/** GitHub allows 60 unauthenticated calls an hour per IP: check rarely. */
const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;
const CACHE_KEY = "sift.update";
const SEEN_KEY = "sift.update.seen";

export interface Release {
  version: string;
  url: string;
  name: string;
  notes: string;
  published: string;
}

/** Numeric compare of "1.2.3" style versions; pre-release tags are ignored. */
export function newer(a: string, b: string): boolean {
  const parse = (v: string) =>
    v
      .replace(/^v/i, "")
      .split(/[-+]/)[0]
      .split(".")
      .map((n) => parseInt(n, 10) || 0);
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d > 0;
  }
  return false;
}

function store<T>(key: string, value?: T): T | null {
  try {
    if (value === undefined) {
      const raw = localStorage.getItem(key);
      return raw ? (JSON.parse(raw) as T) : null;
    }
    localStorage.setItem(key, JSON.stringify(value));
    return value;
  } catch {
    return null;
  }
}

/** Latest published (non-draft, non-pre-release) GitHub release, cached for
 *  a few hours. Returns it only when it's newer than `current`. */
export async function checkForUpdate(current: string, force = false): Promise<Release | null> {
  let rel: Release | null = null;
  const cached = store<{ at: number; rel: Release }>(CACHE_KEY);
  if (!force && cached && Date.now() - cached.at < CHECK_EVERY_MS) {
    rel = cached.rel;
  } else {
    const res = await fetch(LATEST_API, { headers: { Accept: "application/vnd.github+json" } });
    if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
    const j = await res.json();
    rel = {
      version: String(j.tag_name ?? "").replace(/^v/i, ""),
      url: String(j.html_url ?? `${REPO}/releases/latest`),
      name: String(j.name ?? j.tag_name ?? ""),
      notes: String(j.body ?? ""),
      published: String(j.published_at ?? ""),
    };
    store(CACHE_KEY, { at: Date.now(), rel });
  }
  return rel && rel.version && newer(rel.version, current) ? rel : null;
}

/** The popup shows once per version; the top-bar flag stays until updated. */
export const updateSeen = (v: string) => store<string>(SEEN_KEY) === v;
export const markUpdateSeen = (v: string) => void store(SEEN_KEY, v);

/** First lines of the release notes, as plain text, for the popup. */
export function notesPreview(md: string, lines = 6): string[] {
  return md
    .split(/\r?\n/)
    .map((l) => l.replace(/^#+\s*/, "").replace(/\*\*/g, "").trim())
    .filter((l) => l && !/^sha-?256/i.test(l) && !/^[a-f0-9]{64}$/i.test(l) && !l.startsWith("```"))
    .slice(0, lines);
}

export function bugReportUrl(info: AppInfo | null): string {
  const body = [
    "**What happened?**",
    "",
    "",
    "**What did you expect?**",
    "",
    "",
    "**Steps to reproduce**",
    "1. ",
    "",
    "---",
    `Sift ${info?.version ?? "?"} · ${info?.os ?? "Windows"}`,
    "<!-- Settings → About → Copy diagnostics, then paste it here if you can. -->",
  ].join("\n");
  return `${REPO}/issues/new?labels=bug&title=${encodeURIComponent("Bug: ")}&body=${encodeURIComponent(body)}`;
}

export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // not a secure context in some webviews: fall back to the old way
  }
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  const ok = document.execCommand("copy");
  ta.remove();
  if (!ok) throw new Error("Couldn't copy to the clipboard");
}

/** Send uncaught webview errors to the log file. */
export function installErrorLogging() {
  const send = (msg: string) => api.logUi("error", msg).catch(() => {});
  window.addEventListener("error", (e) => send(`${e.message} @ ${e.filename}:${e.lineno}`));
  window.addEventListener("unhandledrejection", (e) => send(`unhandled: ${String(e.reason?.stack ?? e.reason)}`));
}
