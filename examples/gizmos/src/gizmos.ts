// Bevy's 3d_gizmos.rs `draw_example_collection` and `update_config`, and light_gizmos.rs'
// `update_config`, call for call. A TS project cannot declare a gizmo group (a group is a Rust
// type), so the "round" gizmos draw into Bevy's FrustumGizmoConfigGroup: a second group with its
// own line config, as 3d_gizmos.rs' MyRoundGizmos is.
import { app, Real, srgb, Startup, Time, Update } from "bevy-ts";
import type { ColorValue, Res, ResMut } from "bevy-ts";
import { KeyCode, type Keys } from "bevy-ts/input";
import {
  AabbGizmoConfigGroup,
  DefaultGizmoConfigGroup,
  FrustumGizmoConfigGroup,
  type GizmoConfig,
  type GizmoConfigStore,
  type GizmoGroup,
  type Gizmos,
  LightGizmoConfigGroup,
} from "bevy-ts/gizmos";

const ROUND = FrustumGizmoConfigGroup;

const PI = Math.PI;
/** A rotation of `angle` about a unit axis, as `{ x, y, z, w }`. */
const about = (axis: [number, number, number], angle: number) => {
  const s = Math.sin(angle / 2);
  return { x: axis[0] * s, y: axis[1] * s, z: axis[2] * s, w: Math.cos(angle / 2) };
};
/** The shortest rotation from unit vector `a` to unit vector `b`. */
const arc = (a: [number, number, number], b: [number, number, number]) => {
  const c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  const w = 1 + a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const n = Math.hypot(c[0], c[1], c[2], w);
  return { x: c[0] / n, y: c[1] / n, z: c[2] / n, w: w / n };
};
const mul = (p: { x: number; y: number; z: number; w: number }, q: { x: number; y: number; z: number; w: number }) => ({
  x: p.w * q.x + p.x * q.w + p.y * q.z - p.z * q.y,
  y: p.w * q.y - p.x * q.z + p.y * q.w + p.z * q.x,
  z: p.w * q.z + p.x * q.y - p.y * q.x + p.z * q.w,
  w: p.w * q.w - p.x * q.x - p.y * q.y - p.z * q.z,
});
const DIAGONAL: [number, number, number] = [1 / Math.sqrt(3), 1 / Math.sqrt(3), 1 / Math.sqrt(3)];

/** css TEAL mixed toward HOT_PINK by `k`, as `TEAL.mix(&HOT_PINK, k)`. */
const tealToPink = (k: number) => srgb(k, 0.5 - 0.09 * k, 0.5 + 0.21 * k);

export function drawExampleCollection(gizmos: Gizmos, myGizmos: Gizmos<FrustumGizmoConfigGroup>, time: Res<Time>) {
  const t = time.elapsed_secs;
  gizmos.grid(about([1, 0, 0], PI / 2), [20, 20], [2, 2], srgb(0.65, 0.65, 0.65));
  gizmos.grid({ translation: [10, 10, 10], rotation: about([1, 0, 0], (PI / 3) * 2) }, [20, 20], [2, 2], "#800080");
  gizmos.sphere([10, 10, 10], 1, "#800080");

  gizmos
    .primitive3d(
      { Plane3d: { normal: [0, 1, 0], half_size: [1, 1] } },
      { translation: [4 + Math.sin(t), 4 + Math.cos(t), 4], rotation: about([1, 0, 0], PI / 2 + t) },
      "#008000",
    )
    .cellCount([5, 10])
    .spacing([0.2, 0.1]);

  gizmos.cube({ translation: [0, 0.5, 0], scale: [1.25, 1.25, 1.25] }, "#000");
  gizmos.rect({ translation: [Math.cos(t) * 2.5, 1, 0], rotation: about([0, 1, 0], PI / 2) }, [2, 2], "#0f0");

  gizmos.cross([-1, 1, 1], 0.5, "#f0f");

  // 3d_gizmos.rs draws a FunctionCurve, a closure, which has no TS form: the same points, sampled
  // here, drawn as the strip `curve_gradient_3d` would draw.
  const resolution = Math.floor((Math.sin(t) + 1) * 100);
  const points: [[number, number, number], ColorValue][] = [];
  for (let n = 0; n <= resolution; n++) {
    const s = (n / Math.max(1, resolution)) * 5;
    points.push([[Math.sin(s * 10), Math.cos(s * 10), s - 6], tealToPink(s / 5)]);
  }
  // A gradient call takes about 60 points (docs/gizmos.md): strips of 50 that share their ends.
  for (let k = 0; k + 1 < points.length; k += 49) gizmos.linestripGradient(points.slice(k, k + 50));

  gizmos.primitive3d(
    { Capsule3d: { radius: 0.35, half_length: 0.25 } },
    { translation: [-1.75, 0.75, 0.75], rotation: mul(about([0, 1, 0], Math.cos(t / 2) * 10), about([0, 0, 1], PI / 3)) },
    "#9acd32",
  );

  myGizmos.sphere([1, 0.5, 0], 0.5, "#f00");

  myGizmos.roundedCuboid([-2, 0.75, -0.75], [0.9, 0.9, 0.9], "#40e0d0").edgeRadius(0.1).arcResolution(4);

  for (const y of [0, 0.5, 1]) {
    gizmos.ray([1, y, 0], [-3, Math.sin(t * 3), 0], "#00f");
  }

  myGizmos.arc3d(PI, 0.2, { translation: [1, 1, 1], rotation: arc([0, 1, 0], DIAGONAL) }, "#ffa500").resolution(10);

  // Circles have 32 line-segments by default.
  myGizmos.circle(arc([0, 0, 1], [0, 1, 0]), 3, "#000");
  // You may want to increase this for larger circles or spheres.
  myGizmos.circle(arc([0, 0, 1], [0, 1, 0]), 3.1, "#000080").resolution(64);
  myGizmos.sphere([0, 0, 0], 3.2, "#000").resolution(64);

  gizmos.arrow([0, 0, 0], [1.5, 1.5, 1.5], "#ff0");
  // You can create more complex arrows using the arrow builder.
  gizmos.arrow([2, 0, 2], [2, 2, 2], "#ff4500").withDoubleEnd().withTipLength(0.5);

  const from: [number, number, number] = [1, 2, 3];
  const to: [number, number, number] = [3, 2.5, 4];
  gizmos.rect(from, [1, 1], "#f00");
  myGizmos.shortArc3dBetween([(from[0] + to[0]) / 2, (from[1] + to[1]) / 2, (from[2] + to[2]) / 2], from, to, "#9acd32");
  gizmos.rect(to, [1, 1], "#f00");
}

