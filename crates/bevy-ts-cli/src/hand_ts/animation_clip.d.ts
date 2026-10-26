/** Bevy's `interval(start, end)`, `[start, end]`; `null` or `Infinity` for an unbounded end. */
export type CurveInterval = readonly [number | null, number | null];
export declare function interval(start: number, end: number | null): CurveInterval;
/** A value a curve holds, as its field's JSON: a number, a bool, `[x, y, z]`, `[x, y, z, w]`. */
export type CurveValue<T> = T extends number ? number : T extends boolean ? boolean : T extends ReadonlyQuatLike ? readonly [number, number, number, number] : T extends ReadonlyVec3Like ? readonly [number, number, number] : T;
interface ReadonlyVec3Like { readonly x: number; readonly y: number; readonly z: number }
interface ReadonlyQuatLike extends ReadonlyVec3Like { readonly w: number }
/** The fields of a component Bevy can animate: numbers, bools, vectors and quaternions. */
export type AnimatableKeys<C> = { [K in keyof C & string]: C[K] extends number | boolean | ReadonlyVec3Like ? K : never }[keyof C & string];
/** Bevy's `animated_field!(Component::field)`. */
export interface AnimatedField<T> {
  readonly component: string;
  readonly field: string;
  readonly __value?: T;
}
export declare function animatedField<C extends abstract new (...args: never[]) => object, K extends AnimatableKeys<InstanceType<C>>>(
  component: C,
  field: K,
): AnimatedField<CurveValue<InstanceType<C>[K]>>;
/** Bevy's `EaseFunction`, by its variant names. */
export type EaseFunction =
  | "Linear" | "QuadraticIn" | "QuadraticOut" | "QuadraticInOut" | "CubicIn" | "CubicOut" | "CubicInOut"
  | "QuarticIn" | "QuarticOut" | "QuarticInOut" | "QuinticIn" | "QuinticOut" | "QuinticInOut"
  | "SmoothStepIn" | "SmoothStepOut" | "SmoothStep" | "SmootherStepIn" | "SmootherStepOut" | "SmootherStep"
  | "SineIn" | "SineOut" | "SineInOut" | "CircularIn" | "CircularOut" | "CircularInOut"
  | "ExponentialIn" | "ExponentialOut" | "ExponentialInOut" | "ElasticIn" | "ElasticOut" | "ElasticInOut"
  | "BackIn" | "BackOut" | "BackInOut" | "BounceIn" | "BounceOut" | "BounceInOut"
  | { readonly Steps: readonly [number, "Start" | "End" | "None" | "Both"] }
  | { readonly Elastic: number };
/**
 * A curve of values `T`: Bevy's constructor or adaptor, as the JSON Rust builds it from. Each
 * constructor infers `T` as written (`[0, 2, 0]` a 3-tuple), so it fits the field it animates.
 */
export declare class Curve<T> {
  protected constructor();
  readonly json: unknown;
  private readonly __value?: T;
  /** This curve over `domain`: how an ease over 0..1 becomes N seconds. */
  reparametrizeLinear(domain: CurveInterval): Curve<T>;
  reverse(): Curve<T>;
  /** Plays the curve `count` more times after the first. */
  repeat(count: number): Curve<T>;
  /** Repeats without end: a clip of it needs `setDuration`. */
  forever(): Curve<T>;
  /** There and back. */
  pingPong(): Curve<T>;
  /** This curve, then `other`. */
  chain(other: Curve<T>): Curve<T>;
  /** This curve, then `other` offset to start where this one ends (vector values). */
  chainContinue(other: Curve<T>): Curve<T>;
}
export declare class EasingCurve<const T> extends Curve<T> {
  constructor(start: T, end: T, ease: EaseFunction);
}
export declare class ConstantCurve<const T> extends Curve<T> {
  constructor(domain: CurveInterval, value: T);
}
export declare class SampleAutoCurve<const T> extends Curve<T> {
  constructor(domain: CurveInterval, samples: Iterable<T>);
}
export declare class UnevenSampleAutoCurve<const T> extends Curve<T> {
  constructor(keyframes: Iterable<readonly [number, T]>);
}
export declare class AnimatableKeyframeCurve<const T> extends Curve<T> {
  constructor(keyframes: Iterable<readonly [number, T]>);
}
export declare class SteppedKeyframeCurve<const T> extends Curve<T> {
  constructor(keyframes: Iterable<readonly [number, T]>);
}
export declare class CubicKeyframeCurve<const T> extends Curve<T> {
  /** `values` holds three per time: in-tangent, value, out-tangent. */
  constructor(times: Iterable<number>, values: Iterable<T>);
}
export declare class CubicRotationCurve extends Curve<readonly [number, number, number, number]> {
  constructor(times: Iterable<number>, values: Iterable<readonly [number, number, number, number]>);
}
/** A spline: `toCurve()` and `toCurveCyclic()` make its curve, as Bevy's generators do. */
export declare class Spline<T> {
  protected constructor();
  toCurve(): Curve<T>;
  toCurveCyclic(): Curve<T>;
}
export declare class CubicBezier<const T> extends Spline<T> {
  /** Each segment is four control points. */
  constructor(controlPoints: Iterable<readonly [T, T, T, T]>);
}
export declare class CubicHermite<const T> extends Spline<T> {
  constructor(controlPoints: Iterable<T>, tangents: Iterable<T>);
}
export declare class CubicCardinalSpline<const T> extends Spline<T> {
  constructor(tension: number, controlPoints: Iterable<T>);
  static newCatmullRom<const T>(controlPoints: Iterable<T>): CubicCardinalSpline<T>;
}
export declare class CubicBSpline<const T> extends Spline<T> {
  constructor(controlPoints: Iterable<T>);
}
export declare class CubicNurbs<const T> extends Spline<T> {
  constructor(controlPoints: Iterable<T>, weights?: Iterable<number> | null, knots?: Iterable<number> | null);
}
export declare class LinearSpline<const T> extends Spline<T> {
  constructor(points: Iterable<T>);
}
/** Bevy's `AnimatableCurve::new(animated_field!(...), curve)`: the curve's values are the field's type. */
export declare class AnimatableCurve<T> {
  constructor(property: AnimatedField<T>, curve: Curve<NoInfer<T>>);
  readonly property: AnimatedField<T>;
  readonly curve: Curve<T>;
}
/**
 * `new AnimationClip()`: Bevy's `AnimationClip` as a builder, holding the JSON (`AnimationClip`,
 * the type) Rust builds the clip from. An asset value: `PlayAnimation.clip`, a graph's `addClip`,
 * `assets.add(clip)` and `assets.set(ref, clip)` take it.
 */
export interface AnimationClipBuilder extends AssetValue<AnimationClip> {
  readonly $of: "bevy_animation::AnimationClip";
  readonly props: AnimationClip;
  /** Adds `curve` on `target`; the duration grows to the curve's end. */
  addCurveToTarget<T>(target: AnimationTarget, curve: AnimatableCurve<T>): this;
  /** Fixes the clip's length in seconds; a clip with an unbounded curve needs one. */
  setDuration(secs: number): this;
  /** The length set with `setDuration`; `undefined` until then (Rust takes the longest curve). */
  readonly duration: number | undefined;
}
export declare const AnimationClip: AssetType<AnimationClip> & {
  readonly typePath: "bevy_animation::AnimationClip";
  /** An empty clip. */
  new (): AnimationClipBuilder;
  /** An asset value from the clip's JSON. */
  of(props?: AssetProps<AnimationClip>): AssetValue<AnimationClip>;
};
