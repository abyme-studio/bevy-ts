import { app, Component, type f32, Startup, Time, Transform, Update } from "bevy-ts";
import type { Commands, Mut, Query, Res } from "bevy-ts";
import { Cube } from "./cube.tsx";

export class Mover extends Component {
  speed: f32 = 1;
  amplitude: f32 = 2;
}

/** The game's camera; the cube and the sun are the scene's, in cube.tsx. */
export function scene(commands: Commands) {
  commands.spawnNative("camera3d", { position: [-3, 3, 6], lookAt: [0, 0, 0] });
}

export function bob(time: Res<Time>, q: Query<[Mover, Mut<Transform>]>) {
  q.each((mover, t) => {
    t.translation.y = Math.sin(time.elapsed_secs * mover.speed) * mover.amplitude;
  });
}

app.scene("cube", Cube);
app.addSystems(Startup, scene).addSystems(Update, bob);
