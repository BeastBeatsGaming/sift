use crate::*;
use std::fs;
use std::path::Path;

fn write(p: &Path, bytes: usize, fill: u8) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, vec![fill; bytes]).unwrap();
}

fn small_opts() -> (ScanOptions, SuggestOptions) {
    (
        ScanOptions { big_file_min: 1000, threads: 2 },
        SuggestOptions { min_bytes: 1, regen_min: 1, dup_min: 1000, stale_min: 1000, stale_days: 365, downloads_days: 60, find_duplicates: true, ignored: vec![], guarded_dirs: vec![], synced_dirs: vec![] },
    )
}

fn fixture() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    write(&r.join("web/app/node_modules/react/index.js"), 5000, 1);
    write(&r.join("web/app/node_modules/react/node_modules/x/y.js"), 700, 1); // nested: not separately flagged
    write(&r.join("web/app/src/main.ts"), 100, 2);
    write(&r.join("game/Assets/a.cs"), 10, 3);
    write(&r.join("game/ProjectSettings/p.asset"), 10, 3);
    write(&r.join("game/Library/cache.bin"), 4000, 4);
    write(&r.join("notunity/Library/book.pdf"), 3000, 5); // Library outside a Unity project: keep
    write(&r.join("media/clip.mp4"), 2500, 6);
    write(&r.join("media/backup/clip copy.mp4"), 2500, 6); // duplicate
    write(&r.join("media/other.mp4"), 2500, 7); // same size, different content
    write(&r.join("art/scene.blend1"), 1200, 8);
    write(&r.join("ue/Shooter.uproject"), 10, 9);
    write(&r.join("ue/DerivedDataCache/ddc.bin"), 900, 9);
    write(&r.join("notue/Intermediate/x.bin"), 50, 9); // no .uproject: keep
    t
}

#[test]
fn sizes_add_up() {
    let t = fixture();
    let (so, _) = small_opts();
    let s = scan(t.path(), &so, &Progress::default());
    let expected = 5000 + 700 + 100 + 10 + 10 + 4000 + 3000 + 2500 * 3 + 1200 + 10 + 900 + 50;
    assert_eq!(s.root.size, expected);
    assert_eq!(s.stats.files, 14);
    assert_eq!(s.root.file_count, 14);
    // children sorted largest first
    let sizes: Vec<u64> = s.root.children.iter().map(|c| c.size).collect();
    let mut sorted = sizes.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(sizes, sorted);
}

#[test]
fn regen_rules_respect_context() {
    let t = fixture();
    let (so, sg) = small_opts();
    let s = scan(t.path(), &so, &Progress::default());
    let r = suggest(&s, &sg);
    let nm = r.suggestions.iter().find(|x| x.id == "regen:node_modules").expect("node_modules flagged");
    assert_eq!(nm.items.len(), 1, "nested node_modules must not be listed twice");
    assert_eq!(nm.bytes, 5700);
    let lib = r.suggestions.iter().find(|x| x.id == "regen:unity_library").expect("unity library flagged");
    assert_eq!(lib.items.len(), 1);
    assert!(lib.items[0].path.ends_with("game/Library"));
    let ddc = r.suggestions.iter().find(|x| x.id == "regen:unreal_ddc").expect("unreal ddc flagged");
    assert_eq!(ddc.bytes, 900);
    assert!(r.suggestions.iter().all(|x| x.id != "regen:unreal_intermediate"));
}

#[test]
fn finds_duplicates_and_specials() {
    let t = fixture();
    let (so, sg) = small_opts();
    let s = scan(t.path(), &so, &Progress::default());
    let r = suggest(&s, &sg);
    assert_eq!(r.duplicates.len(), 1);
    assert_eq!(r.duplicates[0].paths.len(), 2);
    assert_eq!(r.duplicates[0].wasted, 2500);
    let b = r.suggestions.iter().find(|x| x.id == "ext:blend_backup").unwrap();
    assert_eq!(b.bytes, 1200);
}

