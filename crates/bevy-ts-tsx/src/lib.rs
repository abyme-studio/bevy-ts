//! TSX scenes for bevy-ts: the Rust side of `app.scene`.
//!
//! JS evaluates every scene during a generation's load and writes a plain-data description into
//! the manifest's `tsx` section. This crate checks it with the generation (an error fails the
//! whole reload, with the element's TS location), then, once the generation's components are
//! live, reconciles each root against what it last applied: by key path, writing only what the
//! description changed. The retained tree is ECS state ([`TsxNode`], [`TsxTree`]), so it outlives
//! every JS isolate. Nothing here runs per frame.

pub mod desc;
pub mod host;
pub mod intrinsics;
pub mod mount;
pub mod overlay;
pub mod picking;
pub mod protocol;
pub mod reconcile;
pub mod reflected;
pub mod resolve;
pub mod tree;
pub mod ui;

use crate::desc::SceneSection;
use crate::reconcile::{apply_root, resolve_tree, unmount_root};
use crate::tree::{ReconcileStats, RUNTIME_ROOT};
use bevy::prelude::*;
use bevy_ts::backend::ScriptError;
use bevy_ts::layout::NativeLayouts;
use bevy_ts::log::{LogLevel, ScriptLog};
use bevy_ts::plugin_api::BevyTsAppExt;

pub use crate::host::{ExpandItem, expand};
pub use crate::tree::{Provenance, TsxNode, TsxTree};

/// The manifest section `app.scene` writes.
pub const SECTION: &str = "tsx";

/// Adds TSX scenes to an app with [`bevy_ts::BevyTsPlugin`].
pub struct TsxPlugin;

fn parse(bytes: &[u8]) -> Result<SceneSection, ScriptError> {
    let mut s: SceneSection = serde_json::from_slice(bytes).map_err(|e| ScriptError::new(format!("tsx section: {e}")))?;
    let mut files = desc::Files::new();
    for e in &mut s.errors {
        if let Some(l) = &mut e.location {
            l.file = files.display(&l.file);
        }
    }
    for r in &mut s.roots {
        desc::normalize_with(&mut files, &mut r.nodes);
        if let Some(l) = &mut r.location {
            l.file = files.display(&l.file);
        }
    }
    Ok(s)
}

pub(crate) fn uses_assets(nodes: &[desc::NodeDesc]) -> Option<Option<bevy_ts::backend::SourceLocation>> {
    for n in nodes {
        if let Some(c) = n.components.iter().find(|c| c.kind == desc::ComponentKind::Intrinsic && (c.ty == resolve::MESH || c.ty == resolve::MATERIAL)) {
            return Some(c.source.clone());
        }
        if let Some(l) = uses_assets(&n.children) {
            return Some(l);
        }
    }
    None
}

/// Every `<material kind>` against the kinds the generation declares. Bound props are left to
/// `resolve`, which fails them on their own.
fn check_script_materials(plugin: bool, kinds: &std::collections::BTreeMap<String, Option<bevy_ts_material::layout::Bindings>>, nodes: &[desc::NodeDesc], errors: &mut Vec<ScriptError>) {
    for n in nodes {
        for c in n.components.iter().filter(|c| c.kind == desc::ComponentKind::Intrinsic && c.ty == resolve::MATERIAL && c.props.contains_key("kind")) {
            let Ok((kind, mut props)) = resolve::script_material(c) else { continue };
            if !plugin {
                errors.push(ScriptError::at("<material kind>: script materials need ScriptMaterialPlugin", c.source.clone()));
                continue;
            }
            props.retain(|_, v| desc::binding_id(v).is_none());
            if let Err(m) = bevy_ts_material::kinds::check_props(kinds, &kind, &props) {
                errors.push(ScriptError::at(format!("<material>: {m}"), c.source.clone()));
            }
        }
        check_script_materials(plugin, kinds, &n.children, errors);
    }
}

/// Checks a generation's scenes against its pending components. Touches nothing.
fn check(world: &mut World, input: &bevy_ts::plugin_api::SectionInput) -> Result<(), Vec<ScriptError>> {
    let t = std::time::Instant::now();
    let r = check_inner(world, input);
    world.resource_mut::<TsxTree>().timings.check_ms = t.elapsed().as_secs_f64() * 1e3;
    r
}

