/**
 * Bevy's `AnimationTargetId`, and how TS names one: `fromNames(names)`, the names from the
 * animation root down to the target, which Rust hashes with Bevy's `from_names`; `fromName(name)`;
 * `root`, the animation root itself. A UUID string names one too.
 */
export const AnimationTargetId = Object.assign(reflectClass("AnimationTargetId"), {
  fromNames: (names) => Object.freeze({ names: Object.freeze([...names].map(String)) }),
  fromName: (name) => Object.freeze({ names: Object.freeze([String(name)]) }),
  root: Object.freeze({ names: Object.freeze([]) }),
});
