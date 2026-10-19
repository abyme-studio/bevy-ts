import { Component, type NodeRef } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";
import { Health } from "./enemy.tsx";

export class Player extends Component {}

export const PlayerUnit = ({ handle }: { handle: NodeRef }) => (
  <entity name="player" ref={handle}>
    <Transform translation={[0, 0, 0]} />
    <Health initialHp={100} max={100} />
    <Player />
    <mesh shape="sphere" radius={0.5} color="#3366cc" />
  </entity>
);
