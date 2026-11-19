export function bytes(n: number, digits = 1): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return i === 0 ? `${v} B` : `${v.toFixed(v >= 100 ? 0 : digits)} ${units[i]}`;
}

export const count = (n: number) => n.toLocaleString("en-IN");

export const pct = (part: number, whole: number) => (whole > 0 ? (part / whole) * 100 : 0);

export function ago(unixSecs: number): string {
  if (!unixSecs) return "—";
  const days = (Date.now() / 1000 - unixSecs) / 86400;
  if (days < 1) return "today";
  if (days < 30) return `${Math.floor(days)} d ago`;
  if (days < 365) return `${Math.floor(days / 30)} mo ago`;
  return `${(days / 365).toFixed(1)} y ago`;
}

export function basename(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}

export function dirname(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  const d = i > 0 ? p.slice(0, i) : p;
  return /^[A-Za-z]:$/.test(d) ? d + "\\" : d;
}

/** "just now", "12 min ago", "3 h ago", "2 d ago". */
export function since(unixSecs: number): string {
  if (!unixSecs) return "—";
  const s = Math.max(0, Date.now() / 1000 - unixSecs);
  if (s < 90) return "just now";
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400 * 2) return `${Math.round(s / 3600)} h ago`;
  return `${Math.round(s / 86400)} d ago`;
}

/** File type groups, matching the scanner's categories (rules.rs). */
export function category(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? "";
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  const groups: [string, string[]][] = [
    ["Video", ["mp4", "mkv", "avi", "mov", "webm", "m4v", "wmv", "flv", "ts"]],
    ["Audio", ["mp3", "wav", "flac", "aac", "ogg", "m4a", "opus"]],
    ["Images & design", ["jpg", "jpeg", "png", "gif", "psd", "tif", "tiff", "raw", "cr2", "nef", "arw", "heic", "webp", "exr", "hdr", "kra", "ai"]],
    ["Archives", ["zip", "rar", "7z", "tar", "gz", "tgz", "xz", "bz2", "zst"]],
    ["Installers & disk images", ["exe", "msi", "iso", "img", "msix", "msixbundle", "appx", "dmg"]],
    ["Virtual disks", ["vhdx", "vhd", "vmdk", "qcow2", "vdi"]],
    ["3D & game assets", ["blend", "blend1", "blend2", "fbx", "obj", "glb", "gltf", "unitypackage", "uasset", "umap", "pak", "abc", "usd", "usdz"]],
    ["Documents", ["pdf", "docx", "doc", "pptx", "xlsx", "epub", "txt", "md"]],
    ["Dev binaries", ["dll", "pdb", "lib", "so", "a", "jar", "whl", "node", "pyd", "ilk", "pch"]],
    ["Logs, dumps & temp", ["log", "dmp", "tmp", "etl", "bak", "old"]],
  ];
  return groups.find(([, exts]) => exts.includes(ext))?.[0] ?? "Other";
}
