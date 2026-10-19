// Type tests for TSX elements: `deno check` (and `tsc -p test`) must pass, and every
// `@ts-expect-error` must be an error.
import { app, component, f32, vec3 } from "bevy-ts";
import { Transform, Name } from "bevy-ts/bevy";

const Health = component("game::Health", { hp: f32.default(100), max: f32.default(100) });
const Mover = component("game::Mover", { velocity: vec3, maxSpeed: f32 });

export const Enemy = ({ at, hp = 100 }: { at: [number, number, number]; hp?: number }) => (
  <entity name="enemy">
    <Transform translation={at} />
    <Health initialHp={hp} max={hp} />
    <Mover initialVelocity={[0, 0, 1]} maxSpeed={3} />
    <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
    <entity key="eye">
      <Transform translation={[0, 0.6, 0.5]} />
      <Name value="eye" />
    </entity>
  </entity>
);

app.scene("arena", () => (
  <>
    <Enemy key="a" at={[-3, 0, -4]} />
    <Enemy key="b" at={[3, 0, -4]} hp={250} />
    {[1, 2, 3].map((i) => (
      <Enemy key={`e${i}`} at={[i, 0, 0]} />
    ))}
  </>
));

// The value form still works for commands.
app.startup("s", ({ commands }) => {
  commands.spawn(Health({ hp: 3 }), Transform);
});

export const Bad = () => (
  <entity>
    {/* @ts-expect-error a field given both ways */}
    <Health hp={10} initialHp={10} />
    {/* @ts-expect-error an unknown field */}
    <Health mx={100} />
    {/* @ts-expect-error a wrongly typed field */}
    <Health max="lots" />
    {/* @ts-expect-error an unknown shape */}
    <mesh shape="torus" />
    {/* @ts-expect-error a prefab's required prop */}
    <Enemy />
  </entity>
);

// @ts-expect-error entity props are name and children only
export const AlsoBad = () => <entity colour="red" />;
