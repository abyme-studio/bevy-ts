/**
 * Bevy's `AnimationGraph` as a builder (change `animation-control`, design 10): Bevy's methods,
 * camelCased, over the JSON Rust's hand-map reads and writes (Bevy's `SerializedAnimationGraph`
 * shape, `mask_groups` as `[target, mask]` pairs). Building makes no host call. A graph is an
 * asset value: it goes in a `Handle<AnimationGraph>` slot (`<PlayAnimation graph={g} />`),
 * `assets.add(g)` and `assets.set(ref, g)`. Node numbers are this graph's own, as Bevy returns them.
 */
const GRAPH_PATH = "bevy_animation::graph::AnimationGraph";

// Masks are u64 bitfields in Rust, numbers here: exact up to bit 52.
const bits = (f) => (a, b) => Number(BigInt.asUintN(64, f(BigInt(a), BigInt(b))));
const or = bits((a, b) => a | b);
const andNot = bits((a, b) => a & ~b);
const bit = (group) => {
  if (!Number.isInteger(group) || group < 0 || group > 52) throw new RangeError(`a mask group is a whole number from 0 to 52, not ${group}`);
  return 2 ** group;
};
const sameTarget = (a, b) => JSON.stringify(a) === JSON.stringify(b);

export class AnimationGraph {
  static typePath = GRAPH_PATH;

  /** A graph with one blend node, its root (Bevy's `AnimationGraph::new`). */
  constructor() {
    this.$of = GRAPH_PATH;
    this.props = {
      graph: { nodes: [{ node_type: "Blend", mask: 0, weight: 1 }], node_holes: [], edge_property: "directed", edges: [] },
      root: 0,
      mask_groups: [],
    };
  }

  /** A graph playing one clip under its root: `[graph, node]`. */
  static fromClip(clip) {
    const g = new AnimationGraph();
    return [g, g.addClip(clip, 1, g.root)];
  }

  /** A graph playing each clip under its root: `[graph, nodes]`. */
  static fromClips(clips) {
    const g = new AnimationGraph();
    return [g, g.addClips(clips, 1, g.root)];
  }

  get root() {
    return this.props.root;
  }
  set root(n) {
    this.#node(n);
    this.props.root = n;
  }
  get graph() {
    return this.props.graph;
  }
  get mask_groups() {
    return this.props.mask_groups;
  }

  #add(node_type, mask, weight, parent) {
    this.#node(parent);
    const nodes = this.props.graph.nodes;
    nodes.push({ node_type, mask, weight });
    const n = nodes.length - 1;
    this.props.graph.edges.push([parent, n, null]);
    return n;
  }

  #node(n) {
    if (!Number.isInteger(n) || n < 0 || n >= this.props.graph.nodes.length) throw new RangeError(`node ${n} is not in this graph`);
    return n;
  }

  /** A clip node under `parent`; `clip` is an asset path (`"models/Fox.glb#Animation0"`), an `AnimationClip` ref or value. */
  addClip(clip, weight, parent) {
    return this.#add({ Clip: clip }, 0, weight, parent);
  }
  addClipWithMask(clip, mask, weight, parent) {
    return this.#add({ Clip: clip }, mask, weight, parent);
  }
  addClips(clips, weight, parent) {
    return [...clips].map((c) => this.addClip(c, weight, parent));
  }
  /** A blend node: its children's weights are normalized to 1. */
  addBlend(weight, parent) {
    return this.#add("Blend", 0, weight, parent);
  }
  addBlendWithMask(mask, weight, parent) {
    return this.#add("Blend", mask, weight, parent);
  }
  /** An additive node: each child is added, scaled by its weight. */
  addAdditiveBlend(weight, parent) {
    return this.#add("Add", 0, weight, parent);
  }
  addAdditiveBlendWithMask(mask, weight, parent) {
    return this.#add("Add", mask, weight, parent);
  }
  /** Makes `to` a child of `from` too. */
  addEdge(from, to) {
    this.props.graph.edges.push([this.#node(from), this.#node(to), null]);
  }
  /** Removes the edge from `from` to `to`; false when there was none. */
  removeEdge(from, to) {
    const edges = this.props.graph.edges;
    const i = edges.findIndex(([a, b]) => a === from && b === to);
    if (i < 0) return false;
    edges.splice(i, 1);
    return true;
  }
  /** A live view of node `n`: `weight` and `mask` write through, as Bevy's `get_mut`; `undefined` when there is no such node. */
  get(n) {
    const node = this.props.graph.nodes[n];
    return Number.isInteger(n) && node ? new AnimationGraphNode(node) : undefined;
  }
  /** Every node number, in order. */
  nodes() {
    return this.props.graph.nodes.map((_, i) => i);
  }
  /** Puts `target` (`AnimationTargetId.fromNames(...)` or a UUID) in mask group `group`. */
  addTargetToMaskGroup(target, group) {
    const groups = this.props.mask_groups;
    const entry = groups.find(([t]) => sameTarget(t, target));
    if (entry) entry[1] = or(entry[1], bit(group));
    else groups.push([target, bit(group)]);
  }
}

/** One node of a graph, as `AnimationGraph.get(n)` gives it. */
class AnimationGraphNode {
  #node;
  constructor(node) {
    this.#node = node;
  }
  /** `"Blend"`, `"Add"` or `{ Clip: clip }`. */
  get node_type() {
    return this.#node.node_type;
  }
  get weight() {
    return this.#node.weight;
  }
  set weight(w) {
    this.#node.weight = w;
  }
  /** The mask groups this node and its descendants do not animate, one bit each. */
  get mask() {
    return this.#node.mask;
  }
  set mask(m) {
    this.#node.mask = m;
  }
  addMask(mask) {
    this.#node.mask = or(this.#node.mask, mask);
    return this;
  }
  removeMask(mask) {
    this.#node.mask = andNot(this.#node.mask, mask);
    return this;
  }
  addMaskGroup(group) {
    return this.addMask(bit(group));
  }
  removeMaskGroup(group) {
    return this.removeMask(bit(group));
  }
}
