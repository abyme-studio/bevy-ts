//! `examples/ui-gallery` loads headless and mounts every intrinsic `jsx-runtime.d.ts` types, so a
//! new element cannot land without its gallery panel.

use crate::common;

use bevy::prelude::*;
use bevy_ts::generation::ReloadStatus;
use bevy_ts_tsx::TsxNode;
use common::*;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The tags in `IntrinsicElements`: each `name: ...Props` line of that interface.
fn intrinsics() -> BTreeSet<String> {
    let dts = std::fs::read_to_string(repo().join("packages/bevy-ts/jsx-runtime.d.ts")).unwrap();
    let body = dts.split("interface IntrinsicElements {").nth(1).expect("IntrinsicElements").split('}').next().unwrap();
    body.lines().filter_map(|l| l.trim().split_once(':')).map(|(k, _)| k.trim().to_string()).filter(|k| !k.is_empty() && !k.starts_with("//")).collect()
}

#[test]
fn the_gallery_mounts_every_intrinsic() {
    let _hang = common::watchdog("the_gallery_mounts_every_intrinsic", common::BOUND);
    let dir = repo().join("examples/ui-gallery");
    let mut app = app(&dir.join("src/gallery.tsx"));
    app.add_plugins(AssetPlugin { file_path: dir.join("assets").to_string_lossy().into_owned(), ..default() })
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<Image>()
        .init_asset::<bevy::image::TextureAtlasLayout>()
        .init_resource::<bevy::picking::pointer::PointerMap>()
        // The Focus panel's system reads `bevy-ts/input`, with the gallery's two switches on.
        .add_plugins((bevy::input::InputPlugin, bevy::input_focus::InputFocusPlugin, bevy_ts_cli::input::ScriptInputPlugin))
        .add_plugins((bevy::input_focus::tab_navigation::TabNavigationPlugin, bevy::input_focus::directional_navigation::DirectionalNavigationPlugin));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    let errors: Vec<_> = logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect();
    assert!(errors.is_empty(), "{errors:?}");

    let world = app.world_mut();
    let mut mounted = BTreeSet::new();
    for n in world.query::<&TsxNode>().iter(world) {
        mounted.extend(n.components.keys().cloned());
        mounted.extend(n.types.iter().filter(|t| *t == "entity").cloned());
    }
    let missing: Vec<_> = intrinsics().into_iter().filter(|t| !mounted.contains(t)).collect();
    assert!(missing.is_empty(), "examples/ui-gallery does not mount {missing:?}; add each to a panel");

    // The game's camera is the scene's, not a startup system's, so edit mode's Game tab shows it.
    let cameras: Vec<Vec<String>> = world.query_filtered::<&TsxNode, With<Camera3d>>().iter(world).map(|n| n.key_path.clone()).collect();
    assert!(cameras.contains(&vec!["camera".to_string()]), "the scene mounts the game camera: {cameras:?}");
}