#[test]
fn tree_lookup_matches_disk() {
    let t = fixture();
    let (so, _) = small_opts();
    let s = scan(t.path(), &so, &Progress::default());
    let p = t.path().join("web/app");
    assert_eq!(suggest::tree_size(&s, &p), crate::scan::path_size(&p));
    assert_eq!(suggest::tree_size(&s, &p), Some(5800));
}

#[test]
fn expand_env_and_glob() {
    let t = fixture();
    std::env::set_var("SIFT_TEST_ROOT", t.path());
    let mut got = suggest::expand("%SIFT_TEST_ROOT%/*/Library");
    got.sort();
    assert_eq!(got, vec![t.path().join("game/Library"), t.path().join("notunity/Library")]);
    assert!(suggest::expand("%SIFT_DOES_NOT_EXIST%/x").is_empty());
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    let t = fixture();
    std::os::unix::fs::symlink(t.path().join("media"), t.path().join("loop")).unwrap();
    let (so, _) = small_opts();
    let s = scan(t.path(), &so, &Progress::default());
    assert_eq!(s.stats.files, 14);
}

#[test]
fn cancel_stops_early() {
    let t = fixture();
    let p = Progress::default();
    p.cancel();
    let s = scan(t.path(), &ScanOptions::default(), &p);
    assert_eq!(s.root.size, 0);
}

#[test]
fn report_serializes() {
    let t = fixture();
    let (so, sg) = small_opts();
    let s = scan(t.path(), &so, &Progress::default());
    let json = serde_json::to_string(&suggest(&s, &sg)).unwrap();
    assert!(json.contains("\"kind\":\"quarantine\""));
}

// ------------------------------------------------------------------ trash

fn trash_in(t: &tempfile::TempDir) -> crate::Trash {
    crate::Trash::open(t.path().join("state/history.json"), t.path().join("trash"))
}

#[test]
fn quarantine_and_restore_folder() {
    let t = fixture();
    let nm = t.path().join("web/app/node_modules");
    let mut tr = trash_in(&t);
    let o = tr.quarantine("node_modules", &[(nm.clone(), 5700)], false);
    assert_eq!(o.done, 1);
    assert_eq!(o.bytes, 5700);
    assert!(!nm.exists());
    assert_eq!(tr.bytes_in_trash(), 5700);

    let id = o.batch_id.unwrap();
    let r = tr.restore(&id).unwrap();
    assert_eq!(r.done, 1);
    assert!(nm.join("react/index.js").exists());
    assert_eq!(tr.bytes_in_trash(), 0);
}

#[test]
fn contents_mode_keeps_the_folder() {
    let t = fixture();
    let media = t.path().join("media");
    let mut tr = trash_in(&t);
    let o = tr.quarantine("media", &[(media.clone(), 0)], true);
    assert_eq!(o.done, 3, "clip.mp4, other.mp4, backup/");
    assert!(media.is_dir());
    assert_eq!(fs::read_dir(&media).unwrap().count(), 0);
    assert_eq!(o.bytes, 7500);
}

#[test]
fn restore_refuses_to_overwrite() {
    let t = fixture();
    let nm = t.path().join("web/app/node_modules");
    let mut tr = trash_in(&t);
    let id = tr.quarantine("nm", &[(nm.clone(), 5700)], false).batch_id.unwrap();
    write(&nm.join("fresh.js"), 10, 1); // "npm install" ran again
    let r = tr.restore(&id).unwrap();
    assert_eq!(r.done, 0);
    assert_eq!(r.skipped.len(), 1);
    assert_eq!(tr.bytes_in_trash(), 5700, "stays in the trash");
}

