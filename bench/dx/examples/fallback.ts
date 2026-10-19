// The fallback (operator, 2026-10-20): a system whose param types are computed uses the setup
// closure. The syntax-only reader refuses `ReturnType<...>` with file and line; this form declares
// with values instead.
import { Mut, query, system, Transform, Update, app, type Query, type With } from "bevy-ts";
import { Alive, Enemy } from "./systems.ts";

declare function makeQuery(): Query<[Enemy, Mut<Transform>], With<Alive>>;

// Refused by the reader: computed param type.
export function computed(q: ReturnType<typeof makeQuery>) { q.each(() => {}); }

// The fallback: declarations run once at load, the body every frame.
export const drift = system(() => {
  const q = query(Enemy, Mut(Transform)).with(Alive);
  return () => q.each((enemy, t) => { t.translation.y += enemy.speed * 0.001; });
});

app.addSystems(Update, drift);
