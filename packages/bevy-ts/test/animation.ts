// Type tests for the `AnimationGraph` and `AnimationClip` builders (docs/animation.md): `tsc -p test` must pass, and
// every `@ts-expect-error` must be an error.
import { assets, Transform, type AssetRef } from "bevy-ts";
import { AnimatableCurve, animatedField, AnimationClip, AnimationGraph, AnimationTargetId, ConstantCurve, EasingCurve, interval, UiTransform } from "bevy-ts/bevy";

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

// Type tests for the `AnimationClip` builder and its curves.
export function clip() {
  const up = new EasingCurve<readonly [number, number, number]>([0, 0, 0], [0, 2, 0], "CubicOut").reparametrizeLinear(interval(0, 0.5));
  const c = new AnimationClip().addCurveToTarget(AnimationTargetId.root, new AnimatableCurve(animatedField(Transform, "translation"), up));
  c.setDuration(1);
  const r: AssetRef<AnimationClip> = assets.add(c);
  assets.set(r, c);
  const d: number | undefined = assets.get(r)?.duration;
  void d;

  // A Vec2 field is a Vec3 view whose z reads 0: its curve takes [x, y, 0] (a UI pop).
  new AnimatableCurve(animatedField(UiTransform, "scale"), new EasingCurve([1, 1, 0], [1.2, 1.2, 0], "BackOut"));

  // @ts-expect-error  a translation curve holds Vec3 values, not numbers
  new AnimatableCurve(animatedField(Transform, "translation"), new ConstantCurve(interval(0, 1), 3));
  // @ts-expect-error  Transform has no field `nope`
  animatedField(Transform, "nope");
  // @ts-expect-error  an ease is one of Bevy's EaseFunction names
  new EasingCurve(0, 1, "Wobbly");
}
