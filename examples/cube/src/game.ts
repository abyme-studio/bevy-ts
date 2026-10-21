import { app, Component, type f32, Startup, Time, Transform, Update } from "bevy-ts";
import type { Commands, Mut, Query, Res } from "bevy-ts";
import "./glow.tsx";

export class Mover extends Component {
  speed: f32 = 1;
  amplitude: f32 = 2;
}

export function scene(commands: Commands) {
  commands.spawnNative("camera3d", { position: [-3, 3, 6], lookAt: [0, 0, 0] });
  commands.spawnNative("directionalLight", { position: [3, 6, 2], lookAt: [0, 0, 0] });
  const cube = commands.spawnNative("cuboid", { size: [1, 1, 1], color: "#f80" });
  commands.entity(cube).insert(Mover.of());
}

export function bob(time: Res<Time>, q: Query<[Mover, Mut<Transform>]>) {
  q.each((mover, t) => {
    t.translation.y = Math.sin(time.elapsed_secs * mover.speed) * mover.amplitude;
  });
}

app.addSystems(Startup, scene).addSystems(Update, bob);