#[test]
fn purge_frees_and_history_survives_reopen() {
    let t = fixture();
    let f = t.path().join("art/scene.blend1");
    let mut tr = trash_in(&t);
    let id = tr.quarantine("blend", &[(f.clone(), 1200)], false).batch_id.unwrap();
    drop(tr);
    let mut tr = trash_in(&t);
    assert_eq!(tr.history().batches.len(), 1);
    let p = tr.purge(&id).unwrap();
    assert_eq!(p.bytes, 1200);
    assert_eq!(tr.bytes_in_trash(), 0);
    assert!(tr.restore(&id).unwrap().done == 0);
    // nothing left behind in the trash folder
    let leftovers: Vec<_> = fs::read_dir(t.path().join("trash")).unwrap().collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn readonly_files_are_purged() {
    let t = fixture();
    let f = t.path().join("ro.bin");
    write(&f, 50, 1);
    let mut perm = fs::metadata(&f).unwrap().permissions();
    perm.set_readonly(true);
    fs::set_permissions(&f, perm).unwrap();
    let mut tr = trash_in(&t);
    let id = tr.quarantine("ro", &[(f, 50)], false).batch_id.unwrap();
    assert_eq!(tr.purge(&id).unwrap().done, 1);
}

#[test]
fn protected_paths_are_refused() {
    use crate::protect::check_movable;
    assert!(check_movable(Path::new("/")).is_err());
    if let Ok(home) = std::env::var("HOME") {
        assert!(check_movable(Path::new(&home)).is_err());
        assert!(check_movable(Path::new(&home).parent().unwrap()).is_err(), "folder containing home");
    }
    assert!(check_movable(Path::new("/x/.sift-trash/1/a")).is_err());
    assert!(check_movable(Path::new("/x/pagefile.sys")).is_err());
    assert!(check_movable(Path::new("/tmp/some/project/node_modules")).is_ok());
    assert!(check_movable(Path::new("relative/path")).is_err());
}

#[test]
fn scan_and_report_forget_removed_paths() {
    let t = fixture();
    let (so, sg) = small_opts();
    let mut s = scan(t.path(), &so, &Progress::default());
    let mut r = suggest(&s, &sg);
    let before = s.root.size;
    let nm = t.path().join("web/app/node_modules");
    let dup = r.duplicates[0].paths[1].clone();

    s.forget(&nm, 5700);
    s.forget(&dup, 2500);
    r.forget(&[nm.clone(), dup.clone()], &[]);

    assert_eq!(s.root.size, before - 5700 - 2500);
    assert_eq!(suggest::tree_size(&s, &t.path().join("web")), Some(100));
    assert!(r.suggestions.iter().all(|x| x.id != "regen:node_modules"));
    assert!(r.duplicates.is_empty());
    assert!(r.suggestions.iter().all(|x| x.id != "duplicates"));
    assert!(s.big_files.iter().all(|f| f.path != dup));
}

#[cfg(unix)]
#[test]
fn commands_run_and_record() {
    let (code, out) = crate::actions::run_command("echo", &["hello".into()]);
    assert_eq!(code, Some(0));
    assert_eq!(out, "hello");
    let (code, out) = crate::actions::run_command("definitely-not-a-program-xyz", &[]);
    assert_eq!(code, None);
    assert!(out.starts_with("Couldn't start"));
}

#[test]
fn ignored_paths_are_never_suggested() {
    let t = fixture();
    let (so, mut sg) = small_opts();
    sg.ignored = vec![t.path().join("web"), t.path().join("media/backup")];
    let s = scan(t.path(), &so, &Progress::default());
    let r = suggest(&s, &sg);
    assert!(r.suggestions.iter().all(|x| x.id != "regen:node_modules"));
    assert!(r.duplicates.is_empty(), "the only duplicate pair loses its copy");
    assert!(r.suggestions.iter().any(|x| x.id == "regen:unity_library"));
}

#[test]
fn full_check_rejects_sample_lookalikes() {
    let t = tempfile::tempdir().unwrap();
    let size = 4 * 1024 * 1024;
    let a = t.path().join("a.bin");
    let b = t.path().join("b.bin");
    let c = t.path().join("c.bin");
    write(&a, size, 7);
    write(&b, size, 7);
    // same size and same sampled regions (first/middle/last MiB), but one
    // byte differs in an unsampled stretch
    let mut bytes = vec![7u8; size];
    bytes[1024 * 1024 + 10] = 8;
    fs::write(&c, bytes).unwrap();
    assert_eq!(dupes::sampled_hash(&a, size as u64), dupes::sampled_hash(&c, size as u64));

    let (same, differ) = dupes::verify_copies(&a, &[a.clone(), b.clone(), c.clone()]).unwrap();
    assert_eq!(same, vec![b]);
    assert_eq!(differ.len(), 1);
    assert_eq!(differ[0].0, c);
    assert!(dupes::verify_copies(&t.path().join("missing"), &[a]).is_err());
}

#[test]
fn snapshot_round_trips() {
    let t = fixture();
    let (so, sg) = small_opts();
    let scan = scan(t.path(), &so, &Progress::default());
    let report = suggest(&scan, &sg);
    let file = t.path().join("state/last-scan.json");
    crate::snapshot::save(&file, &scan, &report).unwrap();
    let (s2, r2) = crate::snapshot::load(&file).expect("loads");
    assert_eq!(s2.root.size, scan.root.size);
    assert_eq!(s2.scanned_at, scan.scanned_at);
    assert_eq!(s2.big_files.len(), scan.big_files.len());
    assert_eq!(s2.special.len(), scan.special.len());
    assert_eq!(r2.suggestions.len(), report.suggestions.len());
    // rule ids survive the trip, so regenerable folders are still found
    let unity = s2.root.children.iter().find(|c| c.name == "game").unwrap();
    assert!(unity.children.iter().any(|c| c.regen_rule == Some("unity_library")));
    assert!(crate::snapshot::load(&t.path().join("missing.json")).is_none());
}

#[test]
fn cargo_target_needs_a_manifest() {
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    write(&r.join("crate/Cargo.toml"), 10, 1);
    write(&r.join("crate/target/debug/big.rlib"), 5000, 2);
    write(&r.join("site/target/keep.txt"), 5000, 3); // no Cargo.toml: keep
    let (so, sg) = small_opts();
    let report = suggest(&scan(r, &so, &Progress::default()), &sg);
    let s = report.suggestions.iter().find(|s| s.id == "regen:cargo_target").expect("flagged");
    assert_eq!(s.items.len(), 1);
    assert!(s.items[0].path.ends_with("crate/target"));
}

#[test]
fn only_trash_moves_are_undoable() {
    use crate::suggest::is_permanent;
    assert!(!is_permanent("x", &Action::Quarantine { contents: true }));
    assert!(is_permanent("npm_cache", &Action::Command { program: "npm".into(), args: vec![] }));
    assert!(is_permanent("windows_old", &Action::OpenSettings { uri: "ms-settings:storagepolicies".into() }));
    assert!(is_permanent("recycle_bin", &Action::Manual));
    assert!(!is_permanent("hiberfil", &Action::Manual));
}

// ------------------------------------------- installed software & cloud sync

fn age(p: &Path, days: u64) {
    let t = std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86_400);
    fs::File::options().write(true).open(p).unwrap().set_modified(t).unwrap();
}

