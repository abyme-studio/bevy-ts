import { app, Component, f32, Update } from "bevy-ts";
import type { Mut, Query } from "bevy-ts";

export class Mover extends Component("game::Mover", { speed: f32.default(1) }) {}

export function speedUp(q: Query<[Mut<Mover>]>) {
  q.each((m) => {
    m.speed *= 2;
  });
}

app.addSystems(Update, speedUp);
