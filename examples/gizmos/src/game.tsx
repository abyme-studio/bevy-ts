// Bevy's examples/gizmos/3d_gizmos.rs and light_gizmos.rs in TS: the scene here, the gizmos and
// their key toggles in gizmos.ts. Remove the import below to see the scene without gizmos.
import { app, assets, hex, px } from "bevy-ts";
import { Camera3d, DirectionalLight, Gizmo, PointLight, SpotLight, Transform } from "bevy-ts/bevy";
import { GizmoAsset } from "bevy-ts/gizmos";
import "./gizmos.ts";

// Not in Bevy's examples: a retained grid over the ground, drawn once into a GizmoAsset (Bevy's
// retained gizmos) rather than every frame.
const fine = new GizmoAsset();
fine.grid({ rotation: [Math.SQRT1_2, 0, 0, Math.SQRT1_2] }, [10, 10], [0.5, 0.5], "#a8d8a8").outerEdges();
const groundGrid = assets.add(GizmoAsset, fine);

const HELP = [
  "T: gizmos on top of the scene",
  "P: line perspective",
  "Left / Right: straight line width; Up / Down: round line width",
  "1 / 2: show straight / round gizmos",
  "B: every AABB box",
  "U / I: straight / round line style; J / K: straight / round joints",
  "A: light gizmos; C: light gizmo colours",
].join("\n");

app.scene("gizmos", () => (
  <>
    {/* At (0, 1.5, 6), looking at the origin, as 3d_gizmos.rs' camera. */}
    <entity name="camera">
      <Camera3d />
      <Transform translation={[0, 1.5, 6]} rotation={[-0.1222, 0, 0, 0.9925]} />
    </entity>
    <entity name="ground">
      <mesh shape="plane" size={[5, 5]} color="#4d804d" />
    </entity>
    <entity name="ground-grid">
      <Transform translation={[0, 0.01, 0]} />
      <Gizmo handle={groundGrid} line_config={{ width: 1 }} />
    </entity>
    <entity name="cube">
      <Transform translation={[0, 0.5, 0]} />
      <mesh shape="cuboid" size={[1, 1, 1]} color="#ccb399" />
    </entity>
    <entity name="lamp">
      <Transform translation={[0, 1.5, 0]} />
      <PointLight color={hex("#008b8b")} range={2} shadow_maps_enabled />
    </entity>
    <entity name="spot">
      <Transform translation={[4, 2, 0]} rotation={[0, 0.5, 0, 0.8660254]} />
      <SpotLight color={hex("#800080")} range={3.5} />
    </entity>
    <entity name="sun">
      <Transform translation={[-4, 2, 0]} rotation={[-0.3826834, 0, 0, 0.9238795]} />
      <DirectionalLight color={hex("#ffd700")} illuminance={500} />
    </entity>
    <node style={{ position: "absolute", top: px(12), left: px(12) }}>
      <text size={14}>{HELP}</text>
    </node>
  </>
));
