// Foxes: a grid of Bevy's animated fox in TSX. Each fox is a glTF scene (`WorldAssetRoot`) with
// `PlayAnimation` beside it; Rust plays the clip once the scene is ready. The button switches
// every fox's clip, and Rust cross-fades to it. `bevy-ts dev src/game.tsx`.
import { app, type Commands, px, signal, Startup, Transform } from "bevy-ts";
import { DirectionalLight, PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";

/** Fox.glb's clips, in the file's order. */
const CLIPS = [
  { name: "Survey", path: "models/Fox.glb#Animation0" },
  { name: "Walk", path: "models/Fox.glb#Animation1" },
  { name: "Run", path: "models/Fox.glb#Animation2" },
];
const SIDE = 5;
const SPACING = 2.5;

const [clip, setClip] = signal(0);

export function stage(commands: Commands) {
  commands.spawnNative("camera3d", { position: [0, 9, 14], lookAt: [0, 0, 0] });
}

/** One fox: its scene and the clip it plays, bound so a new clip reaches every fox. */
const Fox = ({ at }: { at: [number, number, number] }) => (
  <entity name="fox">
    <Transform translation={at} scale={[0.02, 0.02, 0.02]} />
    <WorldAssetRoot value="models/Fox.glb#Scene0" />
    <PlayAnimation clip={() => CLIPS[clip()].path} transition_secs={0.4} />
  </entity>
);

const offset = ((SIDE - 1) * SPACING) / 2;
const grid: [number, number, number][] = [];
for (let z = 0; z < SIDE; z++) {
  for (let x = 0; x < SIDE; x++) grid.push([x * SPACING - offset, 0, z * SPACING - offset]);
}

app.scene("foxes", () => (
  <>
    <entity name="sun">
      <DirectionalLight shadow_maps_enabled />
      <Transform translation={[3, 6, 2]} rotation={[-0.4341, 0.4107, 0.2323, 0.7674]} />
    </entity>
    <entity name="ground">
      <mesh shape="plane" size={[20, 20]} color="#4a6b3a" />
    </entity>
    {grid.map((at, i) => <Fox key={`fox${i}`} at={at} />)}
    <node style={{ position: "absolute", top: px(12), left: px(12), padding: px(8), flexDirection: "column", rowGap: px(6) }} background="#00000088">
      <text key="clip" size={20}>{() => `Clip: ${CLIPS[clip()].name}`}</text>
      <button key="next" style={{ padding: px(4) }} background="#3366cc" onPointerClick={() => setClip((c) => (c + 1) % CLIPS.length)}>
        <text>Next clip</text>
      </button>
    </node>
  </>
));
app.addSystems(Startup, stage);
