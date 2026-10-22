// Every bevy_ui element bevy-ts reaches from TSX, one panel per stage of the bevy-ui-coverage change.
// Not shipped (no example.json): `bevy-ts dev src/gallery.tsx`. A test (crates/bevy-ts-cli/tests/ui_gallery.rs)
// loads it headless and fails when an intrinsic in jsx-runtime.d.ts is not mounted here.
import { app, type Commands, px, signal, Startup } from "bevy-ts";
import type { Child } from "bevy-ts/jsx-runtime";

export function stage(commands: Commands) {
  commands.spawnNative("camera3d", { position: [-3, 3, 6], lookAt: [0, 0, 0] });
}

/** A titled panel in the gallery's grid of panels. */
const Panel = ({ title, children }: { title: string; children?: Child }) => (
  <node style={{ flexDirection: "column", rowGap: px(6), padding: px(10), minWidth: px(260) }} background="#1b1f27ee" border="#3a4150" radius={px(6)}>
    <text key="title" size={16} color="#9fb3d1">{title}</text>
    <node key="body" style={{ flexDirection: "column", rowGap: px(6) }}>
      {children}
    </node>
  </node>
);

const [clicks, setClicks] = signal(0);

/** Stage 0: the four intrinsics bevy-ts had before the change. */
const Basics = () => (
  <Panel title="Basics">
    <node key="box" style={{ width: px(120), height: px(24) }} background="#3366cc" />
    <text key="text" size={14}>{() => `Plain text, clicked ${clicks()} times`}</text>
    <image key="image" src="ui/checker.png" style={{ width: px(48), height: px(48) }} />
    <button key="button" style={{ padding: px(6) }} background="#2a7a4b" radius={px(4)} onPointerClick={() => setClicks(clicks() + 1)}>
      <text>Click me</text>
    </button>
  </Panel>
);

app.scene("gallery", () => (
  <>
    {/* Not UI: a cube behind the panels, so <entity>, <mesh> and <material> appear too. */}
    <entity key="cube">
      <mesh shape="cuboid" size={[1, 1, 1]} />
      <material color="#c33" roughness={0.6} />
    </entity>
    <node key="gallery" style={{ position: "absolute", top: px(12), left: px(12), right: px(12), flexWrap: "wrap", columnGap: px(12), rowGap: px(12) }}>
      <Basics />
    </node>
  </>
));
app.addSystems(Startup, stage);
