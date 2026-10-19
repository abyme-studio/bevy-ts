// The reactive core and the tsx host-call protocol, with no engine: `deno test crates/bevy-ts-v8/js/test`.
import {
  Fragment, For, Show, TsxContext, batch, computed, describe, effect, jsx, jsxs, onCleanup, ref, runScene, setHost, signal, untrack,
} from "../jsx-runtime.js";

function eq(actual, expected, msg) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${msg || "not equal"}\n  actual:   ${a}\n  expected: ${e}`);
}

function handle(name, fields = {}) {
  const h = function (values) {
    return { type: h, values };
  };
  h.componentName = name;
  h.source = "ts";
  h.fields = fields;
  return h;
}

const Health = handle("game::Health");
const Transform = handle("Transform");
Transform.source = "rust";
Transform.fields = { translation: { spec: { kind: "vec3" } } };

Deno.test("signals, effects and computed values re-run only on change", () => {
  const [count, setCount] = signal(1);
  const doubled = computed(() => count() * 2);
  const seen = [];
  const stop = effect(() => seen.push(doubled()));
  setCount(2);
  setCount(2);
  batch(() => {
    setCount(3);
    setCount(4);
  });
  eq(seen, [2, 4, 8]);
  stop();
  setCount(5);
  eq(seen, [2, 4, 8], "a stopped effect does not run");
});

Deno.test("untrack and onCleanup", () => {
  const [a, setA] = signal(0);
  const [b, setB] = signal(0);
  const runs = [];
  const cleaned = [];
  effect(() => {
    runs.push(`${a()}:${untrack(b)}`);
    onCleanup(() => cleaned.push(a()));
  });
  setB(1);
  setA(1);
  eq(runs, ["0:0", "1:1"]);
  eq(cleaned, [1]);
});

/** A HUD as a scene describes it, then the host calls Rust would make. */
function hud(player) {
  return jsxs(Fragment, {
    children: [
      jsx("entity", { ref: player }),
      jsx("node", {
        children: jsx("text", { size: 20, children: ["HP ", () => player.get(Health).hp, " / ", () => player.get(Health).max] }),
      }),
    ],
  });
}

Deno.test("a binding waits for its ref and value, then patches once per change", () => {
  const ctx = new TsxContext();
  const player = ref();
  const [p, node] = describe(hud(player), ctx);
  eq(p.ref, 1);
  const text = node.children[0].components[0];
  eq(text.type, "text");
  eq(text.props, { size: 20, text: { $b: 1 } });

  let out = ctx.call({});
  eq(out.patches, [], "the ref is not mounted yet");
  out = ctx.call({ refs: [[1, 42]] });
  eq(out.needs, [[42, "game::Health"]]);
  eq(out.subscribe, [[42, "game::Health"]], "watched from the first read");
  eq(out.patches, []);
  out = ctx.call({ values: [[42, "game::Health", { hp: 80, max: 100 }]] });
  eq(out.patches, [[1, "HP 80 / 100"]]);
  eq(out.subscribe, []);
  eq(out.needs, []);

  out = ctx.call({ values: [[42, "game::Health", { hp: 80, max: 100 }]] });
  eq(out.patches, [], "the same value writes nothing");
  out = ctx.call({ values: [[42, "game::Health", { hp: 70, max: 100 }]] });
  eq(out.patches, [[1, "HP 70 / 100"]]);
  eq(out.subscribe, []);
});

Deno.test("a ref wrapping an entity and vector fields", () => {
  const ctx = new TsxContext();
  const target = ref(7);
  describe(jsx("entity", { children: jsx(Health, { max: () => target.get(Transform).translation.y * 2 }) }), ctx);
  // A site instance that reads another entity takes the per-pair path.
  eq(ctx.call({ instances: [[1, 99]] }).needs, [[7, "Transform"]]);
  const out = ctx.call({ values: [[7, "Transform", { translation: [1, 2, 3] }]] });
  eq(out.patches, [[1, 4]]);
});

Deno.test("a signal set outside a host call asks for one and the next call carries the patch", () => {
  const ctx = new TsxContext();
  const asked = [];
  setHost({ requestFlush: () => (asked.push(1), true) });
  try {
    const [label, setLabel] = signal("a");
    describe(jsx("text", { children: () => label() }), ctx);
    eq(ctx.call({}).patches, [[1, "a"]]);
    setLabel("b");
    setLabel("c");
    eq(asked.length, 1, "one request until the call");
    eq(ctx.call({}).patches, [[1, "c"]]);
  } finally {
    setHost(null);
  }
});

Deno.test("Show mounts and unmounts its subtree and disposes its bindings", () => {
  const ctx = new TsxContext();
  const [open, setOpen] = signal(false);
  const player = ref(9);
  const [slot] = describe(
    jsx(Show, {
      when: () => open(),
      fallback: jsx("text", { children: "closed" }),
      children: jsx("text", { children: () => `hp ${player.get(Health).hp}` }),
    }),
    ctx,
  );
  eq(slot.slot, 1);
  eq(slot.key, "Show#0");
  let out = ctx.call({});
  eq(out.mounts.map(([s, g, nodes]) => [s, g, nodes.map((n) => n.components[0].props.text)]), [[1, "else", ["closed"]]]);
  setOpen(true);
  out = ctx.call({});
  eq(out.unmounts, [[1, "else"]]);
  eq(out.mounts[0][1], "then");
  eq(out.needs, [[9, "game::Health"]]);
  eq(out.subscribe, [[9, "game::Health"]]);
  out = ctx.call({ values: [[9, "game::Health", { hp: 3 }]] });
  eq(out.patches.length, 1);
  setOpen(false);
  out = ctx.call({});
  eq(out.unmounts, [[1, "then"]]);
  eq(out.unsubscribe, [[9, "game::Health"]], "the disposed binding stops watching");
});

Deno.test("For keeps items by key and unmounts only the removed one", () => {
  const ctx = new TsxContext();
  const [items, setItems] = signal([{ id: "a" }, { id: "b" }, { id: "c" }]);
  describe(
    jsx(For, { each: () => items(), key: (i) => i.id, children: (i) => jsx("entity", { name: i.id }) }),
    ctx,
  );
  let out = ctx.call({});
  eq(out.mounts.map(([, g, n]) => [g, n[0].name]), [["a", "a"], ["b", "b"], ["c", "c"]]);
  setItems((xs) => xs.filter((x) => x.id !== "b"));
  out = ctx.call({});
  eq(out.unmounts, [[1, "b"]]);
  eq(out.mounts, []);
});

Deno.test("a read outside a binding warns once with its location", () => {
  const warnings = [];
  const old = console.warn;
  console.warn = (m) => warnings.push(m);
  try {
    const [n] = signal(1);
    const Bad = () => jsx("text", { children: String(n()) });
    const ctx = new TsxContext();
    runScene(ctx, () => describe(jsxs(Fragment, { children: [jsx(Bad, {}), jsx(Bad, {})] }), ctx));
    effect(() => n());
  } finally {
    console.warn = old;
  }
  eq(warnings.length, 1, warnings.join("\n"));
  if (!/read outside a binding.*reactive\.test\.js/.test(warnings[0])) throw new Error(warnings[0]);
});

Deno.test("a throwing binding reports its location and others carry on", () => {
  const ctx = new TsxContext();
  describe(
    jsxs("entity", {
      children: [
        jsx(Health, { max: () => { throw new Error("nope"); } }, undefined),
        jsx(Transform, { translation: () => [1, 2, 3] }),
      ],
    }),
    ctx,
  );
  const out = ctx.call({ instances: [[1, 5], [2, 5]] });
  eq(out.patches, [[2, [1, 2, 3]]]);
  eq(out.errors.length, 1);
  if (!/nope/.test(out.errors[0].message)) throw new Error(out.errors[0].message);
});

/** Siren: a light that follows its own entity's alarm, instanced. */
function sirens(ctx, n, extra) {
  const Alarm = handle("game::Alarm");
  const Light = handle("game::Light");
  const Siren = () => {
    const self = ref();
    return jsx("entity", { ref: self, children: jsx(Light, { intensity: () => (self.get(Alarm).on ? 8 : 2) + (extra ? extra() : 0) }) });
  };
  const nodes = describe(jsxs(Fragment, { children: Array.from({ length: n }, (_, i) => jsx(Siren, {}, `s${i}`)) }), ctx);
  return { nodes, Alarm };
}

Deno.test("bindings on native fields run per site, row by row, with no effects", () => {
  const ctx = new TsxContext();
  const { nodes } = sirens(ctx, 3);
  eq(nodes.map((n) => n.components[0].props.intensity), [{ $b: 1, $s: 1 }, { $b: 2, $s: 1 }, { $b: 3, $s: 1 }]);
  let out = ctx.call({ refs: [[1, 10], [2, 11], [3, 12]], instances: [[1, 10], [2, 11], [3, 12]] });
  eq(out.needs, [[10, "game::Alarm"], [11, "game::Alarm"], [12, "game::Alarm"]]);
  eq(out.subscribe, [], "own reads are not per-pair subscriptions");
  out = ctx.call({ values: [10, 11, 12].map((e) => [e, "game::Alarm", { on: 0 }]) });
  eq(out.patches, [[1, 2], [2, 2], [3, 2]]);
  eq(out.sites, [[1, ["game::Alarm"]]], "the site's reads, fixed at its first run");
  // Rust finds the changed rows; only they re-run, and a same value writes nothing.
  out = ctx.call({ rows: [[2, [["game::Alarm", { on: 1 }]]], [3, [["game::Alarm", { on: 0 }]]]] });
  eq(out.patches, [[2, 8]]);
});

Deno.test("a signal a site reads re-runs every instance; a new read is an error naming the site", () => {
  const ctx = new TsxContext();
  const [boost, setBoost] = signal(0);
  sirens(ctx, 2, () => boost());
  ctx.call({ refs: [[1, 10], [2, 11]], instances: [[1, 10], [2, 11]] });
  ctx.call({ values: [[10, "game::Alarm", { on: 0 }], [11, "game::Alarm", { on: 1 }]] });
  setBoost(1);
  eq(ctx.call({}).patches, [[1, 3], [2, 9]]);
});

Deno.test("a site run that reads a component its first run did not is an error naming the site", () => {
  const ctx = new TsxContext();
  const Alarm = handle("game::Alarm");
  const Heat = handle("game::Heat");
  const Light = handle("game::Light");
  const self = ref();
  describe(jsx("entity", { ref: self, children: jsx(Light, { intensity: () => (self.get(Alarm).on ? self.get(Heat).t : 2) }) }), ctx);
  ctx.call({ refs: [[1, 10]], instances: [[1, 10]] });
  ctx.call({ values: [[10, "game::Alarm", { on: 0 }]] });
  const out = ctx.call({ rows: [[1, [["game::Alarm", { on: 1 }]]]] });
  eq(out.errors.length, 1);
  if (!/did not read on its first run/.test(out.errors[0].message)) throw new Error(out.errors[0].message);
});
