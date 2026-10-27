//! Pixel-art 2D (openspec change `pixel-art-2d`). [`YSort`] draws an entity by its base y within a
//! layer. It owns `Transform` z: the pass in [`Pixel2dSystems`] writes it before Bevy's transform
//! propagation, so `GlobalTransform` stays Bevy's own and every reader of it (picking, visibility,
//! extraction) sees one value. Nothing here writes `GlobalTransform`.

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_ts::log::{LogLevel, ScriptLog};
use std::collections::{HashMap, HashSet};

/// The largest `YSort.layer`, either side of 0: under 128 an f32's step is finer than one pixel's
/// share of z, so a one-pixel move of y always changes z.
pub const MAX_LAYER: f32 = 127.0;
/// The base y past which y-sorted sprites tie, either side of 0: the y term stays inside ±0.5, so
/// layers a whole unit apart never mix.
pub const MAX_BASE: f32 = 49_999.0;
/// z per pixel of base y.
pub const Z_PER_PX: f64 = 1e-5;

/// Draws the entity by its base y within `layer`: lower on screen is in front. Its `Transform` z is
/// `layer - clamp(y + offset, ±49,999) × 10⁻⁵`, `y` its own `Transform` y (in its parent's space
/// when it has one). `offset` moves the base from the origin to the feet. It owns `Transform` z: a
/// write to z is overwritten the same frame; write `layer` instead.
#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
#[require(Transform)]
pub struct YSort {
    /// The whole layer the entity draws in, from -127 to 127.
    pub layer: f32,
    /// Added to y for the base, in pixels: `-8` puts it 8 px below the origin.
    pub offset: f32,
}

/// The z a y-sorted entity at `y` draws at, for a `layer` within ±[`MAX_LAYER`].
pub fn y_sort_z(layer: f32, y: f32, offset: f32) -> f32 {
    let base = (y + offset).clamp(-MAX_BASE, MAX_BASE);
    (layer as f64 - base as f64 * Z_PER_PX) as f32
}

/// The pass that writes what [`YSort`] owns, in `PostUpdate` after the TSX reconcile and before
/// `TransformSystems::Propagate`. It writes a field only when its value changes, so a still entity
/// is not propagated again.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pixel2dSystems;

/// [`YSort`] and its pass. The engine adds it; a Rust app adds it beside `DefaultPlugins`.
pub struct Pixel2dPlugin;

impl Plugin for Pixel2dPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<YSort>()
            .configure_sets(PostUpdate, Pixel2dSystems.before(TransformSystems::Propagate))
            .add_systems(PostUpdate, y_sort.in_set(Pixel2dSystems).run_if(any_with_component::<YSort>));
    }
}

/// What the pass has said, so each problem is logged once per entity and value.
#[derive(Default)]
struct Told {
    /// An entity's out-of-range `layer`, by its bits.
    layer: HashMap<Entity, u32>,
    /// Entities whose base went past the clamp.
    clamped: HashSet<Entity>,
}

fn log(log: &mut Option<ResMut<ScriptLog>>, generations: &Option<Res<bevy_ts::generation::Generations>>, level: LogLevel, message: String) {
    match log {
        Some(l) => l.push(level, message, None, generations.as_ref().map_or(0, |g| g.live)),
        None => warn!("{message}"),
    }
}

fn named(e: Entity, name: Option<&Name>) -> String {
    match name {
        Some(n) => format!("entity {e} \"{n}\""),
        None => format!("entity {e}"),
    }
}

fn y_sort(
    mut sorted: Query<(Entity, &YSort, &mut Transform, Option<&Name>)>,
    mut script_log: Option<ResMut<ScriptLog>>,
    generations: Option<Res<bevy_ts::generation::Generations>>,
    mut told: Local<Told>,
) {
    for (e, y, mut t, name) in &mut sorted {
        if !(-MAX_LAYER..=MAX_LAYER).contains(&y.layer) {
            if told.layer.insert(e, y.layer.to_bits()) != Some(y.layer.to_bits()) {
                let message = format!("{}: YSort.layer {} is out of range: a layer is from -127 to 127", named(e, name), y.layer);
                log(&mut script_log, &generations, LogLevel::Error, message);
            }
            continue;
        }
        told.layer.remove(&e);
        let base = t.translation.y + y.offset;
        if base.abs() > MAX_BASE && told.clamped.insert(e) {
            let message = format!("{}: YSort base y {base} is past ±49999, where y-sorted sprites tie and draw in no set order", named(e, name));
            log(&mut script_log, &generations, LogLevel::Warn, message);
        }
        let z = y_sort_z(y.layer, t.translation.y, y.offset);
        if t.translation.z != z {
            t.translation.z = z;
        }
    }
}
