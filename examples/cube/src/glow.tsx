// The cube example with a script material (bevy-ts/material): its params in TS, its shader in
// glow.wgsl, no Rust. Run this file as the entry: `bevy-ts dev examples/cube/src/glow.tsx`.
// Save glow.wgsl and the cube redraws with no reload; change the params and it re-encodes.
import "./game.ts";
import { app } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
import { color, f32, material } from "bevy-ts/material";
import glow from "./glow.wgsl";

export const Glow = material("game::Glow", { params: { color: color, pulse: f32 }, fragment: glow });

app.scene("glow", () => (
  <entity name="glow">
    <Transform translation={[2.5, 0, 0]} />
    <mesh shape="cuboid" size={[1, 1, 1]} />
    <material kind={Glow} color="#ff8800" pulse={2} />
  </entity>
));