const STYLES = ["Solid", "Dotted", { Dashed: { gap_scale: 3, line_scale: 5 } }] as const;
type LineStyle = NonNullable<NonNullable<GizmoConfig["line"]>["style"]>;
const nextStyle = (style: LineStyle | undefined): LineStyle =>
  style === "Solid" ? STYLES[1] : style === "Dotted" ? STYLES[2] : STYLES[0];
const nextJoints = (j: unknown) => (j === "Bevel" ? "Miter" : j === "Miter" ? { Round: 4 } : typeof j === "object" ? "None" : "Bevel");
const clampWidth = (w: number) => Math.min(50, Math.max(0, w));

/** Each config's `line` and fields, as `GizmoConfigStore.config` reads them. */
const config = (store: ResMut<GizmoConfigStore>, group: GizmoGroup) => store.config(group)[0];

export function lightSetup(store: ResMut<GizmoConfigStore>) {
  store.insert(LightGizmoConfigGroup, {}, { draw_all: true, color: "MatchLightColor" });
}

export function updateConfig(store: ResMut<GizmoConfigStore>, keys: Keys, realTime: Res<Time<Real>>) {
  if (keys.justPressed(KeyCode.KeyT)) {
    for (const g of store.groups()) store.insert(g, { depth_bias: config(store, g).depth_bias === 0 ? -1 : 0 }, {});
  }
  if (keys.justPressed(KeyCode.KeyP)) {
    for (const g of store.groups()) {
      const line = config(store, g).line!;
      const perspective = !line.perspective;
      store.insert(g, { line: { perspective, width: line.width! * (perspective ? 5 : 1 / 5) } }, {});
    }
  }

  const step = 5 * realTime.delta_secs;
  for (const [group, wider, narrower, show, style, joints] of [
    [DefaultGizmoConfigGroup, KeyCode.ArrowRight, KeyCode.ArrowLeft, KeyCode.Digit1, KeyCode.KeyU, KeyCode.KeyJ],
    [ROUND, KeyCode.ArrowUp, KeyCode.ArrowDown, KeyCode.Digit2, KeyCode.KeyI, KeyCode.KeyK],
  ] as const) {
    const c = config(store, group);
    const line = c.line!;
    if (keys.pressed(wider)) store.insert(group, { line: { width: clampWidth(line.width! + step) } }, {});
    if (keys.pressed(narrower)) store.insert(group, { line: { width: clampWidth(line.width! - step) } }, {});
    if (keys.justPressed(show)) store.insert(group, { enabled: !c.enabled }, {});
    if (keys.justPressed(style)) store.insert(group, { line: { style: nextStyle(line.style) } }, {});
    if (keys.justPressed(joints)) store.insert(group, { line: { joints: nextJoints(line.joints) as never } }, {});
  }

  if (keys.justPressed(KeyCode.KeyB)) {
    store.insert(AabbGizmoConfigGroup, {}, { draw_all: !store.config(AabbGizmoConfigGroup)[1].draw_all });
  }

  // light_gizmos.rs
  const [, light] = store.config(LightGizmoConfigGroup);
  if (keys.justPressed(KeyCode.KeyA)) store.insert(LightGizmoConfigGroup, {}, { draw_all: !light.draw_all });
  if (keys.justPressed(KeyCode.KeyC)) {
    const color = light.color === "MatchLightColor" ? "ByLightType" : light.color === "ByLightType" ? "Varied" : "MatchLightColor";
    store.insert(LightGizmoConfigGroup, {}, { color });
  }
}

app.addSystems(Startup, lightSetup);
app.addSystems(Update, drawExampleCollection);
app.addSystems(Update, updateConfig);
