//! `common::TestDir`, the scratch folder every test makes (docs/testing.md).

use crate::common::TestDir;

#[test]
fn a_test_dir_is_removed_when_it_drops() {
    let dir = TestDir::with_keep("test-dir-removed", false);
    std::fs::write(dir.join("f.txt"), "x").unwrap();
    let path = dir.to_path_buf();
    assert!(path.is_dir());
    drop(dir);
    assert!(!path.exists());
}

#[test]
fn a_kept_test_dir_stays_and_two_of_one_name_differ() {
    let a = TestDir::with_keep("test-dir-kept", true);
    let b = TestDir::with_keep("test-dir-kept", true);
    assert_ne!(*a, *b);
    let (pa, pb) = (a.to_path_buf(), b.to_path_buf());
    drop((a, b));
    assert!(pa.is_dir() && pb.is_dir());
    std::fs::remove_dir_all(&pa).unwrap();
    std::fs::remove_dir_all(&pb).unwrap();
}
