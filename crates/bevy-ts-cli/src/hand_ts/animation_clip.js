/**
 * Bevy's `AnimationClip` as a builder, and the curves that go in it (change `animation-control`,
 * design 16): Bevy's constructors and adaptors by their names, each a JSON value Rust's hand-map
 * turns into Bevy's own curve. Building makes no host call. A clip is an asset value: it goes in
 * `PlayAnimation.clip`, a graph's `addClip`, `assets.add(clip)` and `assets.set(ref, clip)`.
 */
const CLIP_PATH = "bevy_animation::AnimationClip";

/** Bevy's `interval(start, end)`: `null` or `Infinity` for an unbounded end. */
export function interval(start, end) {
  const n = (x) => (x === null || !Number.isFinite(x) ? null : x);
  if (start !== null && end !== null && Number.isFinite(start) && Number.isFinite(end) && end < start) throw new RangeError(`interval(${start}, ${end}): end is before start`);
  return Object.freeze([n(start), n(end)]);
}

/** Bevy's `animated_field!(Component::field)`: a reflected component's top-level field. */
export function animatedField(component, field) {
  const path = component?.typePath ?? component?.path ?? component?.componentName;
  if (typeof path !== "string") throw new TypeError("animatedField(Component, field): Component is a component class (Transform)");
  return Object.freeze({ component: path, field: String(field) });
}

/** A curve: Bevy's constructor or adaptor, as the JSON Rust builds it from. */
class Curve {
  constructor(json) {
    this.json = json;
  }
  toJSON() {
    return this.json;
  }
  /** Bevy's `reparametrize_linear`: this curve over `domain` (how an ease over 0..1 becomes N seconds). */
  reparametrizeLinear(domain) {
    return new Curve({ ReparametrizeLinear: [this.json, domain] });
  }
  reverse() {
    return new Curve({ Reverse: this.json });
  }
  /** Plays the curve `count` more times after the first. */
  repeat(count) {
    return new Curve({ Repeat: [this.json, count] });
  }
  /** Repeats without end: a clip of it needs `setDuration`. */
  forever() {
    return new Curve({ Forever: this.json });
  }
  /** There and back. */
  pingPong() {
    return new Curve({ PingPong: this.json });
  }
  /** This curve, then `other`. */
  chain(other) {
    return new Curve({ Chain: [this.json, other.json] });
  }
  /** This curve, then `other` offset to start where this one ends (vector values). */
  chainContinue(other) {
    return new Curve({ ChainContinue: [this.json, other.json] });
  }
}

export class EasingCurve extends Curve {
  constructor(start, end, ease) {
    super({ EasingCurve: { start, end, ease } });
  }
}
export class ConstantCurve extends Curve {
  constructor(domain, value) {
    super({ ConstantCurve: { domain, value } });
  }
}
export class SampleAutoCurve extends Curve {
  constructor(domain, samples) {
    super({ SampleAutoCurve: { domain, samples: [...samples] } });
  }
}
const timed = (name) =>
  class extends Curve {
    constructor(keyframes) {
      super({ [name]: [...keyframes].map(([t, v]) => [t, v]) });
    }
  };
export const UnevenSampleAutoCurve = timed("UnevenSampleAutoCurve");
export const AnimatableKeyframeCurve = timed("AnimatableKeyframeCurve");
export const SteppedKeyframeCurve = timed("SteppedKeyframeCurve");
export class CubicKeyframeCurve extends Curve {
  /** `values` holds three per time: in-tangent, value, out-tangent. */
  constructor(times, values) {
    super({ CubicKeyframeCurve: { times: [...times], values: [...values] } });
  }
}
export class CubicRotationCurve extends Curve {
  constructor(times, values) {
    super({ CubicRotationCurve: { times: [...times], values: [...values] } });
  }
}

/** A spline: `toCurve()` and `toCurveCyclic()` make its curve, as Bevy's generators do. */
class Spline {
  constructor(json) {
    this.json = json;
  }
  toCurve() {
    return new Curve({ ToCurve: this.json });
  }
  toCurveCyclic() {
    return new Curve({ ToCurveCyclic: this.json });
  }
}
export class CubicBezier extends Spline {
  /** Each segment is four control points. */
  constructor(controlPoints) {
    super({ CubicBezier: { control_points: [...controlPoints] } });
  }
}
export class CubicHermite extends Spline {
  constructor(controlPoints, tangents) {
    super({ CubicHermite: { control_points: [...controlPoints], tangents: [...tangents] } });
  }
}
export class CubicCardinalSpline extends Spline {
  constructor(tension, controlPoints) {
    super({ CubicCardinalSpline: { tension, control_points: [...controlPoints] } });
  }
  /** Bevy's `new_catmull_rom`: tension 0.5. */
  static newCatmullRom(controlPoints) {
    return new CubicCardinalSpline(0.5, controlPoints);
  }
}
export class CubicBSpline extends Spline {
  constructor(controlPoints) {
    super({ CubicBSpline: { control_points: [...controlPoints] } });
  }
}
export class CubicNurbs extends Spline {
  constructor(controlPoints, weights = null, knots = null) {
    super({ CubicNurbs: { control_points: [...controlPoints], weights: weights && [...weights], knots: knots && [...knots] } });
  }
}
export class LinearSpline extends Spline {
  constructor(points) {
    super({ LinearSpline: { points: [...points] } });
  }
}

/** Bevy's `AnimatableCurve::new(animated_field!(...), curve)`. */
export class AnimatableCurve {
  constructor(property, curve) {
    this.property = property;
    this.curve = curve;
  }
  toJSON() {
    return { property: this.property, curve: this.curve.json };
  }
}

export class AnimationClip {
  static typePath = CLIP_PATH;
  /** `AnimationClip.of(props)`: an asset value from the clip's JSON. */
  static of(props = {}) {
    return Object.freeze({ $of: CLIP_PATH, props });
  }

  /** An empty clip (Bevy's `AnimationClip::default`). */
  constructor() {
    this.$of = CLIP_PATH;
    this.props = { curves: [] };
  }
  /** Adds `curve` (an `AnimatableCurve`) on `target`; the duration grows to the curve's end. */
  addCurveToTarget(target, curve) {
    this.props.curves.push([target, curve.toJSON()]);
    return this;
  }
  /** Fixes the clip's length in seconds; a clip with an unbounded curve needs one. */
  setDuration(secs) {
    this.props.duration = secs;
    return this;
  }
  /** The length set with `setDuration`; `undefined` until then (Rust takes the longest curve). */
  get duration() {
    return this.props.duration;
  }
}
