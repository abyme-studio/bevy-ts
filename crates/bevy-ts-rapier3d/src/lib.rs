//! The bevy-ts port of `bevy_rapier3d` 0.37, adopted with `"native": { "ports": ["rapier3d"] }`.
//!
//! Reflection already makes Rapier's plain-data components (`Velocity`, `ExternalImpulse`,
//! `RigidBody`, `LockedAxes`, ...) columns in TS systems with no code here. The port adds what
//! reflection cannot:
//!
//! - the plugin built in `FixedUpdate`, where TS physics gameplay runs;
//! - Rapier's sets under stable names: `rapier:sync-backend`, `rapier:step`, `rapier:writeback`;
//! - `ReadRapierContext`, an exported param TS systems take to cast rays, one host call per ray
//!   or per batch;
//! - `bevy-ts/rapier`, its TS module: `Collider.cuboid(commands, e, hx, hy, hz)` and friends
//!   (Rapier's `Collider` has no `Reflect`; they insert it through the `rapier/collider`
//!   command), and `CollisionEvent` flattened to `{ kind, a, b, sensor }`, which a TS system
//!   reads with `MessageReader<CollisionEvent>`.

use bevy::ecs::system::{SystemParam, SystemState};
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use bevy_ts_native::prelude::*;

/// Adds Rapier in `FixedUpdate`, exposes its sets and exports `ReadRapierContext`.
#[derive(Default)]
pub struct PortPlugin;

impl Plugin for PortPlugin {
    fn build(&self, app: &mut App) {
        // Rapier's default schedule is PostUpdate; TS physics gameplay belongs in FixedUpdate.
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule())
            .ts_set("rapier:sync-backend", FixedUpdate, PhysicsSet::SyncBackend)
            .ts_set("rapier:step", FixedUpdate, PhysicsSet::StepSimulation)
            .ts_set("rapier:writeback", FixedUpdate, PhysicsSet::Writeback)
            .ts_param::<RapierQuery<'static, 'static>>()
            .ts_module("bevy-ts/rapier", include_str!("../ts/rapier.js"), include_str!("../ts/rapier.d.ts"))
            .add_ts_command_kind("rapier/collider", insert_collider)
            .add_systems(FixedUpdate, forward_collisions.after(PhysicsSet::Writeback));
    }
}

