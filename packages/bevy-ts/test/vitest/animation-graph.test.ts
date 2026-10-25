// The `AnimationGraph` builder (change `animation-control`, design 10): Bevy's methods over the
// JSON Rust's hand-map reads and writes, with no host call.
import { expect, test } from "vitest";
import { AnimationGraph, AnimationTargetId } from "bevy-ts/bevy";

const RUN = "models/Fox.glb#Animation2";
const WALK = "models/Fox.glb#Animation1";
const node = (clip: string, weight = 1, mask = 0) => ({ node_type: { Clip: clip }, mask, weight });
const blend = (weight = 1, mask = 0) => ({ node_type: "Blend", mask, weight });

test("a new graph is one blend node, its root, and an asset value", () => {
  const g = new AnimationGraph();
  expect(g.root).toBe(0);
  expect(g.nodes()).toEqual([0]);
  expect(JSON.parse(JSON.stringify(g))).toEqual({
    $of: "bevy_animation::graph::AnimationGraph",
    props: { graph: { nodes: [blend()], node_holes: [], edge_property: "directed", edges: [] }, root: 0, mask_groups: [] },
  });
});

test("each add returns the new node's number and adds the edge from its parent", () => {
  const g = new AnimationGraph();
  const b = g.addBlend(0.5, g.root);
  expect(b).toBe(1);
  expect(g.addClip(RUN, 0.25, b)).toBe(2);
  expect(g.addClipWithMask(WALK, 3, 1, b)).toBe(3);
  expect(g.addBlendWithMask(4, 1, g.root)).toBe(4);
  expect(g.addAdditiveBlend(2, g.root)).toBe(5);
  expect(g.addAdditiveBlendWithMask(1, 1, 5)).toBe(6);
  expect(g.addClips([RUN, WALK], 0.5, 6)).toEqual([7, 8]);
  expect(g.graph.nodes).toEqual([
    blend(),
    blend(0.5),
    node(RUN, 0.25),
    node(WALK, 1, 3),
    blend(1, 4),
    { node_type: "Add", mask: 0, weight: 2 },
    { node_type: "Add", mask: 1, weight: 1 },
    node(RUN, 0.5),
    node(WALK, 0.5),
  ]);
  expect(g.graph.edges).toEqual([[0, 1, null], [1, 2, null], [1, 3, null], [0, 4, null], [0, 5, null], [5, 6, null], [6, 7, null], [6, 8, null]]);
  expect(() => g.addClip(RUN, 1, 42)).toThrow(/node 42/);
});

test("fromClip and fromClips put clips under the root", () => {
  const [g, n] = AnimationGraph.fromClip(RUN);
  expect(n).toBe(1);
  expect(g.graph.nodes[1]).toEqual(node(RUN));
  const [h, ns] = AnimationGraph.fromClips([RUN, WALK]);
  expect(ns).toEqual([1, 2]);
  expect(h.graph.edges).toEqual([[0, 1, null], [0, 2, null]]);
});

test("addEdge and removeEdge", () => {
  const g = new AnimationGraph();
  const a = g.addBlend(1, g.root);
  const c = g.addClip(RUN, 1, g.root);
  g.addEdge(a, c);
  expect(g.graph.edges).toContainEqual([a, c, null]);
  expect(g.removeEdge(a, c)).toBe(true);
  expect(g.removeEdge(a, c)).toBe(false);
  expect(g.graph.edges).toEqual([[0, 1, null], [0, 2, null]]);
});

test("get(node) writes through to the graph", () => {
  const g = new AnimationGraph();
  const c = g.addClip(RUN, 1, g.root);
  const v = g.get(c)!;
  expect(v.node_type).toEqual({ Clip: RUN });
  v.weight = 0.3;
  v.addMaskGroup(0).addMaskGroup(2).addMask(16);
  expect(g.graph.nodes[c]).toEqual(node(RUN, 0.3, 0b10101));
  v.removeMaskGroup(0).removeMask(16);
  expect(g.graph.nodes[c].mask).toBe(0b100);
  v.mask = 0;
  expect(g.graph.nodes[c].mask).toBe(0);
  expect(g.get(9)).toBeUndefined();
  expect(() => v.addMaskGroup(64)).toThrow(/mask group/);
});

test("mask groups take targets by names, by UUID and the root", () => {
  const g = new AnimationGraph();
  const leg = AnimationTargetId.fromNames(["root", "b_Hip_01"]);
  g.addTargetToMaskGroup(leg, 0);
  g.addTargetToMaskGroup(AnimationTargetId.fromNames(["root", "b_Hip_01"]), 3);
  g.addTargetToMaskGroup("4f1c-uuid", 1);
  g.addTargetToMaskGroup(AnimationTargetId.root, 2);
  expect(AnimationTargetId.fromName("tail")).toEqual({ names: ["tail"] });
  expect(g.mask_groups).toEqual([
    [{ names: ["root", "b_Hip_01"] }, 0b1001],
    ["4f1c-uuid", 0b10],
    [{ names: [] }, 0b100],
  ]);
});
