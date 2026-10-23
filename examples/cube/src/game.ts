import { app, Component, type f32, Time, Transform, Update } from "bevy-ts";
import type { Mut, Query, Res } from "bevy-ts";
import { Cube } from "./cube.tsx";

export class Mover extends Component {
  speed: f32 = 1;
  amplitude: f32 = 2;
}

export function bob(time: Res<Time>, q: Query<[Mover, Mut<Transform>]>) {
  q.each((mover, t) => {
    t.translation.y = Math.sin(time.elapsed_secs * mover.speed) * mover.amplitude;
  });
}

app.scene("cube", Cube);
app.addSystems(Update, bob);
