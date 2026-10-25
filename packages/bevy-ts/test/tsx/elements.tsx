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
    <mesh shape="dodecahedron" />
    {/* @ts-expect-error another shape's prop */}
    <mesh shape="sphere" sides={6} />
    {/* @ts-expect-error a wrong-length vector */}
    <mesh shape="cuboid" size={[1, 2]} />
    {/* @ts-expect-error a prefab's required prop */}
    <Enemy />
  </entity>
);

// One of each mesh shape, with a setting or two.
export const Meshes = () => (
  <entity>
    <mesh shape="cuboid" size={[1, 2, 3]} color="#c33" />
    <mesh shape="sphere" radius={2} />
    <mesh shape="sphere" kind="uv" sectors={16} stacks={8} />
    <mesh shape="plane" size={[4, 4]} subdivisions={2} />
    <mesh shape="cylinder" radius={0.5} height={2} resolution={8} caps={false} anchor="bottom" />
    <mesh shape="capsule3d" radius={0.25} length={1} rings={2} uvProfile="uniform" />
    <mesh shape="cone" radius={1} height={2} anchor="base" />
    <mesh shape="conicalFrustum" radiusTop={0.2} radiusBottom={0.6} height={1} segments={2} />
    <mesh shape="torus" innerRadius={0.5} outerRadius={1} angleRange={[0, Math.PI]} />
    <mesh shape="tetrahedron" vertices={[[1, 1, 1], [-1, 1, -1], [-1, -1, 1], [1, -1, -1]]} />
    <mesh shape="triangle3d" vertices={[[0, 1, 0], [-1, 0, 0], [1, 0, 0]]} />
    <mesh shape="circle" radius={1} resolution={64} />
    <mesh shape="circularSector" radius={1} angle={Math.PI / 2} uvAngle={0.5} />
    <mesh shape="circularSegment" radius={1} halfAngle={1} />
    <mesh shape="ellipse" halfSize={[2, 1]} depth={0.5} />
    <mesh shape="annulus" innerRadius={0.5} outerRadius={1} depth={1} segments={3} />
    <mesh shape="rectangle" size={[2, 1]} />
    <mesh shape="rhombus" horizontalDiagonal={2} verticalDiagonal={1} />
    <mesh shape="regularPolygon" circumradius={1} sides={5} depth={0.2} />
    <mesh shape="triangle2d" vertices={[[0, 1], [-1, 0], [1, 0]]} />
    <mesh shape="capsule2d" radius={0.5} length={1} resolution={8} />
    <mesh shape="convexPolygon" vertices={[[0, 0], [1, 0], [1, 1], [0, 1]]} depth={1} />
  </entity>
);

