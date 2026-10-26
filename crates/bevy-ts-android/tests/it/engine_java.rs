//! Change android-native, engine-embedding: `Engine.load` refuses a library of another ABI. On the
//! host's JVM, with a stand-in library that exports only `nativeAbi`: the class's check, not the
//! engine, is under test, and a JVM binds a native once, so a refusal and a load cannot share one
//! Android process.

use std::path::{Path, PathBuf};
use std::process::Command;

const ENGINE_JAVA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/java/dev/bevyts/Engine.java");

/// A fresh directory under the temp dir, named for the test and the process.
fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("bevy-ts-android-tests").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(cmd: &mut Command) -> std::process::Output {
    let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    assert!(out.status.success() || cmd.get_program() == "java", "{cmd:?}: {}", String::from_utf8_lossy(&out.stderr));
    out
}

/// Engine.java, a stand-in `android.view.Surface` and a main that loads the library it is given,
/// compiled into `dir/classes`.
fn compile(dir: &Path) -> PathBuf {
    let src = dir.join("src");
    std::fs::create_dir_all(src.join("android/view")).unwrap();
    std::fs::write(src.join("android/view/Surface.java"), "package android.view;\npublic class Surface {}\n").unwrap();
    std::fs::write(
        src.join("Load.java"),
        "public class Load {\n  public static void main(String[] a) {\n    try { dev.bevyts.Engine.load(a[0]); System.out.println(\"loaded\"); }\n    catch (UnsatisfiedLinkError e) { System.out.println(\"refused: \" + e.getMessage()); }\n  }\n}\n",
    )
    .unwrap();
    let classes = dir.join("classes");
    run(Command::new("javac").arg("-d").arg(&classes).arg(src.join("android/view/Surface.java")).arg(src.join("Load.java")).arg(ENGINE_JAVA));
    classes
}

/// A library whose `nativeAbi` answers `abi`.
fn library(dir: &Path, abi: u32) -> PathBuf {
    let c = dir.join(format!("abi{abi}.c"));
    std::fs::write(&c, format!("int Java_dev_bevyts_Engine_nativeAbi(void *env, void *cls) {{ return {abi}; }}\n")).unwrap();
    let lib = dir.join(format!("libabi{abi}.{}", std::env::consts::DLL_EXTENSION));
    run(Command::new("cc").arg("-shared").arg("-fPIC").arg("-o").arg(&lib).arg(&c));
    lib
}

fn load(classes: &Path, lib: &Path) -> String {
    let out = run(Command::new("java").arg("-cp").arg(classes).arg("Load").arg(lib));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn load_refuses_another_abi_and_takes_its_own() {
    let dir = project("engine-java-abi");
    let classes = compile(&dir);
    let abi = bevy_ts_embed_abi();
    let other = library(&dir, abi + 1);
    let said = load(&classes, &other);
    assert!(said.starts_with("refused: "), "{said}");
    assert!(said.contains(&format!("is engine ABI {}", abi + 1)) && said.contains(&format!("this Engine class is ABI {abi}")), "{said}");
    assert!(said.contains(&*other.to_string_lossy()), "the error names the library: {said}");
    assert_eq!(load(&classes, &library(&dir, abi)), "loaded");
}

/// The ABI the C header declares, which `Engine.ABI` must match.
fn bevy_ts_embed_abi() -> u32 {
    let header = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../bevy-ts-embed/include/bevy_ts_embed.h")).unwrap();
    let java = std::fs::read_to_string(ENGINE_JAVA).unwrap();
    let abi: u32 = header.lines().find_map(|l| l.strip_prefix("#define BEVY_TS_ABI ")).unwrap().trim().parse().unwrap();
    assert!(java.contains(&format!("public static final int ABI = {abi};")), "Engine.ABI is the header's {abi}");
    abi
}
