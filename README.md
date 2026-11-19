# Sift

Disk analyzer and cleanup tool for Windows. Find out what's eating your drive, clear the safe stuff in one click, and undo anything you change your mind about.

> **Status: v0.2.0 — test build.** Expect rough edges. Report anything odd from **Settings → About → Report a bug**, or in [Issues](../../issues).

## Install
1. Download `Sift_0.2.0_x64-setup.exe` from the [latest release](../../releases/latest).
2. Run it. Windows SmartScreen will say *"Windows protected your PC"* because the installer isn't code-signed yet. Click **More info → Run anyway**.
3. It installs for your user only (no admin needed). Uninstall from Settings → Apps.

**Updating:** Sift checks GitHub for new releases and shows an **UPDATE TO vX.Y.Z** flag in the top bar when one is out. Download the new installer and run it over the old one; settings, history and `.sift-trash` are kept.

Requires Windows 10 or 11 (64-bit).

## What it does
- **Overview** — drive usage at a glance, plus how much Sift has freed for you over time.
- **Suggestions** — caches, build folders (node_modules, Rust `target/`, Unity/Unreal caches…), temp files and more, grouped by risk. Anything that can't be undone carries a red **NO UNDO** flag.
- **Explorer** — treemap of every folder by size, with search across the whole scan.
- **Large files** and **Duplicates** — filter large files by type and age and move several at once; duplicates are confirmed by a full-file hash before anything moves, and you choose which copy to keep.
- Right-click any file or folder to show it in Explorer, copy its path, ignore it or move it to the trash. Shortcuts: `Ctrl+R` rescan, `Ctrl+F` search, `Ctrl+1…6` tabs, `Ctrl+,` settings.
- Sift remembers your last scan, so it opens straight to your results.
- **History** — every cleanup, with Undo.
- **Background scan** (optional, off by default) — runs from the tray and notifies you when there's a lot to reclaim.

## Is it safe?
Cleanups don't delete anything right away. Files are moved to a hidden `.sift-trash` folder on the same drive, so **Undo** restores them exactly. Space is actually freed when you clear a batch from History, or automatically after 14 days. Sift refuses to touch drive roots, Windows, Program Files, your profile folders and system files like the page file.

A few suggestions are carried out by another tool instead — a cache command (`npm cache clean`, `docker system prune`…), Windows Settings, or emptying the Recycle Bin. Those skip `.sift-trash` and **can't be undone**, so they're marked **NO UNDO** in red and ask before running. Docker cleanup keeps your volumes.

Run as administrator (from the top bar) to include Windows system folders in the scan.

---

## Development
Tauri 2 + React UI, Rust scanner.
### Layout
- `crates/sift-core/` — scanner, cleanup rules (`rules.rs`), suggestions, duplicates. No UI code.
- `src-tauri/` — the app window; `commands.rs` is everything the UI can call.
- `src/` — React UI (`views/` per screen, `components/Treemap.tsx`).

### Run
```
npm install
npm run tauri dev            # dev mode with hot reload
npm run tauri build          # installer: target/release/bundle/nsis/Sift_0.2.0_x64-setup.exe
cargo test -p sift-core      # engine tests
cargo run --release -p sift-core --example scan -- C:\   # engine only, in a terminal
```
Run as administrator to include Windows system folders.

Dev shortcut: `VITE_SIFT_AUTOSCAN=D:\some\folder npm run tauri dev` scans that folder on launch.

### Cleanup and undo
Cleanups move items into `<drive>:\.sift-trash\<batch>\` (hidden) on the same drive —
an instant rename, so Undo restores them exactly. Each batch folder holds a
`manifest.json` listing original paths. Space is freed when a batch is deleted
from History, or automatically after 14 days (`AUTO_PURGE_DAYS` in
`src-tauri/src/cleanup.rs`). Commands (`npm cache clean` etc.) are logged in
History but can't be undone. `crates/sift-core/src/protect.rs` lists what Sift
refuses to move (drive roots, Windows/Program Files/profile folders, page and
hibernation files).

### Adding a cleanup rule
Add a line to `REGEN_RULES` (folders tools rebuild), `KNOWN_LOCATIONS`
(fixed cache paths, `%VAR%` and `*` allowed) or `SPECIAL_EXTS` in
`crates/sift-core/src/rules.rs`.

### Settings, saved scan and logs
Stored in `%APPDATA%\dev.saimayank.sift\settings.json` (auto-empty days,
suggestion thresholds, ignored paths, rice theme, title bar). History lives
next to it in `history.json`. The last scan is saved to
`%LOCALAPPDATA%\dev.saimayank.sift\last-scan.json` (bump `FORMAT` in
`crates/sift-core/src/snapshot.rs` when the saved shape changes) and the log to
`%LOCALAPPDATA%\dev.saimayank.sift\logs\sift.log` (rotated at 1 MB).

### Releasing
Bump the version in `package.json`, `src-tauri/tauri.conf.json` and both
`Cargo.toml` files, build, then publish a GitHub release tagged `vX.Y.Z`
(not marked pre-release — the update check reads `releases/latest`, which skips
pre-releases and drafts).

### Rice theme

With "Match my rice palette" on, Sift reads the rice's `palette.json` (`{"colors": {role: hex}}`) and maps `bg`, `surface`, `overlay`, `border`, `fg`, `muted`, `accent`, `accent2`, `accent3` onto its CSS variables, re-reading the file every 1.5 s. Safe / review / danger colours stay fixed so they keep their meaning.

The first of these that exists is used:

1. `SIFT_PALETTE` — full path to a palette.json
2. `%RICE_HOME%\palette.json`
3. `~/.config/rice/palette.json`
4. `palette.json` next to a running `rice.exe` ([windows-rice](https://github.com/BeastBeatsGaming/windows-rice) writes it beside itself)
5. `~/windows-rice/rice/palette.json`

## License
MIT — see [LICENSE](LICENSE).
