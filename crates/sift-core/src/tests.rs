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
        SuggestOptions { min_bytes: 1, regen_min: 1, dup_min: 1000, stale_min: 1000, stale_days: 365, downloads_days: 60, find_duplicates: true, ignored: vec![] },
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
