//! `BEVY_TS_ORDER_DUMP=1`: what a shot's frame drew, and in what order, as text beside the shot
//! (`<shot>.order.txt`), so a run that matches and one that flips can be diffed. Without the
//! variable none of it is in the app. One line per fact; a line's entity ids, which follow load
//! timing, come after a tab, so `cut -f1` drops them before a diff.
//!
//! The main world writes the cameras and every drawn entity's materials after the shot is asked
//! for (in `RemoteLast` when there is a [`RemotePlugin`], else `Last`); the render world, after
//! its phases are sorted, writes each camera view's phases in draw order. Items are named by their
//! [`StableKey`] (from [`StableBinOrder`], so only under [`FixedStep`](crate::screenshot::FixedStep)):
//! `-` is an item with no key, a `*` after a key one that another item in the phase shares.

use crate::screenshot::{StableKey, materials};
use bevy::camera::primitives::Aabb;
use bevy::core_pipeline::core_3d::{AlphaMask3d, Opaque3d, Transparent3d};
use bevy::core_pipeline::prepass::{AlphaMask3dPrepass, Opaque3dPrepass};
use bevy::ecs::component::ComponentId;
use bevy::prelude::*;
use bevy::remote::{RemoteLast, RemotePlugin, RemoteSystems};
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_phase::{BinnedPhaseItem, StableBinOrder, ViewBinnedRenderPhases, ViewSortedRenderPhases};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::{ExtractedView, RetainedViewEntity};
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Asks for an order dump with each shot. In the app only under `BEVY_TS_ORDER_DUMP=1`
/// ([`OrderDump::from_env`]); a test inserts it itself.
#[derive(Resource, Default)]
pub struct OrderDump {
    /// The dump of the shot taken this frame, if one was.
    asked: Option<PathBuf>,
    /// The main world's lines for it.
    lines: Vec<String>,
}

impl OrderDump {
    /// An `OrderDump` when `BEVY_TS_ORDER_DUMP` is `1`.
    pub fn from_env() -> Option<Self> {
        std::env::var_os("BEVY_TS_ORDER_DUMP").is_some_and(|v| v == "1").then(Self::default)
    }

    /// Where the dump of a shot at `shot` goes: `<stem>.order.txt` beside it.
    pub fn path(shot: &Path) -> PathBuf {
        shot.with_extension("order.txt")
    }
}

/// Asks for the dump of the shot at `shot`, taken this frame, when there is an [`OrderDump`].
pub(crate) fn ask(world: &mut World, shot: &Path) {
    if let Some(mut dump) = world.get_resource_mut::<OrderDump>() {
        dump.asked = Some(OrderDump::path(shot));
    }
}

/// The dump of the frame being rendered, in the render world.
#[derive(Resource)]
struct DumpNow {
    path: PathBuf,
    lines: Vec<String>,
}

/// Writes [`OrderDump`]'s files. [`ReadyPlugin`](crate::screenshot::ReadyPlugin) adds it; it
/// inserts [`OrderDump::from_env`]'s resource when the variable is set.
pub(crate) struct OrderDumpPlugin;

impl Plugin for OrderDumpPlugin {
    fn build(&self, app: &mut App) {
        if let Some(dump) = OrderDump::from_env() {
            app.insert_resource(dump);
        }
    }

    /// Adds the dump's systems only when there is an [`OrderDump`], so a run without one pays nothing.
    fn finish(&self, app: &mut App) {
        if !app.world().contains_resource::<OrderDump>() {
            return;
        }
        app.add_systems(First, |mut dump: ResMut<OrderDump>| {
            dump.asked = None;
            dump.lines.clear();
        });
        // After the last ask: `bevy-ts/screenshot` captures in `RemoteLast`, after `Last`.
        let lines = main_lines.run_if(|dump: Res<OrderDump>| dump.asked.is_some());
        if app.is_plugin_added::<RemotePlugin>() {
            app.add_systems(RemoteLast, lines.after(RemoteSystems::ProcessRequests));
        } else {
            app.add_systems(Last, lines);
        }
        let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
        render
            .add_systems(ExtractSchedule, |dump: Extract<Res<OrderDump>>, mut commands: Commands| {
                if let Some(path) = dump.asked.clone() {
                    commands.insert_resource(DumpNow { path, lines: dump.lines.clone() });
                }
            })
            .add_systems(
                Render,
                render_lines.run_if(resource_exists::<DumpNow>).after(RenderSystems::PhaseSort).before(RenderSystems::Render),
            );
    }
}

/// An affine's 12 floats as bits, so two runs compare exactly.
fn bits(affine: bevy::math::Affine3A) -> String {
    let m = affine.to_cols_array();
    m.iter().map(|f| format!("{:08x}", f.to_bits())).collect::<Vec<_>>().join(" ")
}

