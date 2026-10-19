import { app, component, f32 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

export const Mover = component("game::Mover", {
  speed: f32.default(1),
  amplitude: f32.default(2),
});

app.startup("scene", ({ commands }) => {
  commands.spawnNative("camera3d", { position: [-3, 3, 6], lookAt: [0, 0, 0] });
  commands.spawnNative("directionalLight", { position: [3, 6, 2], lookAt: [0, 0, 0] });
  const cube = commands.spawnNative("cuboid", { size: [1, 1, 1], color: "#f80" });
  commands.insert(cube, Mover);
});

app.system("bob", {
  stage: "update",
  read: [Mover],
  write: [Transform],
  run(q, ctx) {
    q.each((_, mover, t) => {
      t.translation.y = Math.sin(ctx.elapsed * mover.speed) * mover.amplitude;
    });
  },
});
