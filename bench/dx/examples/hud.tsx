// The HUD two ways. Both read the player's Health and the Score resource, and both patch one text
// node when those change and nothing otherwise (tsx decision 9's per-pair check in Rust).
import { app, type NodeRef, px, ref, res, useComponent } from "bevy-ts";
import { Explode, Health, Score } from "./systems.ts";

// ---------- A. tsx today: a hook makes the subscription ----------

export const HudWithHooks = ({ player }: { player: NodeRef }) => {
  const health = useComponent(player, Health); // must run in the prefab body, once
  return (
    <node style={{ position: "absolute", top: px(12), left: px(12) }} background="#0008">
      <text size={20}>{() => `HP ${Math.ceil(health().hp)} / ${health().max}`}</text>
    </node>
  );
};

// ---------- B. Proposed: no hook; the binding reads through the ref ----------
// `player.get(Health)` inside `() => ...` is a tracked read. The site (this element and prop)
// learns its reads on first evaluation, as a system learns its query (tsx decision 11), and Rust
// re-runs it only when that row's Health tick moves. Outside a binding, `player.get` is a dev error.

export const Hud = ({ player }: { player: NodeRef }) => (
  <node style={{ position: "absolute", top: px(12), left: px(12) }} background="#0008">
    <text size={20}>{() => `HP ${Math.ceil(player.get(Health).hp)} / ${player.get(Health).max}`}</text>
    <text size={16} color={() => (res(Score).value > 10 ? "#fd0" : "#fff")}>{() => `Score ${res(Score).value}`}</text>
    <button onPointerClick={(_, { commands }) => commands.trigger(Explode, player.entity, { radius: 5 })}>
      <text>Boom</text>
    </button>
  </node>
);

const player = ref();
app.scene("hud", () => <Hud player={player} />);
void HudWithHooks;
