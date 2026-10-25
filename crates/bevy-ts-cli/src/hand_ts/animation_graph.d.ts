/** A clip in a graph: an asset path with a label, or an `AnimationClip` ref or value. */
export type ClipSource = string | AssetRef<AnimationClip> | AssetValue<AnimationClip>;
/** One node of a graph's JSON. */
export type AnimationGraphNodeJson = AnimationGraph["graph"]["nodes"][number];
/**
 * `new AnimationGraph()`: Bevy's `AnimationGraph` as a builder, Bevy's methods camelCased, holding
 * the JSON (`AnimationGraph`, the type) Rust reads and writes. An asset value: a
 * `Handle<AnimationGraph>` slot (`<PlayAnimation graph={g} />`), `assets.add(g)` and
 * `assets.set(ref, g)` take it. Node numbers are this graph's own.
 */
export interface AnimationGraphBuilder extends AssetValue<AnimationGraph> {
  readonly $of: "bevy_animation::graph::AnimationGraph";
  readonly props: AnimationGraph;
  root: number;
  readonly graph: AnimationGraph["graph"];
  readonly mask_groups: AnimationGraph["mask_groups"];
  addClip(clip: ClipSource, weight: number, parent: number): number;
  addClipWithMask(clip: ClipSource, mask: number, weight: number, parent: number): number;
  addClips(clips: Iterable<ClipSource>, weight: number, parent: number): number[];
  /** A blend node: its children's weights are normalized to 1. */
  addBlend(weight: number, parent: number): number;
  addBlendWithMask(mask: number, weight: number, parent: number): number;
  /** An additive node: each child is added, scaled by its weight. */
  addAdditiveBlend(weight: number, parent: number): number;
  addAdditiveBlendWithMask(mask: number, weight: number, parent: number): number;
  /** Makes `to` a child of `from` too. */
  addEdge(from: number, to: number): void;
  /** Removes the edge from `from` to `to`; false when there was none. */
  removeEdge(from: number, to: number): boolean;
  /** A live view of node `n`: `weight` and `mask` write through. */
  get(n: number): AnimationGraphNode | undefined;
  /** Every node number, in order. */
  nodes(): number[];
  /** Puts `target` in mask group `group` (0 to 52). */
  addTargetToMaskGroup(target: AnimationTarget, group: number): void;
}
export declare const AnimationGraph: AssetType<AnimationGraph> & {
  readonly typePath: "bevy_animation::graph::AnimationGraph";
  /** A graph with one blend node, its root. */
  new (): AnimationGraphBuilder;
  /** A graph playing one clip under its root, and the clip's node. */
  fromClip(clip: ClipSource): [AnimationGraphBuilder, number];
  /** A graph playing each clip under its root, and their nodes. */
  fromClips(clips: Iterable<ClipSource>): [AnimationGraphBuilder, number[]];
};
/** One node of a graph, as `AnimationGraph.get(n)` gives it. */
export interface AnimationGraphNode {
  readonly node_type: AnimationGraphNodeJson["node_type"];
  weight: number;
  /** The mask groups this node and its descendants do not animate, one bit each. */
  mask: number;
  addMask(mask: number): this;
  removeMask(mask: number): this;
  addMaskGroup(group: number): this;
  removeMaskGroup(group: number): this;
}
