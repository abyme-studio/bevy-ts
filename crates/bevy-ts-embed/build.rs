//! The target triple an embedded engine names in its identity.

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=BEVY_TS_EMBED_TARGET={target}");
}