/// The main world's lines: each active camera by order, then each entity with an `Aabb`, its key
/// and its materials' hashes ([`materials`]).
fn main_lines(world: &mut World, mut kinds: Local<HashMap<ComponentId, bool>>) {
    let mut lines = Vec::new();
    let mut cameras: Vec<(isize, String)> = world.query::<(&Camera, &GlobalTransform)>().iter(world).filter(|(c, _)| c.is_active).map(|(c, t)| (c.order, bits(t.affine()))).collect();
    cameras.sort();
    lines.extend(cameras.into_iter().map(|(order, t)| format!("camera order={order} transform={t}")));
    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    let entities: Vec<(Entity, Option<u64>)> = world.query_filtered::<(Entity, Option<&StableKey>), With<Aabb>>().iter(world).map(|(e, k)| (e, k.map(|k| k.0))).collect();
    let mut drawn: Vec<(String, Entity)> = Vec::new();
    for (e, key) in entities {
        let key = key.map_or("-".into(), |k| format!("{k:016x}"));
        let found = materials(world, &registry, world.entity(e), &mut kinds);
        if found.is_empty() {
            drawn.push((format!("entity key={key} material=-"), e));
        }
        drawn.extend(found.into_iter().map(|(ty, hash)| (format!("entity key={key} material={ty} {hash:016x}"), e)));
    }
    drawn.sort();
    lines.extend(drawn.into_iter().map(|(line, e)| format!("{line}\t# {e}")));
    world.resource_mut::<OrderDump>().lines = lines;
}

/// Each camera view's phases, after `PhaseSort`, under the main world's lines, written to the dump.
fn render_lines(world: &mut World) {
    let Some(DumpNow { path, mut lines }) = world.remove_resource::<DumpNow>() else { return };
    let mut views: Vec<(isize, String, RetainedViewEntity)> =
        world.query::<(&ExtractedView, &ExtractedCamera)>().iter(world).map(|(v, c)| (c.order, bits(v.world_from_view.affine()), v.retained_view_entity)).collect();
    views.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    let stable = world.get_resource::<StableBinOrder>();
    for (order, transform, view) in views {
        lines.push(format!("view order={order} transform={transform}"));
        binned::<Opaque3d>(world, stable, view, "Opaque3d", &mut lines);
        binned::<AlphaMask3d>(world, stable, view, "AlphaMask3d", &mut lines);
        binned::<Opaque3dPrepass>(world, stable, view, "Opaque3dPrepass", &mut lines);
        binned::<AlphaMask3dPrepass>(world, stable, view, "AlphaMask3dPrepass", &mut lines);
        if let Some(phase) = world.get_resource::<ViewSortedRenderPhases<Transparent3d>>().and_then(|p| p.get(&view)) {
            let items: Vec<(MainEntity, String)> = phase.items.values().map(|i| (i.entity.1, format!("distance={:08x}", i.distance.to_bits()))).collect();
            let keys = Keys::of(stable, items.iter().map(|(e, _)| *e));
            lines.push(format!("  Transparent3d items={}", items.len()));
            lines.extend(items.iter().map(|(e, d)| format!("    item {} {d}\t# {e:?}", keys.name(*e))));
        }
    }
    let text = lines.join("\n") + "\n";
    if let Err(e) = std::fs::write(&path, text) {
        error!("Cannot write the order dump {}: {e}", path.display());
    }
}

/// The keys of one phase's items, to name each and mark the ones two items share.
struct Keys<'a> {
    stable: Option<&'a StableBinOrder>,
    counts: HashMap<u64, usize>,
}

impl<'a> Keys<'a> {
    fn of(stable: Option<&'a StableBinOrder>, entities: impl Iterator<Item = MainEntity>) -> Self {
        let mut counts = HashMap::new();
        for e in entities {
            if let Some(k) = stable.and_then(|s| s.0.get(&e)) {
                *counts.entry(*k).or_insert(0) += 1;
            }
        }
        Self { stable, counts }
    }

    fn name(&self, e: MainEntity) -> String {
        match self.stable.and_then(|s| s.0.get(&e)) {
            Some(k) if self.counts.get(k).is_some_and(|n| *n > 1) => format!("{k:016x}*"),
            Some(k) => format!("{k:016x}"),
            None => "-".into(),
        }
    }

    fn names<'e>(&self, entities: impl Iterator<Item = &'e MainEntity>) -> (String, String) {
        let entities: Vec<MainEntity> = entities.copied().collect();
        let names = entities.iter().map(|e| self.name(*e)).collect::<Vec<_>>().join(" ");
        let ids = entities.iter().map(|e| format!("{e:?}")).collect::<Vec<_>>().join(" ");
        (names, ids)
    }
}

/// One binned phase of `view`: its batchable bins in draw order, each with its entities' keys;
/// its unbatchable bins, entities in the order they iterate; and how many non-mesh items it has.
fn binned<BPI: BinnedPhaseItem>(world: &World, stable: Option<&StableBinOrder>, view: RetainedViewEntity, name: &str, lines: &mut Vec<String>) {
    let Some(phase) = world.get_resource::<ViewBinnedRenderPhases<BPI>>().and_then(|p| p.get(&view)) else { return };
    let all = phase
        .batchable_meshes
        .values()
        .flat_map(|b| b.entities().keys().copied())
        .chain(phase.unbatchable_meshes.values().flat_map(|b| b.entities.keys().copied()));
    let keys = Keys::of(stable, all);
    let non_mesh: usize = phase.non_mesh_items.values().map(|b| b.entities.len()).sum();
    lines.push(format!(
        "  {name} bins={} unbatchable={} multidrawable={} non_mesh_items={non_mesh}",
        phase.batchable_meshes.len(),
        phase.unbatchable_meshes.len(),
        phase.multidrawable_meshes.len()
    ));
    for (i, bin) in phase.batchable_meshes.values().enumerate() {
        let (names, ids) = keys.names(bin.entities().keys());
        lines.push(format!("    bin {i}: {names}\t# {ids}"));
    }
    for (i, bin) in phase.unbatchable_meshes.values().enumerate() {
        let (names, ids) = keys.names(bin.entities.keys());
        lines.push(format!("    unbatchable {i}: {names}\t# {ids}"));
    }
}
