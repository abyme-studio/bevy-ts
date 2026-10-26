// The `AnimationClip` builder and its curves (change `animation-control`, design 16): Bevy's
// constructors and adaptors as the JSON Rust's hand-map builds Bevy's own curves from, with no
// host call.
import { expect, test } from "vitest";
import { Transform } from "bevy-ts";
import {
  AnimatableCurve,
  AnimatableKeyframeCurve,
  AnimationClip,
  AnimationTargetId,
  animatedField,
  ConstantCurve,
  CubicBezier,
  CubicBSpline,
  CubicCardinalSpline,
  CubicHermite,
  CubicKeyframeCurve,
  CubicNurbs,
  CubicRotationCurve,
  EasingCurve,
  interval,
  LinearSpline,
  SampleAutoCurve,
  SteppedKeyframeCurve,
  UnevenSampleAutoCurve,
} from "bevy-ts/bevy";

const ease = { EasingCurve: { start: [0, 0, 0], end: [0, 2, 0], ease: "CubicOut" } };
const json = (v: unknown) => JSON.parse(JSON.stringify(v));

test("animatedField names the component's type path and field", () => {
  expect(animatedField(Transform, "translation")).toEqual({ component: "bevy_transform::components::transform::Transform", field: "translation" });
  expect(() => animatedField({} as never, "x" as never)).toThrow(/component class/);
});

test("interval gives [start, end], null for an unbounded end", () => {
  expect(interval(0, 2)).toEqual([0, 2]);
  expect(interval(0, Infinity)).toEqual([0, null]);
  expect(interval(0, null)).toEqual([0, null]);
  expect(() => interval(2, 1)).toThrow(/before start/);
});

test("each constructor is its Bevy name and args", () => {
  expect(new EasingCurve([0, 0, 0], [0, 2, 0], "CubicOut").json).toEqual(ease);
  expect(new EasingCurve(0, 1, { Steps: [4, "End"] }).json).toEqual({ EasingCurve: { start: 0, end: 1, ease: { Steps: [4, "End"] } } });
  expect(new ConstantCurve(interval(0, 1), 3).json).toEqual({ ConstantCurve: { domain: [0, 1], value: 3 } });
  expect(new SampleAutoCurve(interval(0, 1), [0, 1, 0]).json).toEqual({ SampleAutoCurve: { domain: [0, 1], samples: [0, 1, 0] } });
  for (const [C, name] of [[UnevenSampleAutoCurve, "UnevenSampleAutoCurve"], [AnimatableKeyframeCurve, "AnimatableKeyframeCurve"], [SteppedKeyframeCurve, "SteppedKeyframeCurve"]] as const) {
    expect(new C([[0, 1], [0.5, 2]]).json).toEqual({ [name]: [[0, 1], [0.5, 2]] });
  }
  expect(new CubicKeyframeCurve([0, 1], [0, 1, 0, 0, 2, 0]).json).toEqual({ CubicKeyframeCurve: { times: [0, 1], values: [0, 1, 0, 0, 2, 0] } });
  expect(new CubicRotationCurve([0], [[0, 0, 0, 1], [0, 0, 0, 1], [0, 0, 0, 1]]).json).toEqual({
    CubicRotationCurve: { times: [0], values: [[0, 0, 0, 1], [0, 0, 0, 1], [0, 0, 0, 1]] },
  });
});

test("each spline's toCurve and toCurveCyclic", () => {
  const pts = [0, 1, 3, 2];
  expect(new CubicBezier([[0, 1, 2, 3]]).toCurve().json).toEqual({ ToCurve: { CubicBezier: { control_points: [[0, 1, 2, 3]] } } });
  expect(new CubicHermite(pts, pts).toCurveCyclic().json).toEqual({ ToCurveCyclic: { CubicHermite: { control_points: pts, tangents: pts } } });
  expect(CubicCardinalSpline.newCatmullRom(pts).toCurve().json).toEqual({ ToCurve: { CubicCardinalSpline: { tension: 0.5, control_points: pts } } });
  expect(new CubicBSpline(pts).toCurve().json).toEqual({ ToCurve: { CubicBSpline: { control_points: pts } } });
  expect(new CubicNurbs(pts).toCurve().json).toEqual({ ToCurve: { CubicNurbs: { control_points: pts, weights: null, knots: null } } });
  expect(new LinearSpline(pts).toCurveCyclic().json).toEqual({ ToCurveCyclic: { LinearSpline: { points: pts } } });
});

test("adaptors wrap the curve they adapt", () => {
  const c = new EasingCurve([0, 0, 0], [0, 2, 0], "CubicOut");
  const t = c.reparametrizeLinear(interval(0, 0.5));
  expect(t.json).toEqual({ ReparametrizeLinear: [ease, [0, 0.5]] });
  expect(t.pingPong().json).toEqual({ PingPong: t.json });
  expect(c.reverse().json).toEqual({ Reverse: ease });
  expect(c.repeat(2).json).toEqual({ Repeat: [ease, 2] });
  expect(c.forever().json).toEqual({ Forever: ease });
  expect(c.chain(c).json).toEqual({ Chain: [ease, ease] });
  expect(c.chainContinue(c).json).toEqual({ ChainContinue: [ease, ease] });
});

test("a clip is an asset value of its curves on their targets", () => {
  const up = new EasingCurve([0, 0, 0], [0, 2, 0], "CubicOut").reparametrizeLinear(interval(0, 0.5));
  const clip = new AnimationClip()
    .addCurveToTarget(AnimationTargetId.root, new AnimatableCurve(animatedField(Transform, "translation"), up))
    .addCurveToTarget(AnimationTargetId.fromName("sign"), new AnimatableCurve(animatedField(Transform, "scale"), new ConstantCurve(interval(0, 1), [1, 1, 1])));
  expect(clip.duration).toBeUndefined();
  clip.setDuration(2);
  expect(json(clip)).toEqual({
    $of: "bevy_animation::AnimationClip",
    props: {
      curves: [
        [{ names: [] }, { property: { component: "bevy_transform::components::transform::Transform", field: "translation" }, curve: up.json }],
        [{ names: ["sign"] }, { property: { component: "bevy_transform::components::transform::Transform", field: "scale" }, curve: { ConstantCurve: { domain: [0, 1], value: [1, 1, 1] } } }],
      ],
      duration: 2,
    },
  });
  expect(AnimationClip.of({ duration: 1 })).toEqual({ $of: "bevy_animation::AnimationClip", props: { duration: 1 } });
});