#[test]
fn guarded_locations_are_recognised() {
    use crate::protect::{guarded_in, Guarded};
    let roots = vec![(Path::new("/c/Program Files").to_path_buf(), Guarded::ProgramFiles), (Path::new("/c/Users/a/AppData/Local").to_path_buf(), Guarded::AppData)];
    assert_eq!(guarded_in(Path::new("/c/Program Files/nodejs/node_modules"), &roots), Some(Guarded::ProgramFiles));
    assert_eq!(guarded_in(Path::new("/c/Users/a/AppData/Local/Packages/x/LocalState/ext4.vhdx"), &roots), Some(Guarded::AppData));
    assert_eq!(guarded_in(Path::new("/d/SteamLibrary/steamapps/common/Game/data.pak"), &roots), Some(Guarded::GameLibrary));
    assert_eq!(guarded_in(Path::new("/d/SteamLibrary/steamapps/workshop/content/1"), &roots), Some(Guarded::GameLibrary));
    assert_eq!(guarded_in(Path::new("/d/Epic Games/Fortnite/x.pak"), &roots), Some(Guarded::GameLibrary));
    // shader cache lives in steamapps but isn't game data; the folder named like a root isn't inside it
    assert_eq!(guarded_in(Path::new("/d/SteamLibrary/steamapps/shadercache/1"), &roots), None);
    assert_eq!(guarded_in(Path::new("/c/Program Files Backup/x"), &roots), None);
    assert_eq!(guarded_in(Path::new("/c/Users/a/Documents/big.iso"), &roots), None);
}

