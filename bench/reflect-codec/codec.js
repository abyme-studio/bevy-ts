// JS-side cost per row of the ways a TS system could read and write a reflect-tier value
// (research-script-perf). Run: `node bench/reflect-codec/codec.js` or `deno run bench/reflect-codec/codec.js`.
// The host call itself (32-41 ns, docs/research/custom-relationships.md) and the Rust side are not
// in these numbers.

const N = 1000;
const RUNS = 2000;

// Two shapes: Bevy's Color (an enum with one 4-f32 struct per variant) and a Projection-like
// enum with a bigger payload.
const colors = Array.from({ length: N }, (_, i) => ({ Srgba: { red: i / N, green: 0.5, blue: 0.25, alpha: 1 } }));
const projs = Array.from({ length: N }, (_, i) => ({ Perspective: { fov: 0.78 + i * 1e-4, aspect_ratio: 1.77, near: 0.1, far: 1000 } }));

// ---- JSON: the current plan (the string arrives from Rust, JS parses; a write stringifies) ----
const colorJson = colors.map((c) => JSON.stringify(c));
const projJson = projs.map((p) => JSON.stringify(p));

// ---- Binary: a per-type codec generated at load from the type's op program ----
// Row = u32 tag + payload of f32 lanes, fixed stride per type (max over variants).
const COLOR_VARIANTS = ["Srgba", "LinearRgba", "Hsla", "Hsva", "Hwba", "Laba", "Lcha", "Oklaba", "Oklcha", "Xyza"];
const COLOR_FIELDS = [
  ["red", "green", "blue", "alpha"], ["red", "green", "blue", "alpha"], ["hue", "saturation", "lightness", "alpha"],
  ["hue", "saturation", "value", "alpha"], ["hue", "whiteness", "blackness", "alpha"], ["lightness", "a", "b", "alpha"],
  ["lightness", "chroma", "hue", "alpha"], ["lightness", "a", "b", "alpha"], ["lightness", "chroma", "hue", "alpha"],
  ["x", "y", "z", "alpha"],
];
// What the generator would emit: one function per type, a switch on the tag, literal objects so
// V8 gives every variant one hidden class.
function genDecoder(variants, fields) {
  let src = "const t = u32[o]; switch (t) {\n";
  variants.forEach((v, k) => {
    const body = fields[k].map((f, j) => `${f}: f32[o + ${j + 1}]`).join(", ");
    src += `case ${k}: return { ${v}: { ${body} } };\n`;
  });
  src += "} throw new Error('bad tag ' + t);";
  return new Function("u32", "f32", "o", src);
}
function genEncoder(variants, fields) {
  let src = "";
  variants.forEach((v, k) => {
    const body = fields[k].map((f, j) => `f32[o + ${j + 1}] = x.${f};`).join(" ");
    src += `if ('${v}' in v) { const x = v.${v}; u32[o] = ${k}; ${body} return; }\n`;
  });
  src += "throw new Error('bad value');";
  return new Function("u32", "f32", "o", "v", src);
}
const decColor = genDecoder(COLOR_VARIANTS, COLOR_FIELDS);
const encColor = genEncoder(COLOR_VARIANTS, COLOR_FIELDS);
const PROJ_VARIANTS = ["Perspective", "Orthographic"];
const PROJ_FIELDS = [["fov", "aspect_ratio", "near", "far"], ["near", "far", "scale", "area_min_x", "area_min_y", "area_max_x", "area_max_y"]];
const decProj = genDecoder(PROJ_VARIANTS, PROJ_FIELDS);
const encProj = genEncoder(PROJ_VARIANTS, PROJ_FIELDS);

const STRIDE = 8; // u32 words per row
const buf = new ArrayBuffer(N * STRIDE * 4);
const u32 = new Uint32Array(buf);
const f32 = new Float32Array(buf);
const pbuf = new ArrayBuffer(N * STRIDE * 4);
const pu32 = new Uint32Array(pbuf);
const pf32 = new Float32Array(pbuf);
for (let i = 0; i < N; i++) {
  encColor(u32, f32, i * STRIDE, colors[i]);
  encProj(pu32, pf32, i * STRIDE, projs[i]);
}

// ---- Cache keyed by the row's changed tick (the tick column is already a zero-copy view) ----
const ticks = new Uint32Array(N).fill(5);
const cacheTick = new Uint32Array(N).fill(5);
const cacheVal = colors.slice();

// ---- Layout-tier baseline: a flyweight getter over a Float32Array column ----
const col = new Float32Array(N * 4);

let sink = 0;
const cases = {
  "json read Color": () => { for (let i = 0; i < N; i++) sink += JSON.parse(colorJson[i]).Srgba.red; },
  "json write Color": () => { for (let i = 0; i < N; i++) sink += JSON.stringify(colors[i]).length; },
  "json read Projection": () => { for (let i = 0; i < N; i++) sink += JSON.parse(projJson[i]).Perspective.fov; },
  "json write Projection": () => { for (let i = 0; i < N; i++) sink += JSON.stringify(projs[i]).length; },
  "binary read Color": () => { for (let i = 0; i < N; i++) sink += decColor(u32, f32, i * STRIDE).Srgba.red; },
  "binary write Color": () => { for (let i = 0; i < N; i++) encColor(u32, f32, i * STRIDE, colors[i]); },
  "binary read Projection": () => { for (let i = 0; i < N; i++) sink += decProj(pu32, pf32, i * STRIDE).Perspective.fov; },
  "binary write Projection": () => { for (let i = 0; i < N; i++) encProj(pu32, pf32, i * STRIDE, projs[i]); },
  "tick cache hit Color": () => {
    for (let i = 0; i < N; i++) sink += (ticks[i] === cacheTick[i] ? cacheVal[i] : decColor(u32, f32, i * STRIDE)).Srgba.red;
  },
  "layout column read 4 lanes": () => { for (let i = 0; i < N; i++) sink += col[i * 4] + col[i * 4 + 1] + col[i * 4 + 2] + col[i * 4 + 3]; },
};

const rows = [];
for (const [name, f] of Object.entries(cases)) {
  for (let w = 0; w < 200; w++) f();
  const samples = [];
  for (let r = 0; r < 15; r++) {
    const t0 = performance.now();
    for (let k = 0; k < RUNS / 15; k++) f();
    samples.push(((performance.now() - t0) * 1e6) / ((RUNS / 15) * N));
  }
  samples.sort((a, b) => a - b);
  rows.push([name, samples[7].toFixed(1)]);
}
const engine = typeof Deno !== "undefined" ? `deno ${Deno.version.deno}, V8 ${Deno.version.v8}` : `node ${process.version}, V8 ${process.versions.v8}`;
console.log(`${engine}, ${N} rows, median ns per row`);
for (const [n, v] of rows) console.log(`| ${n} | ${v} |`);
if (sink === 42) console.log(sink);
