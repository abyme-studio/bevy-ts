// Type tests for the retained `Gizmo` component (docs/gizmos.md): `tsc -p test` must pass, and
// every `@ts-expect-error` must be an error.
import { assets } from "bevy-ts";
import { Gizmo, StandardMaterial } from "bevy-ts/bevy";
import { GizmoAsset } from "bevy-ts/gizmos";

const ref = assets.add(GizmoAsset, new GizmoAsset());
export const Shown = () => (
  <>
    <entity><Gizmo handle={ref} line_config={{ width: 4 }} depth_bias={-0.1} /></entity>
  </>
);

export const Bad = () => (
  <>
    {/* @ts-expect-error a number is not an asset ref */}
    <Gizmo handle={1} />
    {/* @ts-expect-error width is a number */}
    <Gizmo handle={ref} line_config={{ width: "4" }} />
    {/* @ts-expect-error a material ref is not a GizmoAsset ref */}
    <Gizmo handle={assets.add(StandardMaterial)} />
  </>
);
