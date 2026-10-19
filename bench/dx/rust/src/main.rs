//! The Rust Bevy 0.20 side of docs/research/dx.md, one section per case, compiled with
//! `cargo check` so the comparison is against real Bevy, not remembered Bevy.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

// ---------- Declarations ----------

#[derive(Component, Default, Clone)]
struct Enemy {
    speed: f32,
}
#[derive(Component, Default, Clone)]
struct Health {
    hp: f32,
    max: f32,
}
#[derive(Component)]
struct Player;
#[derive(Component, Default, Clone)]
struct Alive;
#[derive(Resource, Default)]
struct Score {
    value: u32,
}

// ---------- 1. A system with two queries and a resource ----------

fn chase(
    time: Res<Time>,
    player: Single<&Transform, With<Player>>,
    mut enemies: Query<(&Enemy, &mut Transform), (With<Alive>, Without<Player>)>,
) {
    for (enemy, mut t) in &mut enemies {
        let dir = (player.translation - t.translation).normalize_or_zero();
        t.translation += dir * enemy.speed * time.delta_secs();
    }
}

// Two queries over Transform that may alias need Without<> or a ParamSet.
fn separate(mut set: ParamSet<(Query<&mut Transform, With<Enemy>>, Query<(Entity, &Transform), With<Enemy>>)>) {
    let all: Vec<(Entity, Vec3)> = set.p1().iter().map(|(e, t)| (e, t.translation)).collect();
    for mut t in &mut set.p0() {
        if all.iter().filter(|(_, p)| p.distance(t.translation) < 1.0).count() > 1 {
            t.translation.y += 0.01;
        }
    }
}

// ---------- 2. Ordering, run conditions, states ----------

#[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
enum Game {
    #[default]
    Menu,
    Playing,
    Paused,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct EnemySet;

fn spawn_wave(mut commands: Commands, mut wave: Local<u32>) {
    *wave += 1;
    for i in 0..*wave {
        commands.spawn((Enemy { speed: 2.0 + *wave as f32 }, Transform::from_xyz(i as f32, 0.0, -10.0), Alive));
    }
}

fn pause(mut next: ResMut<NextState<Game>>) {
    next.set(Game::Paused);
}

// ---------- 3. A message and an observer ----------

#[derive(Message)]
struct Damage {
    target: Entity,
    amount: f32,
}

#[derive(EntityEvent)]
struct Explode {
    entity: Entity,
    radius: f32,
}

fn apply_damage(
    mut damage: MessageReader<Damage>,
    mut q: Query<&mut Health>,
    mut score: ResMut<Score>,
    mut commands: Commands,
) {
    for d in damage.read() {
        let Ok(mut h) = q.get_mut(d.target) else { continue };
        h.hp -= d.amount;
        if h.hp <= 0.0 {
            score.value += 1;
            commands.trigger(Explode { entity: d.target, radius: 3.0 });
        }
    }
}

fn on_explode(
    explode: On<Explode>,
    q: Query<(Entity, &Transform), With<Enemy>>,
    mut damage: MessageWriter<Damage>,
    mut commands: Commands,
) {
    let Ok((_, at)) = q.get(explode.entity) else { return };
    for (e, t) in &q {
        if t.translation.distance(at.translation) < explode.radius {
            damage.write(Damage { target: e, amount: 50.0 });
        }
    }
    commands.entity(explode.entity).despawn();
}

fn on_health_added(add: On<Add<Health>>, mut q: Query<&mut Health>) {
    if let Ok(mut h) = q.get_mut(add.entity) {
        h.hp = h.max;
    }
}

// ---------- 4. A plugin ----------

pub struct EnemyPlugin;
impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>()
            .add_message::<Damage>()
            .init_state::<Game>()
            .add_systems(OnEnter(Game::Playing), spawn_wave)
            .add_systems(
                Update,
                (
                    (spawn_wave, chase, separate).chain().in_set(EnemySet).run_if(in_state(Game::Playing)),
                    apply_damage.after(EnemySet),
                    pause.run_if(in_state(Game::Playing)),
                ),
            )
            .add_observer(on_explode)
            .add_observer(on_health_added);
    }
}

// ---------- 5. Spawning a prefab ----------

fn grunt(at: Vec3, speed: f32) -> impl Scene {
    bsn! {
        Transform { translation: at }
        Enemy { speed: speed }
        Health { hp: 100.0, max: 100.0 }
        Alive
        RigidBody::Dynamic
        Collider::cuboid(0.5, 0.5, 0.5)
        on(|click: On<PointerClick>, mut commands: Commands| {
            commands.trigger(Explode { entity: click.entity, radius: 3.0 });
        })
    }
}

fn spawn_grunt(mut commands: Commands) {
    commands.spawn_scene(grunt(vec3(0.0, 0.0, -12.0), 3.0));
}

// ---------- 6. A UI HUD ----------

#[derive(Component, Default, Clone)]
struct HpText;

fn hud() -> impl Scene {
    bsn! {
        Node { position_type: PositionType::Absolute, top: px(12), left: px(12) }
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5))
        Children [
            HpText
            Text("")
        ]
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn_scene(hud());
}

fn update_hp(player: Single<&Health, (With<Player>, Changed<Health>)>, mut text: Single<&mut Text, With<HpText>>) {
    text.0 = format!("HP {} / {}", player.hp.ceil(), player.max);
}

// ---------- 7. Adopting Rapier ----------

fn steer(mut q: Query<(&Enemy, &Transform, &mut Velocity)>, player: Single<&Transform, With<Player>>) {
    for (enemy, t, mut v) in &mut q {
        let dir = (player.translation - t.translation).normalize_or_zero();
        v.linear.x = dir.x * enemy.speed;
        v.linear.z = dir.z * enemy.speed;
    }
}

fn hurt(mut contacts: MessageReader<CollisionEvent>, mut q: Query<&mut Health, With<Player>>) {
    for c in contacts.read() {
        if let CollisionEvent::Started(a, b, _) = *c {
            let player = if q.contains(a) { a } else { b };
            if let Ok(mut h) = q.get_mut(player) {
                h.hp -= 10.0;
            }
        }
    }
}

fn grounded(rapier: ReadRapierContext, q: Single<(Entity, &Transform, &mut Velocity), With<Player>>) {
    let Ok(ctx) = rapier.single() else { return };
    let (e, t, mut v) = q.into_inner();
    let hit = ctx.cast_ray(t.translation, Vec3::NEG_Y, 1.1, true, QueryFilter::default().exclude_rigid_body(e));
    if hit.is_none() && v.linear.y > -20.0 {
        v.linear.y -= 0.5;
    }
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, RapierPhysicsPlugin::<NoUserData>::default(), EnemyPlugin))
        .add_systems(Startup, (spawn_grunt, spawn_hud))
        .add_systems(Update, (update_hp, hurt))
        .add_systems(FixedUpdate, (steer, grounded).before(PhysicsSet::SyncBackend))
        .run();
}
