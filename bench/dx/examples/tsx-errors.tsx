// TSX mistakes the types catch.
import { Transform } from "bevy-ts";
import { RigidBody } from "bevy-ts/rapier";
import { Health } from "./systems.ts";

export const a = (
  // @ts-expect-error  misspelt field
  <Health hpp={3} />
);
// @ts-expect-error  not a RigidBody variant
export const b = <RigidBody value="Floating" />;
// @ts-expect-error  translation is three numbers
export const c = <Transform translation={[1, 2]} />;
// @ts-expect-error  onPointerClick is an entity prop, not a mesh prop
export const d = <mesh shape="cuboid" onPointerClick={() => {}} />;
