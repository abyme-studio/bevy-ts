// The HUD: no hooks. The text binds the player's Health; the button mounts an enemy.
import { type NodeRef, px } from "bevy-ts";
import { Enemy, Health } from "./enemy.tsx";

export const Hud = ({ player }: { player: NodeRef }) => (
  <node style={{ position: "absolute", top: px(12), left: px(12), padding: px(8) }} background="#00000088">
    <text key="hp" size={20}>{() => `HP ${Math.ceil(player.get(Health).hp)} / ${player.get(Health).max}`}</text>
    <button key="spawn" onPointerClick={({ commands, frame }) => commands.mount(<Enemy key={`e${frame}`} at={[0, 0, -6]} />)}>
      <text>Spawn</text>
    </button>
  </node>
);
