//! docs/2d.md's coverage table: a row marked working names tests that exist (change script-2d).

#[test]
fn every_working_2d_row_names_a_test_that_exists() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let doc = std::fs::read_to_string(format!("{root}/docs/2d.md")).unwrap();
    let mut working = 0;
    for row in doc.lines().filter(|l| l.starts_with("| ") && !l.starts_with("| Bevy |") && !l.starts_with("| prop |")) {
        let cells: Vec<&str> = row.trim_matches('|').split(" | ").map(str::trim).collect();
        let [bevy, _, state, tests] = cells[..] else { continue };
        if state != "working" {
            continue;
        }
        working += 1;
        let names: Vec<&str> = tests.split(", ").map(|t| t.trim_matches('`')).collect();
        assert!(!names.is_empty() && names.iter().all(|n| n.contains("::")), "{bevy}: working, but names no test");
        for name in names {
            let (file, test) = name.split_once("::").unwrap();
            // `crate/file::test` names another crate's test.
            let (krate, file) = file.split_once('/').unwrap_or(("bevy-ts-cli", file));
            let src = std::fs::read_to_string(format!("{root}/crates/{krate}/tests/it/{file}.rs")).unwrap_or_default();
            assert!(src.contains(&format!("fn {test}()")), "{bevy}: no test {name}");
        }
    }
    assert!(working > 0, "no working rows read");
}
