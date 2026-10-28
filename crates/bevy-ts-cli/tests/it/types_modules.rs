//! `bevy-ts types` declares every built-in module the run's engine registers, so a game type-checks
//! every module it can import (`bevy-ts/ready` was missing). Modules are registered for the life of
//! the process, so the run's app is built alone in a child process of this binary.

use crate::common::{self, BIN};
use bevy_ts_cli::cli::{Display, Engine, base_app};
use std::collections::BTreeSet;
use std::process::Command;

const PREFIX: &str = "registered module: ";

/// The child: builds the headless run's app and prints the modules it registered. Ignored, so only
/// `types_lists_every_module_the_run_registers` runs it.
#[test]
#[ignore]
fn print_run_modules() {
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let _app = base_app(&engine, bevy_ts_cli::Prebuilt, Display::Headless, Default::default(), None, false, Default::default(), &[], false);
    for name in bevy_ts::modules::all().keys() {
        println!("{PREFIX}{name}");
    }
}

#[test]
fn types_lists_every_module_the_run_registers() {
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["types_modules::print_run_modules", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&child.stdout);
    assert!(child.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&child.stderr));
    let run: BTreeSet<&str> = stdout.lines().filter_map(|l| l.strip_prefix(PREFIX)).collect();
    assert!(run.contains("bevy-ts/ready"), "{run:?}");

    let project = common::project("types-every-module", &[]);
    let dts = project.join("native.d.ts");
    let st = Command::new(BIN).arg("types").arg("--out").arg(&dts).current_dir(&project).status().unwrap();
    assert!(st.success());
    let text = std::fs::read_to_string(project.join("native-modules.d.ts")).unwrap();
    let declared: BTreeSet<&str> = text.split("declare module \"").skip(1).filter_map(|b| b.split('"').next()).collect();
    let missing: Vec<&&str> = run.difference(&declared).collect();
    assert!(missing.is_empty(), "bevy-ts types does not declare {missing:?}");
}