fn check_inner(world: &mut World, input: &bevy_ts::plugin_api::SectionInput) -> Result<(), Vec<ScriptError>> {
    let t = std::time::Instant::now();
    let section = parse(input.bytes).map_err(|e| vec![e])?;
    world.resource_mut::<TsxTree>().timings.parse_ms = t.elapsed().as_secs_f64() * 1e3;
    // Overlays layer over the new generation's scenes: one that no longer fits fails it.
    let overlaid = with_overlays(world, &section);
    if !section.errors.is_empty() {
        return Err(section.errors);
    }
    let mut errors = resolve::check_roots(&overlaid.roots, input.components, world.resource::<NativeLayouts>());
    if !intrinsics::available(world)
        && let Some(at) = section.roots.iter().find_map(|r| uses_assets(&r.nodes))
    {
        errors.push(ScriptError::at("<mesh> and <material> need the render plugins (no Assets<Mesh>)", at));
    }
    // The generation's kinds, read from its own `materials` section: sections are checked in the
    // order JS wrote them, and a scene may run before the first `material()`.
    // A malformed section is reported by its own check.
    if let Some(kinds) = bevy_ts_material::kinds::section_kinds(input.sections) {
        let plugin = world.contains_resource::<bevy_ts_material::MaterialKinds>();
        for r in &overlaid.roots {
            check_script_materials(plugin, &kinds, &r.nodes, &mut errors);
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // Applied after commit; parsing a large section twice would double a reload's cost.
    world.insert_resource(Checked { generation: input.generation, section });
    Ok(())
}

/// The section a generation's check parsed, kept for its apply step.
#[derive(Resource)]
struct Checked {
    generation: u64,
    section: SceneSection,
}

/// The section with each root's overlay layered over it.
fn with_overlays(world: &World, section: &SceneSection) -> SceneSection {
    let tree = world.resource::<TsxTree>();
    let mut s = section.clone();
    for r in &mut s.roots {
        if let Some(o) = tree.overlays.get(&r.name) {
            overlay::merge(&mut r.nodes, o);
        }
    }
    s
}

/// Reconciles every root of an applied generation, and unmounts roots it no longer declares.
pub fn reconcile_generation(world: &mut World, generation: u64, section: &SceneSection) -> ReconcileStats {
    // Ids and layouts may have changed with the generation.
    world.resource_mut::<reconcile::TemplateCache>().clear();
    // The old generation's closures died with it: its bindings, refs, slots and subscriptions
    // go, and the new generation's register as its roots reconcile.
    {
        let mut tree = world.resource_mut::<TsxTree>();
        let r = &mut tree.reactive;
        r.targets.clear();
        r.refs.clear();
        r.new_refs.clear();
        r.slots.clear();
        r.subs.clear();
        r.sites.clear();
        r.new_instances.clear();
    }
    let mut total = ReconcileStats::default();
    {
        let mut tree = world.resource_mut::<TsxTree>();
        tree.scenes = section.roots.iter().map(|r| (r.name.clone(), r.clone())).collect();
    }
    let section = &with_overlays(world, section);
    for r in &section.roots {
        let t = std::time::Instant::now();
        let resolved = resolve_tree(world, &r.name, &r.nodes, (&[], &[]));
        world.resource_mut::<TsxTree>().timings.resolve_ms = t.elapsed().as_secs_f64() * 1e3;
        match resolved {
            Ok(nodes) => {
                let t = std::time::Instant::now();
                total.add(&apply_root(world, &r.name, r.location.clone(), &nodes, None, reconcile::Prune::Root));
                world.resource_mut::<TsxTree>().timings.reconcile_ms = t.elapsed().as_secs_f64() * 1e3;
            }
            // Checked with the generation; only a bug lands here.
            Err(e) => world.resource_mut::<ScriptLog>().push(LogLevel::Error, format!("tsx: {}", e.message), e.location, generation),
        }
    }
    let gone: Vec<String> = world
        .resource::<TsxTree>()
        .roots
        .keys()
        .filter(|k| {
            let tree = world.resource::<TsxTree>();
            k.as_str() != RUNTIME_ROOT && !tree.external.contains(*k) && !section.roots.iter().any(|r| &r.name == *k)
        })
        .cloned()
        .collect();
    for name in gone {
        total.add(&unmount_root(world, &name));
    }
    let mut tree = world.resource_mut::<TsxTree>();
    tree.journal.close();
    tree.last = total.clone();
    tree.reactive.sweep = !tree.reactive.slots.is_empty();
    if world.get_resource::<bevy_ts::runtime::RuntimeInfo>().is_some_and(|r| r.dev) {
        warn_two_writers(world, generation);
    }
    total
}

/// Sets the overlay of `app.scene` root `scene` (levels' T4): `nodes` layer over the scene by key
/// path, their props overriding (provenance `override`, with their locations). The scene
/// reconciles with it now when the live generation has it, else with the next generation. An
/// overlay that does not resolve against the scene is refused, and nothing changes.
pub fn set_overlay(world: &mut World, scene: &str, mut nodes: Vec<desc::NodeDesc>) -> Result<ReconcileStats, Vec<ScriptError>> {
    desc::normalize_sources(&mut nodes);
    let Some(root) = world.resource::<TsxTree>().scenes.get(scene).cloned() else {
        world.resource_mut::<TsxTree>().overlays.insert(scene.to_string(), nodes);
        return Ok(ReconcileStats::default());
    };
    let mut merged = root.nodes.clone();
    overlay::merge(&mut merged, &nodes);
    let mut errors = Vec::new();
    resolve::check_nodes(&merged, world.resource::<bevy_ts::components::TsComponents>(), world.resource::<NativeLayouts>(), &mut errors);
    if !errors.is_empty() {
        return Err(errors);
    }
    let resolved = resolve_tree(world, scene, &merged, (&[], &[])).map_err(|e| vec![e])?;
    world.resource_mut::<TsxTree>().overlays.insert(scene.to_string(), nodes);
    let stats = apply_root(world, scene, root.location, &resolved, None, reconcile::Prune::Root);
    world.resource_mut::<TsxTree>().journal.close();
    Ok(stats)
}

/// Removes a scene's overlay; the scene reconciles without it.
pub fn clear_overlay(world: &mut World, scene: &str) -> ReconcileStats {
    let had = world.resource_mut::<TsxTree>().overlays.remove(scene).is_some();
    let root = world.resource::<TsxTree>().scenes.get(scene).cloned();
    let (true, Some(root)) = (had, root) else { return ReconcileStats::default() };
    match resolve_tree(world, scene, &root.nodes, (&[], &[])) {
        Ok(resolved) => {
            let stats = apply_root(world, scene, root.location, &resolved, None, reconcile::Prune::Root);
            world.resource_mut::<TsxTree>().journal.close();
            stats
        }
        Err(_) => ReconcileStats::default(),
    }
}

/// The sidecar level of an `app.scene` root: its `level` option, or `levels/<scene>.bsn`.
/// None when the live generation has no such scene.
pub fn scene_level(world: &World, scene: &str) -> Option<String> {
    world.resource::<TsxTree>().scenes.get(scene).map(|r| r.level.clone().unwrap_or_else(|| overlay::default_level(scene)))
}

/// Reconciles a root a Rust plugin describes (levels' T2): a description built in Rust, partly
/// expanded in JS, under the same rules as an `app.scene` root. A resolve error touches nothing.
/// Call it from an exclusive system or a section apply step, at a frame boundary.
pub fn reconcile_external(world: &mut World, name: &str, location: Option<bevy_ts::backend::SourceLocation>, nodes: &[desc::NodeDesc]) -> Result<ReconcileStats, ScriptError> {
    let resolved = resolve_tree(world, name, nodes, (&[], &[]))?;
    world.resource_mut::<TsxTree>().external.insert(name.to_string());
    let stats = apply_root(world, name, location, &resolved, None, reconcile::Prune::Root);
    world.resource_mut::<TsxTree>().journal.close();
    Ok(stats)
}

/// Moves every node of `root` under key path `from` to `to`, keeping its entity and state: a
/// rename the next reconcile then finds in place (levels' writer `rename`). Returns how many
/// nodes moved; none when `to` is taken.
pub fn rekey(world: &mut World, root: &str, from: &[String], to: &[String]) -> usize {
    let moves: Vec<(Entity, u64, Vec<String>)> = {
        let mut q = world.query::<(Entity, &TsxNode)>();
        q.iter(world)
            .filter(|(_, n)| n.root == root && n.key_path.starts_with(from))
            .map(|(e, n)| {
                let mut path = to.to_vec();
                path.extend_from_slice(&n.key_path[from.len()..]);
                (e, n.id, path)
            })
            .collect()
    };
    let tree = world.resource::<TsxTree>();
    if moves.iter().any(|(_, _, p)| tree.entities.contains_key(&desc::node_id(root, p)) && !moves.iter().any(|(_, old, _)| *old == desc::node_id(root, p))) {
        return 0;
    }
    let mut ids = std::collections::HashMap::new();
    for (e, old, path) in &moves {
        let id = desc::node_id(root, path);
        ids.insert(*old, id);
        let mut n = world.get_mut::<TsxNode>(*e).expect("queried");
        n.id = id;
        n.key_path = path.clone();
    }
    let mut tree = world.resource_mut::<TsxTree>();
    let mut moved = Vec::new();
    for (old, new) in &ids {
        if let Some(e) = tree.entities.remove(old) {
            moved.push((*new, e));
        }
    }
    for (id, e) in moved {
        tree.entities.insert(id, e);
    }
    if let Some(state) = tree.roots.get_mut(root) {
        state.nodes = state.nodes.iter().map(|id| ids.get(id).copied().unwrap_or(*id)).collect();
    }
    for t in tree.reactive.targets.values_mut() {
        if let Some(n) = ids.get(&t.node) {
            t.node = *n;
        }
    }
    for n in tree.reactive.refs.values_mut() {
        if let Some(id) = ids.get(n) {
            *n = *id;
        }
    }
    moves.len()
}

/// Unmounts an external root and forgets it.
pub fn unmount_external(world: &mut World, name: &str) -> ReconcileStats {
    world.resource_mut::<TsxTree>().external.remove(name);
    let stats = unmount_root(world, name);
    world.resource_mut::<TsxTree>().journal.close();
    stats
}

/// A bound field has one writer, its binding. Under dev, a TS system whose `write` set holds the
/// bound component is named with the binding's site, once per site and generation.
fn warn_two_writers(world: &mut World, generation: u64) {
    let tree = world.resource::<TsxTree>();
    let mut sites = std::collections::BTreeMap::new();
    for t in tree.reactive.targets.values() {
        if ui::UiKind::from_tag(&t.component).is_some() {
            continue;
        }
        let Some(source) = tree.entities.get(&t.node).and_then(|e| world.get::<TsxNode>(*e)).and_then(|n| n.components.get(&t.component)).map(|c| c.source.clone()) else { continue };
        sites.entry((t.component.clone(), t.prop.clone())).or_insert(source);
    }
    let mut warnings = Vec::new();
    for s in world.resource::<bevy_ts::systems::TsSystems>().iter() {
        for ((component, prop), source) in &sites {
            if s.decl.access().1.iter().any(|w| w == component) {
                warnings.push((format!("system {} writes {component}, whose {prop} is bound in TSX: a bound field has one writer, its binding", s.decl.name), source.clone()));
            }
        }
    }
    let mut log = world.resource_mut::<ScriptLog>();
    for (m, at) in warnings {
        log.push(LogLevel::Warn, m, at, generation);
    }
}

fn apply(world: &mut World, applied: &bevy_ts::plugin_api::AppliedSection) {
    let t = std::time::Instant::now();
    apply_inner(world, applied);
    world.resource_mut::<TsxTree>().timings.apply_ms = t.elapsed().as_secs_f64() * 1e3;
}

fn apply_inner(world: &mut World, applied: &bevy_ts::plugin_api::AppliedSection) {
    // A generation without the section declares no scenes: every scene root unmounts.
    let checked = world.remove_resource::<Checked>().filter(|c| c.generation == applied.generation);
    let section = if let Some(c) = checked {
        c.section
    } else if applied.present {
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
        app.add_ts_command_kind(host::FLUSH, host::flush);
        // The editor methods, when the app serves the protocol already; otherwise call
        // `protocol::add_methods` after adding it.
        if app.world().contains_resource::<bevy::remote::RemoteMethods>() {
            protocol::add_methods(app);
        }
        app.add_systems(
            PostUpdate,
            host::frame.after(bevy_ts::BevyTsSet::PostUpdate).before(bevy::ui::UiSystems::Prepare),
        );
        app.add_ts_command_kind(mount::UNMOUNT, mount::unmount);
    }
}
