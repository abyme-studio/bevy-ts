// Type tests for the `bevy-ts` declarations: `tsc -p test` must pass, and every
// `@ts-expect-error` must be an error.
import { app, component, f32, vec3, enumOf, entity, array, u16 } from "bevy-ts";
import { Transform, GlobalTransform } from "bevy-ts/bevy";

const Mover = component("game::Mover", { speed: f32.default(1), amplitude: f32 });
const Unit = component("game::Unit", {
  pos: vec3,
  state: enumOf("idle", "run"),
  target: entity,
  slots: array(u16, 4),
});

app.system("bob", {
  read: [Mover],
  write: [Transform, Unit],
  with: [GlobalTransform],
  run(q, ctx) {
    q.each((e, mover, t, unit) => {
      const s: number = mover.speed;
      // @ts-expect-error a misspelt field does not exist
      mover.sped = 2;
      // @ts-expect-error read rows are readonly
      mover.speed = 3;
      t.translation.y = s * ctx.elapsed;
      t.scale.set(1, 2, 1);
      unit.state = "run";
      const st: "idle" | "run" = unit.state;
      unit.target = e;
      unit.slots.set(0, 3);
      void st;
    });
    for (const table of q.tables) {
      const { f32: arr, ticks, offsets } = table.split(Unit);
      arr![offsets.pos / 4] = 1;
      ticks[0] = ctx.tick;
    }
  },
});

app.startup("scene", ({ commands }) => {
  const e = commands.spawn(Mover({ speed: 2 }), Transform);
  commands.insert(e, Unit({ pos: [0, 1, 0], state: "idle" }));
  // @ts-expect-error unknown field in a value
  commands.spawn(Mover({ sped: 2 }));
  // @ts-expect-error a Rust component without a layout cannot be read
  app.system("bad", { read: [GlobalTransform], run() {} });
});
import type { Methods, HelloResult, PROTOCOL_VERSION } from "bevy-ts/protocol";
const v: typeof PROTOCOL_VERSION = "1.1.0";
const h: Methods["bevy-ts/hello"]["result"] = {} as HelloResult;
void v; void h;
import type { TsxNodeResult } from "bevy-ts/protocol";
const n: Methods["bevy-ts/tsx_node"]["result"] = {} as TsxNodeResult;
void n;
