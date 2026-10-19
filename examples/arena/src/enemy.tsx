// What an enemy is: a scene component. Levels place it (assets/levels/arena.bsn writes
// `@enemy::Enemy { @hp: 250.0 }`), and TSX can mount it too (`<Enemy at={[0, 0, -6]} />`).
import { Component, SceneComponent, type f32, type Vec3Like } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

export class Health extends Component {
  hp: f32 = 100;
  max: f32 = 100;
}
/** Marks enemies, for the systems that move and cull them. */
export class Foe extends Component {}

export class Enemy extends SceneComponent {
  static scene({ hp = 100, at = [0, 0, 0] }: { hp?: f32; at?: Vec3Like }) {
    return (
      <entity name="enemy">
        <Transform translation={at} />
        <Health initialHp={hp} max={hp} />
        <Foe />
        <mesh shape="cuboid" size={[1, 1, 1]} color="#cc3333" />
        <entity key="eye">
          <Transform translation={[0, 0.6, 0.5]} />
          <mesh shape="sphere" radius={0.15} color="#ffffff" />
        </entity>
      </entity>
    );
  }
}