#[test]
fn scan_suggestions_stay_out_of_installed_software() {
    let t = fixture();
    let r = t.path();
    // an app that ships its own node_modules, a crash dump and a big old file in AppData,
    // and the same video as in `media/` hidden inside Program Files
    write(&r.join("pf/nodejs/node_modules/npm/index.js"), 5000, 1);
    write(&r.join("pf/app/crash.dmp"), 2000, 1);
    write(&r.join("pf/app/clip.mp4"), 2500, 6);
    write(&r.join("appdata/Packages/distro/ext4.vhdx"), 3000, 1);
    age(&r.join("appdata/Packages/distro/ext4.vhdx"), 800);
    write(&r.join("docs/old-render.bin"), 3000, 2);
    age(&r.join("docs/old-render.bin"), 800);

    let (so, mut sg) = small_opts();
    sg.guarded_dirs = vec![r.join("pf"), r.join("appdata")];
    let s = scan(r, &so, &Progress::default());
    let rep = suggest(&s, &sg);
    let in_guarded = |p: &Path| p.starts_with(r.join("pf")) || p.starts_with(r.join("appdata"));

    for sug in rep.suggestions.iter().filter(|x| crate::suggest::is_scan_derived(&x.id)) {
        assert!(sug.items.iter().all(|i| !in_guarded(&i.path)), "{} lists {:?}", sug.id, sug.items);
    }
    assert!(rep.duplicates.iter().all(|g| g.paths.iter().all(|p| !in_guarded(p))));
    // the real ones are still found
    assert!(rep.suggestions.iter().any(|x| x.id == "regen:node_modules" && x.items.iter().any(|i| i.path.ends_with("web/app/node_modules"))));
    let stale = rep.suggestions.iter().find(|x| x.id == "stale").expect("old render is stale");
    assert_eq!(stale.items.len(), 1);
    assert!(stale.items[0].path.ends_with("docs/old-render.bin"));
    assert!(rep.suggestions.iter().all(|x| x.id != "ext:dumps"), "the only .dmp is inside Program Files");
    assert_eq!(rep.duplicates.len(), 1);
    assert_eq!(rep.duplicates[0].paths.len(), 2, "media copies only");
}

#[test]
fn synced_files_are_never_archived_or_deduplicated() {
    use crate::protect::in_synced;
    let t = fixture();
    let r = t.path();
    write(&r.join("OneDrive/Videos/holiday.mp4"), 4000, 1);
    age(&r.join("OneDrive/Videos/holiday.mp4"), 800);
    write(&r.join("OneDrive/Videos/clip copy.mp4"), 2500, 6); // duplicate of media/clip.mp4
    write(&r.join("OneDrive/proj/node_modules/lib/index.js"), 4000, 1); // junk: fine to clear

    assert!(in_synced(&r.join("OneDrive/Videos/holiday.mp4"), &[r.join("OneDrive")]));
    assert!(!in_synced(&r.join("OneDrive2/x"), &[r.join("OneDrive")]));

    let (so, mut sg) = small_opts();
    sg.synced_dirs = vec![r.join("OneDrive")];
    let s = scan(r, &so, &Progress::default());
    let rep = suggest(&s, &sg);
    assert!(rep.suggestions.iter().all(|x| x.id != "stale"), "the only old file is in OneDrive");
    let synced = |p: &Path| p.starts_with(r.join("OneDrive"));
    assert!(rep.duplicates.iter().all(|g| g.paths.iter().all(|p| !synced(p))));
    // regenerable junk in OneDrive is still offered
    let nm = rep.suggestions.iter().find(|x| x.id == "regen:node_modules").unwrap();
    assert!(nm.items.iter().any(|i| synced(&i.path)));
    assert!(crate::suggest::is_archive_like("stale") && !crate::suggest::is_archive_like("regen:node_modules"));
}

