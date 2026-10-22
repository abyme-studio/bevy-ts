// The arena: a player, a HUD and the sun in TSX, the enemies placed by assets/levels/arena.bsn, a camera, and
// three systems that play. `bevy-ts dev src/game.tsx`.
import { app, type Commands, type Entity, type Local, type Mut, type Query, ref, type Res, Startup, Time, Transform, type u32, Update, type With, type Without } from "bevy-ts";
import { DirectionalLight } from "bevy-ts/bevy";
import { Foe, Health } from "./enemy.tsx";
import { Hud } from "./hud.tsx";
import { Player, PlayerUnit } from "./player.tsx";

/** The game's camera. It sits where the editor's first view does, so edit mode opens on the same
 * view. */
export function stage(commands: Commands) {
  commands.spawnNative("camera3d", { position: [-6, 5, 10], lookAt: [0, 0, 0] });
}

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
    {/* The sun is scene, not a startup's, so edit mode shows the arena lit. At (3, 6, 2), looking at the origin. */}
    <entity name="sun">
      <DirectionalLight shadow_maps_enabled />
      <Transform translation={[3, 6, 2]} rotation={[-0.4341, 0.4107, 0.2323, 0.7674]} />
    </entity>
    <PlayerUnit handle={player} />
    <Hud player={player} />
  </>
));
app.level("levels/arena.bsn");
app.addSystems(Startup, stage).addSystems(Update, damage, bob, cull);