// One of each 2D shape as a `<mesh2d>`, and a `<material2d>`.
export const Meshes2d = () => (
  <entity>
    <entity><mesh2d shape="circle" radius={1} resolution={64} color="#c33" /></entity>
    <entity><mesh2d shape="circularSector" radius={1} angle={Math.PI / 2} /></entity>
    <entity><mesh2d shape="circularSegment" radius={1} halfAngle={1} /></entity>
    <entity><mesh2d shape="ellipse" halfSize={[2, 1]} /></entity>
    <entity><mesh2d shape="annulus" innerRadius={0.5} outerRadius={1} /></entity>
    <entity><mesh2d shape="rectangle" size={[2, 1]} /></entity>
    <entity><mesh2d shape="rhombus" horizontalDiagonal={2} verticalDiagonal={1} /></entity>
    <entity><mesh2d shape="regularPolygon" circumradius={1} sides={5} /></entity>
    <entity><mesh2d shape="triangle2d" vertices={[[0, 1], [-1, 0], [1, 0]]} /></entity>
    <entity><mesh2d shape="capsule2d" radius={0.5} length={1} /></entity>
    <entity>
      <mesh2d shape="convexPolygon" vertices={[[0, 0], [1, 0], [1, 1], [0, 1]]} />
      <material2d color="#fff" alpha={0.5} alphaMode={{ mask: 0.5 }} texture="hero.png" uvTransform={{ scale: [2, 2] }} />
    </entity>
    <entity>
      {/* @ts-expect-error a 3D shape */}
      <mesh2d shape="cuboid" />
      {/* @ts-expect-error <mesh2d> is flat */}
      <mesh2d shape="circle" depth={1} />
      {/* @ts-expect-error a StandardMaterial prop */}
      <material2d metallic={1} />
      {/* @ts-expect-error a 3D alpha mode */}
      <material2d alphaMode="add" />
    </entity>
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
// `<input>`: Bevy's TextInput, its events typed; its text has no controlled prop.
export const Field = () => (
  <input
    initialValue=""
    autoFocus
    maxCharacters={40}
    onKeyboardInput={(e) => e.value.length + e.key.length}
    onSubmit={(e) => e.value}
    onCancel={(e) => e.entity}
    onTextEditChange={(e) => e.value}
  />
);
// @ts-expect-error  the text is Bevy's editor's: there is no `value` prop
export const Controlled = () => <input value="x" />;
// @ts-expect-error  a keyboard event has no `foo`
export const BadEvent = () => <input onKeyboardInput={(e) => e.foo} />;
// @ts-expect-error  a cancel has no value
export const BadCancel = () => <input onCancel={(e) => e.value} />;

// `<material>` takes every StandardMaterial field.
export const Showcase = () => (
  <entity>
    <mesh shape="cuboid" />
    <material
      color="#fff"
      baseColorChannel="uv1"
      baseColorTexture="textures/crate.png"
      emissive="#000"
      emissiveExposureWeight={0.5}
      emissiveChannel="uv0"
      emissiveTexture="textures/glow.png"
      roughness={0.4}
      metallic={0.1}
      metallicRoughnessChannel="uv0"
      metallicRoughnessTexture="textures/mr.png"
      reflectance={0.5}
      specularTint="#fff"
      diffuseTransmission={0.1}
      specularTransmission={0.9}
      thickness={0.2}
      ior={1.5}
      attenuationDistance={2}
      attenuationColor="#88ccff"
      normalMapChannel="uv0"
      normalMapTexture="textures/n.png"
      flipNormalMapY
      occlusionChannel="uv0"
      occlusionTexture="textures/ao.png"
      clearcoat={1}
      clearcoatPerceptualRoughness={0.2}
      anisotropyStrength={0.5}
      anisotropyRotation={1}
      doubleSided
      cullMode={null}
      unlit={false}
      fogEnabled
      depthBias={0}
      depthMap="textures/depth.png"
      parallaxDepthScale={0.1}
      parallaxMappingMethod={{ relief: { maxSteps: 8 } }}
      maxParallaxLayerCount={16}
      lightmapExposure={1}
      opaqueRenderMethod="deferred"
      deferredLightingPassId={1}
      uvTransform={{ scale: [0.5, 0.5], rotation: 0, translation: [0.5, 0] }}
    />
  </entity>
);
export const OcclusionParallax = () => <material parallaxMappingMethod="occlusion" cullMode="front" />;
// @ts-expect-error cullMode is "back", "front" or null
export const BadCull = () => <material cullMode="both" />;
// @ts-expect-error a texture blocked on Bevy (its feature is off) is not a prop
export const BlockedTexture = () => <material clearcoatTexture="textures/cc.png" />;
// @ts-expect-error a texture is an asset path
export const BadTexture = () => <material baseColorTexture={1} />;
export const Counted = () => <entity><PlayAnimation clip="models/Fox.glb#Animation2" repeat={{ Count: 3 }} paused={false} cull /></entity>;
export const Once = () => <entity><PlayAnimation clip="models/Fox.glb#Animation2" repeat="Never" paused /></entity>;
// @ts-expect-error  `repeat` is Bevy's RepeatAnimation, not a bool
export const BoolRepeat = () => <entity><PlayAnimation repeat={true} /></entity>;
