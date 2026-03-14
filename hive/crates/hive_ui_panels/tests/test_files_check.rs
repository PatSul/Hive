use std::path::PathBuf;

use hive_ui_panels::panels::files::FilesData;

#[test]
fn toggle_check_adds_and_removes() {
    let mut data = FilesData::default();
    let path = PathBuf::from("/tmp/test.rs");

    // First toggle adds
    data.toggle_check(&path);
    assert_eq!(data.check_count(), 1);
    assert!(data.checked_files.contains(&path));

    // Second toggle removes
    data.toggle_check(&path);
    assert_eq!(data.check_count(), 0);
    assert!(!data.checked_files.contains(&path));
}

#[test]
fn checked_paths_returns_sorted() {
    let mut data = FilesData::default();
    let z = PathBuf::from("/z.rs");
    let a = PathBuf::from("/a.rs");
    let m = PathBuf::from("/m.rs");

    data.toggle_check(&z);
    data.toggle_check(&a);
    data.toggle_check(&m);

    let paths = data.checked_paths();
    assert_eq!(paths, vec![a, m, z]);
}

#[test]
fn clear_checked_removes_all() {
    let mut data = FilesData::default();
    data.toggle_check(&PathBuf::from("/a.rs"));
    data.toggle_check(&PathBuf::from("/b.rs"));
    data.toggle_check(&PathBuf::from("/c.rs"));
    assert_eq!(data.check_count(), 3);

    data.clear_checked();
    assert_eq!(data.check_count(), 0);
    assert!(data.checked_paths().is_empty());
}

#[test]
fn check_count_matches_unique_files() {
    let mut data = FilesData::default();
    let path = PathBuf::from("/same.rs");

    data.toggle_check(&path);
    data.toggle_check(&PathBuf::from("/other.rs"));
    assert_eq!(data.check_count(), 2);

    // Toggle same path again removes it
    data.toggle_check(&path);
    assert_eq!(data.check_count(), 1);
}
