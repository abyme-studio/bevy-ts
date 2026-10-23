// Every bevy_ui element bevy-ts reaches from TSX, one panel per stage of the bevy-ui-coverage change.
// Not shipped (no example.json): `bevy-ts dev src/gallery.tsx`. A test (crates/bevy-ts-cli/tests/ui_gallery.rs)
// loads it headless and fails when an intrinsic in jsx-runtime.d.ts is not mounted here.
import { app, type Commands, conicGradient, deg, fr, linearGradient, percent, px, radialGradient, repeat, signal, span, Startup } from "bevy-ts";
import { ZIndex } from "bevy-ts/bevy";
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

const [tilted, setTilted] = signal(false);

/** Stage 1: grid, the remaining `Node` fields, corners, sides, outline, shadow, transform, gradients. */
const Style = () => (
  <Panel title="Style">
    <node key="grid" style={{ display: "grid", gridTemplateColumns: [fr(1), px(60), repeat(2, fr(1))], gridAutoRows: px(18), columnGap: px(4), rowGap: px(4), width: px(240) }}>
      <node key="wide" style={{ gridColumn: span(2) }} background="#4a6fa5" />
      <node key="a" background="#6b8fc7" />
      <node key="b" background="#6b8fc7" />
      <node key="tall" style={{ gridRow: span(2) }} background="#a5784a" />
      <node key="c" style={{ gridColumn: { start: 2, span: 3 } }} background="#4aa57a" />
    </node>
    <node key="row" style={{ columnGap: px(10), alignItems: "center" }}>
      <node key="corners" style={{ width: px(40), height: px(40), border: px(3) }} radius={[px(14), 0, px(14), 0]} border={{ top: "#ff6060", bottom: "#60ff60" }} background="#333a48" />
      <node key="outline" style={{ width: px(40), height: px(40) }} radius={percent(50)} outline={{ width: px(2), offset: px(2), color: "#ffd060" }} background="#333a48" />
      <node key="shadow" style={{ width: px(40), height: px(40) }} radius={px(6)} shadow={{ color: "#000000c0", x: px(3), y: px(4), blur: px(6) }} background="#566078" />
      <button key="tilt" style={{ width: px(40), height: px(40) }} transform={() => ({ rotation: tilted() ? deg(15) : 0, scale: tilted() ? 1.1 : 1 })} background="#7a4ab0" onPointerClick={() => setTilted(!tilted())}>
        <ZIndex value={1} />
      </button>
    </node>
    <node key="gradients" style={{ columnGap: px(10) }}>
      <node key="linear" style={{ width: px(60), height: px(30) }} background={linearGradient(90, ["#ff5f6d", "#ffc371"])} />
      <node key="radial" style={{ width: px(60), height: px(30) }} background={radialGradient(["#ffffff", "#2a5298"])} />
      <node key="conic" style={{ width: px(30), height: px(30) }} radius={percent(50)} background={conicGradient(["#f00", "#ff0", "#0f0", "#0ff", "#00f", "#f0f", "#f00"])} />
      <node key="frame" style={{ width: px(60), height: px(30), border: px(3) }} border={linearGradient(0, ["#00c6ff", "#0072ff"])} />
    </node>
  </Panel>
);

/** Stage 2: rich text with `<span>`, font, layout and decorations. */
const Text = () => (
  <Panel title="Text">
    <text key="rich" size={16}>
      HP <span key="hp" color="#ff6060" weight={700}>{() => `${100 - clicks()}`}</span> / 100 <span key="note" italic color="#9fb3d1">(click Basics to hurt)</span>
    </text>
    <text key="layout" size={13} justify="center" lineHeight={1.4} letterSpacing={px(1)} style={{ width: px(300) }}>
      Centred text that wraps over two lines, with wider letters.
    </text>
    <text key="decorated" size={16} underline="#ffd060" textShadow={{ x: 1, y: 1, color: "#000000" }}>
      Underlined, with a shadow, and <span key="struck" strikethrough textBackground="#553333">struck through</span>
    </text>
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
      <Style />
      <Text />
    </node>
  </>
));
app.addSystems(Startup, stage);
