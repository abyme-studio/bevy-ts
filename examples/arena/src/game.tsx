// The arena: a player, a HUD, the sun and the camera in TSX, the enemies placed by assets/levels/arena.bsn,
// and three systems that play. `bevy-ts dev src/game.tsx`.
import { app, type Commands, type Entity, hex, type Local, type Mut, type Query, ref, type Res, Time, Transform, type u32, Update, type With, type Without } from "bevy-ts";
import { Camera3d, DirectionalLight } from "bevy-ts/bevy";
import { Foe, Health } from "./enemy.tsx";
import { Hud } from "./hud.tsx";
import { Player, PlayerUnit } from "./player.tsx";

/** Every 30 frames, each enemy within 10 units hurts the player by 5. */
export function damage(
  clock: Local<{ frames: u32 }>,
  enemies: Query<[Transform], With<Foe>>,
  players: Query<[Transform, Mut<Health>], With<Player>>,
) {
  clock.frames += 1;
  if (clock.frames % 30 !== 0) return;
  players.each((p, h) => {
    enemies.each((e) => {
      if (e.translation.distance(p.translation) < 10) h.hp = Math.max(0, h.hp - 5);
    });
  });
}

/** Enemies bob up and down. */
export function bob(time: Res<Time>, enemies: Query<[Mut<Transform>], [With<Foe>, Without<Player>]>) {
  enemies.each((t) => {
    t.translation.y = Math.sin(time.elapsed_secs * 2) * 0.5;
  });
}

/** Enemies at no hp are despawned: a reload does not bring them back. */
export function cull(commands: Commands, enemies: Query<[Entity, Health], With<Foe>>) {
  enemies.each((e, h) => {
    if (h.hp <= 0) commands.entity(e).despawn();
  });
}

const player = ref();
app.scene("arena", () => (
  <>
    {/* The sun is scene, not a startup's, so edit mode shows the arena lit. At (3, 6, 2), looking at the origin; a warm white. */}
    <entity name="sun">
      <DirectionalLight shadow_maps_enabled color={hex("#fff1dc")} />
      <Transform translation={[3, 6, 2]} rotation={[-0.4341, 0.4107, 0.2323, 0.7674]} />
    </entity>
    {/* The camera is scene too, so edit mode's game view shows the arena. At (-6, 5, 10), looking at the
        origin: where the editor's first view sits, so edit mode opens on the same view. */}
    <entity name="camera">
      <Camera3d />
      <Transform translation={[-6, 5, 10]} rotation={[-0.1938, -0.2615, -0.0537, 0.944]} />
    </entity>
    <PlayerUnit handle={player} />
    <Hud player={player} />
  </>
));
app.level("levels/arena.bsn");
app.addSystems(Update, damage, bob, cull);
