//! The thrusters example's Rust: Rapier physics and one Rust system. TS sets `Thrust`; Rust turns
//! it into a force each fixed step. Both read and write the same components in one world.

use bevy_rapier3d::prelude::*;
use bevy::prelude::*;

/// How hard a body pushes along `dir`. Plain data, so TS systems see it as a column.
#[derive(Component, Reflect, Default, Clone, Copy)]
#[reflect(Component, Default)]
pub struct Thrust {
    pub power: f32,
    pub dir: Vec3,
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule())
            .register_type::<Thrust>()
            .add_systems(Startup, spawn)
            .add_systems(FixedUpdate, apply_thrust)
            .add_systems(Update, report);
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("ground"), Transform::from_xyz(0.0, -0.5, 0.0), RigidBody::Fixed, Collider::cuboid(20.0, 0.5, 20.0)));
    commands.spawn((
        Name::new("rocket"),
        Transform::from_xyz(0.0, 2.0, 0.0),
        RigidBody::Dynamic,
        Collider::cuboid(0.5, 0.5, 0.5),
        ExternalForce::default(),
        Velocity::default(),
        Thrust { power: 0.0, dir: Vec3::Y },
    ));
}

fn apply_thrust(mut q: Query<(&Thrust, &mut ExternalForce)>) {
    for (t, mut f) in &mut q {
        f.force = t.dir * t.power;
    }
}

fn report(q: Query<(&Name, &Transform, &Thrust)>, time: Res<Time>, mut last: Local<f32>) {
    if time.elapsed_secs() - *last >= 1.0 {
        *last = time.elapsed_secs();
        for (name, t, thrust) in &q {
            info!("thrusters: {name} y={:.2} power={:.1}", t.translation.y, thrust.power);
        }
    }
}