// ---------------------------------------------------------- journal & history

fn history_file(t: &tempfile::TempDir) -> std::path::PathBuf {
    t.path().join("state/history.json")
}

#[test]
fn interrupted_cleanup_is_settled_on_reopen() {
    use crate::trash::{Batch, BatchKind, Entry, EntryState, History};
    let t = fixture();
    let moved_src = t.path().join("art/scene.blend1");
    let stayed = t.path().join("media/other.mp4");
    // what a crash leaves behind: both journaled as Pending, only the first one moved
    let slot_a = t.path().join("trash/b1/0");
    let slot_b = t.path().join("trash/b1/1");
    fs::create_dir_all(&slot_a).unwrap();
    fs::create_dir_all(&slot_b).unwrap();
    fs::rename(&moved_src, slot_a.join("scene.blend1")).unwrap();
    let entry = |o: &Path, s: std::path::PathBuf| Entry { original: o.to_path_buf(), stored: s, bytes: 10, state: EntryState::Pending };
    let h = History {
        batches: vec![Batch {
            id: "b1".into(),
            created: 1,
            label: "x".into(),
            kind: BatchKind::Quarantine { entries: vec![entry(&moved_src, slot_a.join("scene.blend1")), entry(&stayed, slot_b.join("other.mp4"))] },
        }],
    };
    fs::create_dir_all(t.path().join("state")).unwrap();
    fs::write(history_file(&t), serde_json::to_vec(&h).unwrap()).unwrap();

    let mut tr = trash_in(&t);
    assert!(tr.notice().unwrap().contains("middle of a cleanup"));
    let BatchKind::Quarantine { entries } = &tr.history().batches[0].kind else { panic!() };
    assert_eq!(entries.len(), 1, "the item that never moved is dropped");
    assert_eq!(entries[0].state, EntryState::InTrash);
    assert!(stayed.exists(), "never touched");
    assert!(!slot_b.exists(), "empty slot removed");
    // and the moved one can be restored
    assert_eq!(tr.restore("b1").unwrap().done, 1);
    assert!(moved_src.exists());
    // settled state was saved: reopening shows no pending items and no new notice
    drop(tr);
    assert!(trash_in(&t).notice().is_none());
}

#[test]
fn nothing_moves_if_the_journal_cannot_be_written() {
    let t = fixture();
    let mut tr = trash_in(&t); // opens fine: no history yet
    fs::write(t.path().join("state"), b"a file where the history folder should be").unwrap();
    let f = t.path().join("art/scene.blend1");
    let o = tr.quarantine("blend", &[(f.clone(), 1200)], false);
    assert_eq!(o.done, 0);
    assert!(f.exists(), "file untouched");
    assert!(o.skipped[0].reason.contains("nothing was moved"), "{}", o.skipped[0].reason);
    assert!(tr.history().batches.is_empty());
    let left: Vec<_> = fs::read_dir(t.path().join("trash")).map(|r| r.collect()).unwrap_or_default();
    assert!(left.is_empty(), "no stray folders: {left:?}");
}

