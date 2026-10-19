// The HUD two ways. Both patch one text node when Health or Score changes and nothing otherwise.
import { app, type NodeRef, px, ref, res, useComponent } from "bevy-ts";
import { Explode, Health, Score } from "./systems.ts";

// tsx as specced before the ruling: a hook makes the subscription.
export const HudWithHooks = ({ player }: { player: NodeRef }) => {
  const health = useComponent(player, Health); // must run in the prefab body, once
  return (
    <node style={{ position: "absolute", top: px(12), left: px(12) }} background="#0008">
      <text size={20}>{() => `HP ${Math.ceil(health().hp)} / ${health().max}`}</text>
    </node>
  );
};

// Proposed: no hook; the binding reads through the ref and is re-run per site (tsx decision 11).
export const Hud = ({ player }: { player: NodeRef }) => (
  <node style={{ position: "absolute", top: px(12), left: px(12) }} background="#0008">
    <text size={20}>{() => `HP ${Math.ceil(player.get(Health).hp)} / ${player.get(Health).max}`}</text>
    <text size={16} color={() => (res(Score).value > 10 ? "#fd0" : "#fff")}>{() => `Score ${res(Score).value}`}</text>
    <button onPointerClick={({ commands }) => commands.trigger(Explode.of({ entity: player.entity, radius: 5 }))}>
      <text>Boom</text>
    </button>
  </node>
);

const player = ref();
app.scene("hud", () => <Hud player={player} />);
void HudWithHooks;
