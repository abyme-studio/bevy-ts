// Type tests for `bevy-ts/gizmos`: engine/native-modules.d.ts holds the engine's declaration
// (tests/it/gizmos.rs `the_engine_declares_gizmos` checks it), and `tsc -p test` checks these cases against it.
import { assets, type AssetRef, type ResMut } from "bevy-ts";
import { GizmoAsset, type GizmoAssetValue, type Gizmos, type GizmoConfigStore, type LightGizmoConfigGroup, DefaultGizmoConfigGroup } from "bevy-ts/gizmos";

export function draw(g: Gizmos) {
  g.line([0, 0, 0], { x: 1, y: 0, z: 0 }, "#f00");
  // A strip takes points or a Float32Array.
  g.linestrip([[0, 0, 0], { x: 1, y: 1, z: 0 }], "#0f0");
  g.linestrip(new Float32Array(9), "#0f0");
  // A builder's own settings chain.
  g.circle([0, 0, 0], 1, "#fff").resolution(64);
  g.arrow([0, 0, 0], [1, 0, 0], "#fff").withTipLength(0.2).withDoubleEnd();
  // @ts-expect-error: an arrow has no resolution
  g.arrow([0, 0, 0], [1, 0, 0], "#fff").resolution(8);
  g.primitive3d({ Cuboid: { half_size: [1, 2, 3] } }, [0, 0, 0], "#fff");
  // @ts-expect-error: a cuboid has no radius
  g.primitive3d({ Cuboid: { radius: 1 } }, [0, 0, 0], "#fff");
  // @ts-expect-error: not a primitive
  g.primitive3d({ Blob: {} }, [0, 0, 0], "#fff");
}

// configExt() is typed by the group.
export function light(g: Gizmos<LightGizmoConfigGroup>) {
  const drawAll: boolean | undefined = g.configExt().draw_all;
  const width: number | undefined = g.config().line?.width;
  return [drawAll, width];
}

// @ts-expect-error: Nope is not a gizmo group
export function nope(g: Gizmos<"Nope">) { return g; }

// What `bevy-ts types` writes for the store's claim.
declare module "bevy-ts" {
  interface ResMutClaims {
    "ResMut<GizmoConfigStore>": [GizmoConfigStore, import("bevy-ts/gizmos").GizmoConfigStoreMut];
  }
}

export function toggle(store: ResMut<GizmoConfigStore>) {
  store.insert(DefaultGizmoConfigGroup, { line: { width: 4, style: { Dashed: { gap_scale: 2 } } } }, {});
  // @ts-expect-error: width is a number
  store.insert(DefaultGizmoConfigGroup, { line: { width: "4" } }, {});
  const [, light] = store.config("LightGizmoConfigGroup");
  return light.draw_all;
}

// A retained GizmoAsset: draw into it, add it, replace it; test/tsx/gizmo.tsx shows it.
export function retained() {
  const g = new GizmoAsset();
  g.line([0, 0, 0], [1, 0, 0], "#f00");
  g.circle([0, 0, 0], 1, "#fff").resolution(64);
  const ref: AssetRef<GizmoAssetValue> = assets.add(GizmoAsset, g);
  assets.set(ref, g);
  const calls: readonly unknown[] | undefined = assets.get(ref)?.calls;
  // @ts-expect-error: an arrow has no resolution
  g.arrow([0, 0, 0], [1, 0, 0], "#fff").resolution(8);
  // @ts-expect-error: a GizmoAsset draws, it has no config
  g.config();
  // @ts-expect-error: calls is a list
  assets.add(GizmoAsset, { calls: 1 });
  return calls;
}
