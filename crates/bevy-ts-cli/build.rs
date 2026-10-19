//! Records what a project engine must match: the target triple, the Bevy version this engine
//! was built with (from the workspace `Cargo.lock`) and where the bevy-ts sources live, for
//! project engines built against a checkout instead of crates.io.

use std::path::Path;

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=BEVY_TS_TARGET={target}");
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&manifest).join("../..");
    let lock = root.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let bevy = std::fs::read_to_string(&lock)
        .ok()
        .and_then(|l| {
            let mut lines = l.lines();
            while let Some(line) = lines.next() {
                if line == "name = \"bevy\"" {
                    return lines.next().and_then(|v| v.strip_prefix("version = \"")).map(|v| v.trim_end_matches('"').to_string());
                }
            }
            None
        })
        .unwrap_or_default();
    println!("cargo:rustc-env=BEVY_TS_BEVY_VERSION={bevy}");
    let root = root.canonicalize().map(|p| p.display().to_string()).unwrap_or_default();
    println!("cargo:rustc-env=BEVY_TS_SOURCE_ROOT={root}");
}
