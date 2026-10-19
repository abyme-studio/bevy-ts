// What an enemy is: a prefab, a plain function from props to one element.
import { Component, type f32 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

export class Health extends Component {
  hp: f32 = 100;
  max: f32 = 100;
}
/** Marks enemies, for the systems that move and cull them. */
export class Foe extends Component {}

export const Enemy = ({ at, hp = 100 }: { at: [number, number, number]; hp?: number }) => (
  <entity name="enemy">
    <Transform translation={at} />
    <Health initialHp={hp} max={hp} />
    <Foe />
    <mesh shape="cuboid" size={[1, 1, 1]} color="#cc3333" />
  </entity>
);