/// Copies Rapier's `CollisionEvent`s into the TS message of the same name, flattened, when the
/// live generation reads it.
fn forward_collisions(world: &mut World, reader: &mut SystemState<MessageReader<CollisionEvent>>) {
    let events: Vec<CollisionEvent> = reader.get_mut(world).expect("messages are registered").read().copied().collect();
    for ev in events {
        let (kind, a, b, flags) = match ev {
            CollisionEvent::Started(a, b, f) => ("Started", a, b, f),
            CollisionEvent::Stopped(a, b, f) => ("Stopped", a, b, f),
        };
        let values = serde_json::json!({
            "kind": kind,
            "a": a.to_bits(),
            "b": b.to_bits(),
            "sensor": flags.contains(bevy_rapier3d::rapier::geometry::CollisionEventFlags::SENSOR),
        });
        let name = "bevy_rapier3d::pipeline::events::CollisionEvent";
        match bevy_ts_native::bevy_ts::plugin_api::write_message(world, name, values.as_object().unwrap()) {
            Ok(true) => {}
            // TS does not read collisions this generation.
            Ok(false) => return,
            Err(e) => {
                warn!("bevy-ts-rapier3d: {e}");
                return;
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ColliderCommand {
    entity: u64,
    #[serde(default)]
    ball: Option<f32>,
    #[serde(default)]
    cuboid: Option<[f32; 3]>,
    #[serde(default)]
    capsule_y: Option<[f32; 2]>,
    #[serde(default)]
    sensor: bool,
}

/// `rapier/collider`: inserts a `Collider` (and `Sensor`) on an entity, from JSON.
fn insert_collider(world: &mut World, bytes: &[u8]) -> Result<(), String> {
    let c: ColliderCommand = serde_json::from_slice(bytes).map_err(|e| format!("rapier/collider: {e}"))?;
    let collider = match (c.ball, c.cuboid, c.capsule_y) {
        (Some(r), None, None) => Collider::ball(r),
        (None, Some([x, y, z]), None) => Collider::cuboid(x, y, z),
        (None, None, Some([h, r])) => Collider::capsule_y(h, r),
        _ => return Err("rapier/collider: give exactly one of ball, cuboid, capsuleY".into()),
    };
    let entity = Entity::try_from_bits(c.entity).ok_or_else(|| format!("rapier/collider: {} is not an entity", c.entity))?;
    let mut e = world.get_entity_mut(entity).map_err(|e| format!("rapier/collider: {e}"))?;
    // TS reads collisions as messages, so every collider it makes reports them.
    e.insert((collider, ActiveEvents::COLLISION_EVENTS));
    if c.sensor {
        e.insert(Sensor);
    }
    Ok(())
}

/// Rapier's read-only context, as TS takes it: `rapier: ReadRapierContext`.
#[derive(SystemParam)]
pub struct RapierQuery<'w, 's> {
    ctx: ReadRapierContext<'w, 's>,
}

/// A ray's hit: the collider's entity and the distance along the ray, in multiples of `dir`.
#[derive(HostRecord)]
pub struct RayHit {
    pub entity: Entity,
    pub toi: f32,
}

/// Which colliders a cast ignores.
#[derive(HostRecord, Default)]
pub struct RayFilter {
    pub exclude_rigid_body: Option<Entity>,
    pub exclude_collider: Option<Entity>,
    pub exclude_sensors: bool,
}

impl RayFilter {
    fn query(&self) -> QueryFilter<'static> {
        let mut f = QueryFilter::default();
        if let Some(e) = self.exclude_rigid_body {
            f = f.exclude_rigid_body(e);
        }
        if let Some(e) = self.exclude_collider {
            f = f.exclude_collider(e);
        }
        if self.exclude_sensors {
            f = f.exclude_sensors();
        }
        f
    }
}

#[export(ts = "ReadRapierContext", module = "bevy-ts/rapier")]
impl RapierQuery<'_, '_> {
    /// The first collider along a ray within `max_toi`, or `undefined`.
    pub fn cast_ray(&self, origin: Vec3, dir: Vec3, max_toi: f32, solid: bool, filter: RayFilter) -> Result<Option<RayHit>, String> {
        let ctx = self.ctx.single().map_err(|e| e.to_string())?;
        Ok(ctx.cast_ray(origin, dir, max_toi, solid, filter.query()).map(|(entity, toi)| RayHit { entity, toi }))
    }

    /// Many rays in one call: `rays` holds origin then direction, six floats per ray; `toi` and
    /// `entity` receive one result per ray (`-1` and `0` for a miss). Returns the hit count.
    pub fn cast_rays(&self, rays: &[f32], max_toi: f32, solid: bool, filter: RayFilter, toi: &mut [f32], entity: &mut [f64]) -> Result<u32, String> {
        let ctx = self.ctx.single().map_err(|e| e.to_string())?;
        let q = filter.query();
        let mut hits = 0;
        for (i, r) in rays.chunks_exact(6).enumerate() {
            let hit = ctx.cast_ray(Vec3::new(r[0], r[1], r[2]), Vec3::new(r[3], r[4], r[5]), max_toi, solid, q);
            let (t, e) = hit.map_or((-1.0, 0.0), |(e, t)| (t, e.to_bits() as f64));
            if let Some(slot) = toi.get_mut(i) {
                *slot = t;
            }
            if let Some(slot) = entity.get_mut(i) {
                *slot = e;
            }
            hits += hit.is_some() as u32;
        }
        Ok(hits)
    }
}
