//! TSX scenes for bevy-ts: the Rust side of `app.scene`.
//!
//! JS evaluates every scene during a generation's load and writes a plain-data description into
//! the manifest's `tsx` section. This crate checks it with the generation (an error fails the
//! whole reload, with the element's TS location), then, once the generation's components are
//! live, reconciles each root against what it last applied: by key path, writing only what the
//! description changed. The retained tree is ECS state ([`TsxNode`], [`TsxTree`]), so it outlives
//! every JS isolate. Nothing here runs per frame.

pub mod desc;
pub mod intrinsics;
pub mod mount;
pub mod reconcile;
pub mod resolve;
pub mod tree;

use crate::desc::{SceneSection, normalize_sources};
use crate::reconcile::{apply_root, resolve_tree, unmount_root};
use crate::tree::{ReconcileStats, RUNTIME_ROOT};
use bevy::prelude::*;
use bevy_ts::backend::ScriptError;
use bevy_ts::layout::NativeLayouts;
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts::plugin_api::BevyTsAppExt;

pub use crate::tree::{Provenance, TsxNode, TsxTree};

/// The manifest section `app.scene` writes.
pub const SECTION: &str = "tsx";

/// Adds TSX scenes to an app with [`bevy_ts::BevyTsPlugin`].
pub struct TsxPlugin;

fn parse(bytes: &[u8]) -> Result<SceneSection, ScriptError> {
    let mut s: SceneSection = serde_json::from_slice(bytes).map_err(|e| ScriptError::new(format!("tsx section: {e}")))?;
    for e in &mut s.errors {
        if let Some(l) = &mut e.location {
            l.file = desc::display_file(&l.file);
        }
    }
    for r in &mut s.roots {
        normalize_sources(&mut r.nodes);
        if let Some(l) = &mut r.location {
            l.file = desc::display_file(&l.file);
        }
    }
    Ok(s)
}

pub(crate) fn uses_assets(nodes: &[desc::NodeDesc]) -> Option<Option<bevy_ts::backend::SourceLocation>> {
    for n in nodes {
        if let Some(c) = n.components.iter().find(|c| c.kind == desc::ComponentKind::Intrinsic) {
            return Some(c.source.clone());
        }
        if let Some(l) = uses_assets(&n.children) {
            return Some(l);
        }
    }
    None
}

/// Checks a generation's scenes against its pending components. Touches nothing.
fn check(world: &mut World, input: &bevy_ts::plugin_api::SectionInput) -> Result<(), Vec<ScriptError>> {
    let section = parse(input.bytes).map_err(|e| vec![e])?;
    if !section.errors.is_empty() {
        return Err(section.errors);
    }
    let mut errors = resolve::check_roots(&section.roots, input.components, world.resource::<NativeLayouts>());
    if !intrinsics::available(world)
        && let Some(at) = section.roots.iter().find_map(|r| uses_assets(&r.nodes))
    {
        errors.push(ScriptError::at("<mesh> and <material> need the render plugins (no Assets<Mesh>)", at));
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

/// Reconciles every root of an applied generation, and unmounts roots it no longer declares.
pub fn reconcile_generation(world: &mut World, generation: u64, section: &SceneSection) -> ReconcileStats {
    // Ids and layouts may have changed with the generation.
    world.resource_mut::<reconcile::TemplateCache>().clear();
    let mut total = ReconcileStats::default();
    for r in &section.roots {
        match resolve_tree(world, &r.name, &r.nodes, (&[], &[])) {
            Ok(nodes) => total.add(&apply_root(world, &r.name, r.location.clone(), &nodes, None, true)),
            // Checked with the generation; only a bug lands here.
            Err(e) => world.resource_mut::<ScriptLog>().push(LogLevel::Error, format!("tsx: {}", e.message), e.location, generation),
        }
    }
    let gone: Vec<String> = world
        .resource::<TsxTree>()
        .roots
        .keys()
        .filter(|k| k.as_str() != RUNTIME_ROOT && !section.roots.iter().any(|r| &r.name == *k))
        .cloned()
        .collect();
    for name in gone {
        total.add(&unmount_root(world, &name));
    }
    world.resource_mut::<TsxTree>().last = total.clone();
    total
}

fn apply(world: &mut World, applied: &bevy_ts::plugin_api::AppliedSection) {
    // A generation without the section declares no scenes: every scene root unmounts.
    let section = if applied.present {
        match parse(applied.bytes) {
            Ok(s) => s,
            Err(e) => {
                world.resource_mut::<ScriptLog>().push(LogLevel::Error, e.message, None, applied.generation);
                return;
            }
        }
    } else {
        SceneSection::default()
    };
    reconcile_generation(world, applied.generation, &section);
}

/// `restart: true`: TS-spawned entities are gone; tsx's go too, with every tombstone, so each
/// scene mounts as if first.
fn restart(world: &mut World) {
    let nodes: Vec<Entity> = world.query_filtered::<Entity, With<TsxNode>>().iter(world).collect();
    for e in nodes {
        // Children may already be gone with their parent.
        let _ = world.try_despawn(e);
    }
    world.insert_resource(TsxTree::default());
}

impl Plugin for TsxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TsxTree>()
            .init_resource::<intrinsics::IntrinsicAssets>()
            .init_resource::<reconcile::TemplateCache>();
        app.add_ts_section(SECTION, check);
        app.on_ts_section_applied(SECTION, apply);
        app.on_ts_restart(restart);
        app.add_ts_command_kind(mount::MOUNT, mount::mount);
        app.add_ts_command_kind(mount::UNMOUNT, mount::unmount);
    }
}
