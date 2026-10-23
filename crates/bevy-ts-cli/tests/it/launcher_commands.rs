//! `new` and `examples` belong to the npm launcher: the engine binary run directly refuses them.

use std::process::Command;

#[test]
fn the_binary_refuses_the_launchers_commands() {
    for cmd in ["new", "examples"] {
        let out = Command::new(env!("CARGO_BIN_EXE_bevy-ts")).args([cmd, "--json"]).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{cmd}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(&format!("npx bevy-ts {cmd}")), "{cmd}: {err}");
        assert!(out.stdout.is_empty(), "{cmd}");
    }
}
