// Unit tests for `bevy-ts/jsx-runtime` with no engine: `deno test crates/bevy-ts-v8/js/test`.
import { Fragment, jsx, jsxs, jsxWithSource, describe } from "../jsx-runtime.js";

function eq(actual, expected, msg) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${msg || "not equal"}\n  actual:   ${a}\n  expected: ${e}`);
}

function throws(fn, pattern) {
  try {
    fn();
  } catch (e) {
    if (!pattern.test(e.message)) throw new Error(`threw ${JSON.stringify(e.message)}, expected ${pattern}`);
    return e;
  }
  throw new Error(`did not throw ${pattern}`);
}

function handle(name, source = "ts") {
  const h = function (values) {
    return { type: h, values };
  };
  h.componentName = name;
  h.source = source;
  return h;
}

const Transform = handle("Transform", "rust");
const Health = handle("game::Health");

const at = (file, line, column = 3) => ({ fileName: file, lineNumber: line, columnNumber: column });
const dev = (type, props, key, source, isStatic = false) => jsxWithSource(type, props, key, isStatic, source);

function Enemy({ at: p, hp = 100 }) {
  return dev(
    "entity",
    {
      name: "enemy",
      children: [
        dev(Transform, { translation: p }, undefined, at("enemy.tsx", 5)),
        dev(Health, { initialHp: hp, max: hp }, undefined, at("enemy.tsx", 6)),
        dev("mesh", { shape: "cuboid", size: [1, 1, 1], color: "#c33" }, undefined, at("enemy.tsx", 7)),
        dev("entity", { children: dev(Transform, { translation: [0, 0.6, 0.5] }, undefined, at("enemy.tsx", 8)) }, "eye", at("enemy.tsx", 8)),
      ],
    },
    undefined,
    at("enemy.tsx", 4),
    true,
  );
}

Deno.test("an element is a description and runs nothing", () => {
  const [e] = describe(jsx(Enemy, { at: [1, 0, 0] }));
  eq(e.key, "Enemy#0");
  eq(e.type, "Enemy");
  eq(e.name, "enemy");
  const t = e.components.find((c) => c.type === "Transform");
  eq(t, { type: "Transform", kind: "rust", props: { translation: [1, 0, 0] }, source: { file: "enemy.tsx", line: 5, column: 3 } });
  eq(e.components.map((c) => [c.type, c.kind]), [["Transform", "rust"], ["game::Health", "ts"], ["mesh", "intrinsic"]]);
  eq(e.children.map((c) => c.key), ["eye"]);
});

Deno.test("keys are explicit or type-scoped among siblings", () => {
  const scene = jsxs(Fragment, {
    children: [
      jsx("entity", {}),
      jsx(Enemy, { at: [0, 0, 0] }),
      jsx("entity", {}),
      jsx(Enemy, { at: [1, 0, 0] }, "b"),
      jsx(Enemy, { at: [2, 0, 0] }),
    ],
  });
  eq(describe(scene).map((n) => n.key), ["entity#0", "Enemy#0", "entity#1", "b", "Enemy#1"]);
});

Deno.test("source lines take no part in a key", () => {
  const a = describe(jsxs(Fragment, { children: [dev(Enemy, { at: [0, 0, 0] }, "b", at("game.tsx", 12))] }));
  const b = describe(jsxs(Fragment, { children: [dev(Enemy, { at: [0, 0, 0] }, "b", at("game.tsx", 13))] }));
  eq(a[0].key, b[0].key);
  eq(a[0].source[1].line, 12);
  eq(b[0].source[1].line, 13);
});

Deno.test("an entity made in a prefab carries the prefab call sites, innermost first", () => {
  const Boss = (p) => dev(Enemy, { ...p, hp: 500 }, undefined, at("boss.tsx", 2));
  const [e] = describe(dev(Boss, { at: [0, 0, 0] }, "boss", at("game.tsx", 12)));
  eq(e.source.map((s) => `${s.file}:${s.line}`), ["enemy.tsx:4", "boss.tsx:2", "game.tsx:12"]);
  eq(e.key, "boss");
  eq(e.type, "Boss");
  const eye = e.children[0];
  eq(eye.source.map((s) => `${s.file}:${s.line}`), ["enemy.tsx:8", "boss.tsx:2", "game.tsx:12"]);
});

Deno.test("children passed into a prefab keep their own chain", () => {
  const Group = ({ children }) => dev("entity", { children }, undefined, at("group.tsx", 1));
  const [g] = describe(dev(Group, { children: dev("entity", {}, "inner", at("game.tsx", 20)) }, "g", at("game.tsx", 19)));
  eq(g.source.map((s) => s.line), [1, 19]);
  eq(g.children[0].source.map((s) => s.line), [20]);
});

Deno.test("a component given twice on one entity names the second element", () => {
  const el = dev(
    "entity",
    { children: [dev(Transform, {}, undefined, at("game.tsx", 3)), dev(Transform, {}, undefined, at("game.tsx", 4))] },
    undefined,
    at("game.tsx", 2),
    true,
  );
  const e = throws(() => describe(el), /Transform is given twice.*first at game\.tsx:3/);
  eq(e.bevyTsLocation, { file: "game.tsx", line: 4, column: 3 });
});

Deno.test("a component element outside an entity is an error", () => {
  const e = throws(() => describe(dev(Health, { max: 1 }, undefined, at("game.tsx", 7))), /outside an <entity>/);
  eq(e.bevyTsLocation.line, 7);
});

Deno.test("duplicate sibling keys name both locations", () => {
  const scene = jsxs(Fragment, {
    children: [dev(Enemy, { at: [0, 0, 0] }, "a", at("game.tsx", 3)), dev(Enemy, { at: [0, 0, 0] }, "a", at("game.tsx", 4))],
  });
  const e = throws(() => describe(scene), /key "a" \(the other at game\.tsx:3/);
  eq(e.bevyTsLocation.line, 4);
});

Deno.test("unkeyed elements from an array warn once, keyed ones do not", () => {
  const warnings = [];
  const old = console.warn;
  console.warn = (m) => warnings.push(m);
  try {
    const list = [1, 2].map((i) => dev(Enemy, { at: [i, 0, 0] }, undefined, at("game.tsx", 30)));
    describe(jsx(Fragment, { children: list }));
    describe(jsx(Fragment, { children: list }));
    const keyed = [1, 2].map((i) => dev(Enemy, { at: [i, 0, 0] }, `e${i}`, at("game.tsx", 31)));
    describe(jsx(Fragment, { children: keyed }));
    describe(jsxs(Fragment, { children: [jsx("entity", {}), jsx("entity", {})] }));
  } finally {
    console.warn = old;
  }
  eq(warnings.length, 1);
  if (!/no key.*game\.tsx:30/.test(warnings[0])) throw new Error(warnings[0]);
});

Deno.test("a keyed fragment from a prefab prefixes its children's keys", () => {
  const Pair = () => jsxs(Fragment, { children: [jsx("entity", {}, "l"), jsx("entity", {}, "r")] });
  eq(describe(jsxs(Fragment, { children: [jsx(Pair, {}, "p"), jsx(Pair, {})] })).map((n) => n.key), ["p/l", "p/r", "Pair#0/l", "Pair#0/r"]);
});

Deno.test("function props become bindings carried as ids", () => {
  const [e] = describe(jsx("entity", { children: jsx(Health, { max: () => 3 }) }));
  eq(e.components[0].props.max, { $b: 1, $s: 1 });
});

Deno.test("vector views cross as arrays", () => {
  const v = { x: 1, y: 2, z: 3, toArray: () => [1, 2, 3] };
  const [e] = describe(jsx("entity", { children: jsx(Transform, { translation: v }) }));
  eq(e.components[0].props.translation, [1, 2, 3]);
});

Deno.test("a scene component splits fields from scene props and adds itself to its root", () => {
  class Grunt {
    static scene({ speed = 2 }) {
      return dev("entity", { name: "grunt", children: dev(Transform, { translation: [speed, 0, 0] }, undefined, at("grunt.tsx", 3)) }, undefined, at("grunt.tsx", 2));
    }
  }
  Grunt.componentName = "enemies::Grunt";
  Grunt.source = "ts";
  Grunt.fieldNames = ["team"];
  const [g] = describe(jsxs(Fragment, { children: [dev(Grunt, { team: 1, speed: 3 }, "g1", at("level.tsx", 9)), jsx(Grunt, { initialTeam: 2 })] }));
  eq(g.key, "g1");
  eq(g.type, "enemies::Grunt");
  eq(g.components.map((c) => [c.type, c.props]), [["enemies::Grunt", { team: 1 }], ["Transform", { translation: [3, 0, 0] }]]);
  eq(g.source.map((s) => `${s.file}:${s.line}`), ["grunt.tsx:2", "level.tsx:9"]);
  const all = describe(jsxs(Fragment, { children: [jsx(Grunt, {}), jsx(Grunt, { initialTeam: 2 })] }));
  eq(all.map((n) => n.key), ["enemies::Grunt#0", "enemies::Grunt#1"]);
  eq(all[1].components[0].props, { initialTeam: 2 });
});

Deno.test("a scene component whose field and scene prop share a name fails", () => {
  class Bad {
    static scene() {
      return jsx("entity", {});
    }
  }
  Bad.componentName = "Bad";
  Bad.source = "ts";
  Bad.fieldNames = ["speed"];
  Bad.sceneProps = ["speed"];
  throws(() => jsx(Bad, {}), /speed is both a field and a scene prop/);
});

Deno.test("worldText is a component of its entity, its children its text", () => {
  const [e] = describe(
    jsx("entity", { children: [jsx(Transform, { translation: [0, 2, 0] }), jsxs("worldText", { size: 0.5, children: ["Par", "is"] })] }),
  );
  eq(e.components.map((c) => [c.type, c.kind]), [["Transform", "rust"], ["worldText", "intrinsic"]]);
  eq(e.components[1].props, { size: 0.5, text: "Paris" });
  eq(e.children, []);
  const [b] = describe(jsx("entity", { children: jsxs("worldText", { children: ["HP ", () => 9] }) }));
  if (!b.components[0].props.text.$b) throw new Error("a bound child makes text a binding");
});

Deno.test("worldText refuses an element child and a place outside an entity", () => {
  const inside = throws(() => describe(jsx("entity", { children: dev("worldText", { children: dev("span", { children: "x" }, undefined, at("game.tsx", 9)) }, undefined, at("game.tsx", 8)) })), /<worldText> takes strings.*not <span>/);
  eq(inside.bevyTsLocation.line, 9);
  const outside = throws(() => describe(dev("worldText", { children: "A" }, undefined, at("game.tsx", 4))), /<worldText> is a component element outside an <entity>/);
  eq(outside.bevyTsLocation.line, 4);
});
