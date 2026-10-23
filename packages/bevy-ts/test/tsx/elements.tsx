// Type tests for TSX elements: `deno check` (and `tsc -p test`) must pass, and every
// `@ts-expect-error` must be an error.
import { app, Component, SceneComponent, Startup } from "bevy-ts";
import type { Commands, f32, u8, Vec3 } from "bevy-ts";
import { Transform, Name, PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";

class Health extends Component { hp: f32 = 100; max: f32 = 100 }
class Mover extends Component { declare velocity: Vec3; declare maxSpeed: f32 }
/** A scene component's element takes its fields and its scene's props. */
class Grunt extends SceneComponent {
  team: u8 = 0;
  static scene({ speed = 2 }: { speed?: f32 }) {
    return <entity name="grunt"><Mover maxSpeed={speed} /></entity>;
  }
}
export const Squad = () => <entity><Grunt team={1} speed={3} /></entity>;
// @ts-expect-error  `sped` is neither a field nor a prop of Grunt
export const BadSquad = () => <entity><Grunt sped={3} /></entity>;

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
export function spawnOne(commands: Commands) {
  commands.spawn(Health.of({ hp: 3 }), Transform.of());
}
app.addSystems(Startup, spawnOne);

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

// A HUD with no hooks: bindings read the player's Health directly.
import { For, Show, px, ref, signal } from "bevy-ts";

const player = ref();
const [open, setOpen] = signal(false);
const [items] = signal([{ id: "a", label: "Sword" }]);

export const Hud = () => (
  <node style={{ position: "absolute", top: px(12), left: px(12), padding: px(8) }} background="#0008">
    <text size={20}>{() => `HP ${Math.ceil(player.get(Health).hp)} / ${player.get(Health).max}`}</text>
    <text color={() => (player.get(Health).hp < 20 ? "#f33" : "#fff")}>HP</text>
    <Show when={open} fallback={<text>closed</text>}>
      <For each={items} key={(i) => i.id}>
        {(i) => <text>{i.label}</text>}
      </For>
    </Show>
  </node>
);

app.scene("hud", () => (
  <>
    <entity ref={player}>
      <Health initialHp={100} max={() => 100 + (open() ? 10 : 0)} />
    </entity>
    <Hud />
  </>
));
setOpen(true);

// @ts-expect-error style keys are Node's
export const BadStyle = () => <node style={{ widht: 3 }} />;
// @ts-expect-error a binding reads components of a ref, not strings
export const BadRead = () => <text>{() => player.get("Health")}</text>;

export const Clicky = () => (
  <button onPointerClick={({ commands, frame, button }) => button === "primary" && commands.mount(<Enemy key={`e${frame}`} at={[0, 0, 0]} />)}>
    <text>Spawn</text>
  </button>
);
// @ts-expect-error picking props are onPointer* with Bevy's event names
export const BadClick = () => <entity onClick={() => {}} />;

// A resource read in a binding, no hook.
import { Resource, res, type u32 } from "bevy-ts";
class Score extends Resource { value: u32 = 0 }
export const ScoreLine = () => <text color={() => (res(Score).value > 10 ? "#fd0" : "#fff")}>{() => `Score ${res(Score).value}`}</text>;

// Rust components written through reflection: every field is a prop, controlled or `initial*`,
// and a one-field tuple struct takes `value`.
export const Fox = ({ clip }: { clip: () => string }) => (
  <entity>
    <WorldAssetRoot value="models/Fox.glb#Scene0" />
    <PlayAnimation clip={clip} initialSpeed={2} transition_secs={0.4} />
  </entity>
);
export const StillFox = () => <entity><WorldAssetRoot initialValue="models/Fox.glb#Scene0" /></entity>;
// @ts-expect-error  a one-field tuple struct takes `value`, not its field's name
export const BadRoot = () => <entity><WorldAssetRoot path="models/Fox.glb#Scene0" /></entity>;
// @ts-expect-error  `speed` is a number
export const BadSpeed = () => <entity><PlayAnimation speed="fast" /></entity>;
// @ts-expect-error  PlayAnimation has no field `loop`
export const BadField = () => <entity><PlayAnimation loop /></entity>;
