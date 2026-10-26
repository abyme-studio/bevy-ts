/** A target: `{ names }`, the names from the animation root down to it (Bevy's `from_names`), or a UUID string. */
export type AnimationTarget = string | { readonly names: readonly string[] };
/** Bevy's `AnimationTargetId`; its statics name a target for a graph's mask groups or a clip's curves. */
export declare const AnimationTargetId: ReflectClass<string, { value: string }> & {
  /** The target at `names`, from the animation root down (`["root", "b_Hip_01"]`). */
  fromNames(names: readonly string[]): AnimationTarget;
  /** `fromNames([name])`. */
  fromName(name: string): AnimationTarget;
  /** The animation root itself: `fromNames([])`. */
  readonly root: AnimationTarget;
};
export type AnimationTargetId = InstanceType<typeof AnimationTargetId>;