#[test]
fn history_in_an_unusable_place_blocks_cleanups() {
    let t = fixture();
    let blocker = t.path().join("blocker");
    fs::write(&blocker, b"x").unwrap();
    let mut tr = crate::Trash::open(blocker.join("history.json"), t.path().join("trash"));
    let f = t.path().join("art/scene.blend1");
    let o = tr.quarantine("blend", &[(f.clone(), 1200)], false);
    assert_eq!((o.done, f.exists()), (0, true));
}

#[test]
fn damaged_history_is_set_aside_and_rebuilt_from_manifests() {
    let t = fixture();
    let a = t.path().join("art/scene.blend1");
    let b = t.path().join("media/other.mp4");
    let mut tr = trash_in(&t);
    tr.quarantine("first", &[(a.clone(), 1200)], false);
    let after_first = fs::read(history_file(&t)).unwrap();
    tr.quarantine("second", &[(b.clone(), 2500)], false);
    let bak = history_file(&t).with_extension("json.bak");
    assert!(bak.exists(), "previous version kept");
    fs::write(&bak, after_first).unwrap(); // as if Sift died before the second save landed
    drop(tr);

    // garbage where the history was
    fs::write(history_file(&t), b"{ not json").unwrap();
    let mut tr = trash_in(&t);
    assert!(tr.notice().unwrap().contains("damaged"));
    assert!(tr.needs_recovery());
    let kept: Vec<_> = fs::read_dir(t.path().join("state")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("history.corrupt-")).collect();
    assert_eq!(kept.len(), 1, "damaged file kept, not overwritten");
    assert_eq!(fs::read(kept[0].path()).unwrap(), b"{ not json");

    // the .bak knew about the first cleanup; the manifests know about both
    assert_eq!(tr.history().batches.len(), 1);
    assert_eq!(tr.recover_from_manifests(&[]), 1);
    assert_eq!(tr.history().batches.len(), 2);
    assert_eq!(tr.bytes_in_trash(), 1200 + 2500);
    assert!(!tr.needs_recovery());
    let id = tr.history().batches[1].id.clone();
    assert_eq!(tr.restore(&id).unwrap().done, 1);
    assert!(b.exists());
    // the repaired file is valid again
    drop(tr);
    let tr = trash_in(&t);
    assert!(tr.notice().is_none());
    assert_eq!(tr.history().batches.len(), 2);
}

#[test]
fn damaged_history_with_no_backup_still_recovers() {
    let t = fixture();
    let a = t.path().join("art/scene.blend1");
    let mut tr = trash_in(&t);
    tr.quarantine("only", &[(a.clone(), 1200)], false);
    drop(tr);
    fs::remove_file(history_file(&t).with_extension("json.bak")).ok();
    fs::write(history_file(&t), b"\0\0\0").unwrap();

    let mut tr = trash_in(&t);
    assert!(tr.history().batches.is_empty());
    assert_eq!(tr.recover_from_manifests(&[]), 1);
    assert_eq!(tr.bytes_in_trash(), 1200);
    let id = tr.history().batches[0].id.clone();
    assert_eq!(tr.restore(&id).unwrap().done, 1);
    assert!(a.exists());
}

#[cfg(unix)]
#[test]
fn unreadable_history_blocks_cleanups_instead_of_being_overwritten() {
    use std::os::unix::fs::PermissionsExt;
    let t = fixture();
    let mut tr = trash_in(&t);
    tr.quarantine("one", &[(t.path().join("art/scene.blend1"), 1200)], false);
    drop(tr);
    let h = history_file(&t);
    fs::set_permissions(&h, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&h).is_ok() {
        return; // running as root: can't make a file unreadable
    }
    let mut tr = trash_in(&t);
    assert!(tr.notice().unwrap().contains("can't read"));
    let f = t.path().join("media/other.mp4");
    let o = tr.quarantine("two", &[(f.clone(), 2500)], false);
    assert_eq!(o.done, 0);
    assert!(f.exists());
    fs::set_permissions(&h, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(trash_in(&t).history().batches.len(), 1, "history untouched");
}
