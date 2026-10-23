// The reflected-value decoder (reflect-coverage decision 7) on hand-made bytes: every op, in the
// wire format `bevy_ts::reflect_codec` writes, decodes to the JSON `reflect_value::read` gives.
// No engine: `deno test --import-map=crates/bevy-ts-v8/js/test/import_map.json crates/bevy-ts-v8/js/test`.
import { __reflectCheck } from "../core.js";

function eq(actual, expected, msg) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`${msg || "not equal"}\n  actual:   ${a}\n  expected: ${e}`);
}

class Bytes {
  b = [];
  put(n, f) {
    const v = new DataView(new ArrayBuffer(n));
    f(v);
    this.b.push(...new Uint8Array(v.buffer));
    return this;
  }
  f32(x) { return this.put(4, (v) => v.setFloat32(0, x, true)); }
  f64(x) { return this.put(8, (v) => v.setFloat64(0, x, true)); }
  u32(x) { return this.put(4, (v) => v.setUint32(0, x, true)); }
  i16(x) { return this.put(2, (v) => v.setInt16(0, x, true)); }
  u8(x) { this.b.push(x); return this; }
  str(s) { const u = new TextEncoder().encode(s); this.u32(u.length); this.b.push(...u); return this; }
}

const num = (t) => ({ k: "num", t });

Deno.test("every op decodes to read's JSON", () => {
  const codec = {
    wrap: false,
    node: {
      k: "struct",
      fields: [
        ["a", num("f32")],
        ["nan", num("f32")],
        ["small", { k: "wide" }],
        ["big", { k: "wide" }],
        ["name", { k: "str" }],
        ["v", { k: "lanes", t: "f32", n: 3 }],
        ["list", { k: "list", of: { k: "entity" } }],
        ["none", { k: "option", of: num("u8") }],
        ["some", { k: "option", of: num("u8") }],
        ["d", { k: "duration" }],
        ["t", { k: "tuple", items: [num("i16"), num("bool")] }],
        ["map", { k: "strMap", of: num("f64") }],
        ["set", { k: "json" }],
        [
          "e",
          {
            k: "enum",
            variants: [
              { name: "Wide", shape: "unit", items: [] },
              { name: "Ortho", shape: "newtype", items: [{ k: "struct", fields: [["scale", num("f32")]] }] },
              { name: "Pair", shape: "tuple", items: [num("u8"), num("u8")] },
              { name: "Box", shape: "struct", items: [num("u8")], fields: ["side"] },
            ],
          },
        ],
        ["unit", { k: "enum", variants: [{ name: "Wide", shape: "unit", items: [] }] }],
      ],
    },
  };
  const b = new Bytes()
    .f32(0.1).f32(Infinity)
    .u8(0).f64(7).u8(1).str("18446744073709551615")
    .str("laser")
    .f32(1).f32(2).f32(3)
    .u32(2).u32(42).u32(7).u32(1).u32(0)
    .u8(0).u8(1).u8(9)
    .f64(1.5)
    .i16(-3).u8(1)
    .u32(1).str("k").f64(2.5)
    .str("[1,2]")
    .u32(1).f32(2)
    .u32(0);
  const json = JSON.stringify({
    a: Math.fround(0.1), nan: "Infinity", small: 7, big: "18446744073709551615", name: "laser", v: [1, 2, 3],
    list: [42 + 7 * 4294967296, 1], none: null, some: 9, d: 1.5, t: [-3, true], map: { k: 2.5 }, set: [1, 2],
    e: { Ortho: { scale: 2 } }, unit: "Wide",
  });
  eq(__reflectCheck([{ type: "Fixture", codec, bytes: b.b, json }]), []);
});

Deno.test("a newtype component reads as { value }, and a mismatch is reported", () => {
  const codec = { wrap: true, node: { k: "list", of: num("u32") } };
  const bytes = new Bytes().u32(2).u32(5).u32(6).b;
  eq(__reflectCheck([{ type: "W", codec, bytes, json: JSON.stringify({ value: [5, 6] }) }]), []);
  const bad = __reflectCheck([{ type: "W", codec, bytes, json: JSON.stringify({ value: [5, 7] }) }]);
  eq(bad.map((x) => x.type), ["W"]);
});
