//! The bevy-ts port of `bevy_rapier3d` 0.37, adopted with `"native": { "ports": ["rapier3d"] }`.
//!
//! Reflection already makes Rapier's plain-data components (`Velocity`, `ExternalImpulse`,
//! `RigidBody`, `LockedAxes`, ...) columns in TS systems with no code here. The port adds what
//! reflection cannot: the plugin built in `FixedUpdate`, where TS physics gameplay runs, and
//! Rapier's sets under stable names. Exported params (`ReadRapierContext.castRay`), `Collider`
//! constructors and flattened `CollisionEvent`s follow with exported system params.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use bevy_ts_native::BevyTsNativeExt;

/// Adds Rapier in `FixedUpdate` and exposes `rapier:sync-backend`, `rapier:step` and
/// `rapier:writeback`.
#[derive(Default)]
pub struct PortPlugin;

impl Plugin for PortPlugin {
    fn build(&self, app: &mut App) {
        // Rapier's default schedule is PostUpdate; TS physics gameplay belongs in FixedUpdate.
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule())
            .ts_set("rapier:sync-backend", FixedUpdate, PhysicsSet::SyncBackend)
            .ts_set("rapier:step", FixedUpdate, PhysicsSet::StepSimulation)
            .ts_set("rapier:writeback", FixedUpdate, PhysicsSet::Writeback);
    }
}
