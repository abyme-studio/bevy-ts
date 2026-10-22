// The cube's scene: the cube and the sun, so edit mode shows them before any system runs.
// game.ts mounts it once `Mover` is defined.
import { DirectionalLight, Transform } from "bevy-ts/bevy";
import { Mover } from "./game.ts";

export const Cube = () => (
  <>
    {/* At (3, 6, 2), looking at the origin. */}
    <entity name="sun">
      <DirectionalLight shadow_maps_enabled />
      <Transform translation={[3, 6, 2]} rotation={[-0.4341, 0.4107, 0.2323, 0.7674]} />
    </entity>
    <entity name="cube">
      <Transform />
      <mesh shape="cuboid" size={[1, 1, 1]} color="#ff8800" />
      <Mover />
    </entity>
  </>
);
