//! Components and world setup shared by the bindings bench.

pub use bevy_ecs::component::ComponentId;
use bevy_ecs::prelude::*;

/// What a binding reads: `alarm().on`.
#[derive(Component, Clone, Copy)]
#[repr(C)]
pub struct Alarm {
    pub on: f32,
    pub level: f32,
}

/// What a binding writes: `PointLight.intensity`.
#[derive(Component, Clone, Copy)]
#[repr(C)]
pub struct Light {
    pub intensity: f32,
    pub range: f32,
    pub radius: f32,
    pub pad: f32,
}

/// Ballast so a row is the size of a real entity (Transform + GlobalTransform are 112 bytes).
#[derive(Component, Clone, Copy)]
#[repr(C)]
pub struct Pad(pub [f32; 28]);

pub struct Setup {
    pub world: World,
    pub entities: Vec<Entity>,
    /// A random order over `entities`, so k random ones are the first k.
    pub shuffled: Vec<Entity>,
    pub alarm_id: ComponentId,
    pub light_id: ComponentId,
}

pub fn setup(n: usize) -> Setup {
    let mut world = World::new();
    let mut seed: u32 = 0x9e37_79b9;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / 4294967296.0
    };
    let entities: Vec<Entity> = world
        .spawn_batch((0..n).map(|_| {
            let on = if rnd() < 0.5 { 1.0 } else { 0.0 };
            (
                Alarm { on, level: rnd() },
                Light { intensity: if on > 0.0 { 8.0 } else { 2.0 }, range: 20.0, radius: 0.0, pad: 0.0 },
                Pad([0.0; 28]),
            )
        }))
        .collect();
    let mut shuffled = entities.clone();
    for i in (1..n).rev() {
        let j = (rnd() * (i as f32 + 1.0)) as usize;
        shuffled.swap(i, j.min(i));
    }
    let alarm_id = world.register_component::<Alarm>();
    let light_id = world.register_component::<Light>();
    Setup { world, entities, shuffled, alarm_id, light_id }
}
