// Type tests for the `AnimationGraph` builder (docs/animation.md): `tsc -p test` must pass, and
// every `@ts-expect-error` must be an error.
import { assets, type AssetRef } from "bevy-ts";
import { AnimationGraph, AnimationTargetId } from "bevy-ts/bevy";

export function build() {
  const g = new AnimationGraph();
  const b: number = g.addBlend(1, g.root);
  g.addClip("models/Fox.glb#Animation0", 0.5, b);
  g.addTargetToMaskGroup(AnimationTargetId.fromNames(["root", "b_Hip_01"]), 0);
  const v = g.get(b);
  if (v) v.weight = 0.2;
  const r: AssetRef<AnimationGraph> = assets.add(g);
  assets.set(r, g);
  const root: number | undefined = assets.get(r)?.root;
  void root;
  const [, n] = AnimationGraph.fromClip("models/Fox.glb#Animation1");
  const m: number = n;
  void m;

  // @ts-expect-error  a weight is a number
  g.addBlend("1", g.root);
  // @ts-expect-error  a clip is a path, ref or value, not a number
  g.addClip(3, 1, g.root);
  // @ts-expect-error  a target is names or a UUID
  g.addTargetToMaskGroup(["root"], 0);
}
