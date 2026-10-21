// @generated from crates/bevy-ts-material/ts/material.js by scripts/sync-runtime.mjs. Do not edit.
// `bevy-ts/material`: script materials. A material is a name, a params schema in WGSL types and
// `.wgsl` files imported like modules; Rust lays out the params, generates their declarations and
// draws it with one erased material. Served by the engine; evaluated once, so each declaration
// goes into the loading generation's `materials` section.
import { sectionItems } from "bevy-ts";

export const f32 = "f32";
export const i32 = "i32";
export const u32 = "u32";
export const vec2f = "vec2f";
export const vec3f = "vec3f";
export const vec4f = "vec4f";
export const mat4x4f = "mat4x4f";
/** A `vec4<f32>` in linear RGBA, written as `"#f80"` or sRGB `[r, g, b, a?]`. */
export const color = "color";
/** A 2D texture with a filtering sampler, `<name>` and `<name>_sampler` in WGSL. */
export const texture2d = "texture2d";

const BASES = new Set([f32, i32, u32, vec2f, vec3f, vec4f, mat4x4f, color]);
const ALPHA = new Set(["opaque", "blend", "premultiplied", "add", "multiply", "alphaToCoverage"]);

/** A fixed array, strided to 16 bytes; scalar and `vec2f` elements read as `.x`/`.xy` of a `vec4`. */
export function array(of, len) {
  if (!BASES.has(of)) throw new Error(`array(${JSON.stringify(of)}, ${len}): elements are f32, i32, u32, vec2f, vec3f, vec4f, mat4x4f or color`);
  if (!Number.isInteger(len) || len < 1) throw new Error(`array(${of}, ${len}): the length is a positive integer`);
  return Object.freeze({ array: of, len });
}

const FRAME = /\(?((?:file|bevy-ts):[^\s()]+?):(\d+):(\d+)\)?\s*$/;

function callerLocation() {
  for (const line of String(new Error().stack).split("\n").slice(1)) {
    if (/bevy-ts:/.test(line)) continue;
    const m = FRAME.exec(line);
    if (m) return { file: m[1].replace(/\?gen=\d+$/, ""), line: +m[2], column: +m[3] };
  }
  return undefined;
}

function located(message, location) {
  const e = new Error(message);
  e.bevyTsLocation = location;
  return e;
}

function isShaderFile(v) {
  return v !== null && typeof v === "object" && v.kind === "shaderFile" && typeof v.path === "string";
}

/** A material kind: what `<material kind={...} />` names. Only `name` crosses into a scene. */
class MaterialKind {
  constructor(name) {
    this.name = name;
    Object.freeze(this);
  }
}

/**
 * `material("game::Glow", { params: { color: color, pulse: f32 }, fragment: glow })`, where
 * `glow` is `import glow from "./glow.wgsl"`.
 */
export function material(name, decl) {
  const at = callerLocation();
  const fail = (m) => {
    throw located(`material ${name}: ${m}`, at);
  };
  if (typeof name !== "string" || !name) throw located("material(name, ...): the name is a string (\"game::Glow\")", at);
  if (!decl || typeof decl !== "object") fail("takes { params, fragment }");
  for (const k of Object.keys(decl)) {
    if (!["params", "textures", "fragment", "vertex", "alphaMode"].includes(k)) fail(`unknown option ${k}`);
  }
  const shader = (k) => {
    const v = decl[k];
    if (v === undefined) return null;
    if (typeof v === "string") fail(`${k} is a WGSL file, import it: import shader from "./${k}.wgsl"; then ${k}: shader`);
    if (!isShaderFile(v)) fail(`${k} is a shader file from a .wgsl import`);
    return v.path;
  };
  const fragment = shader("fragment");
  if (fragment === null) fail("needs a fragment shader");
  const params = [];
  for (const [p, type] of Object.entries(decl.params || {})) {
    const ok = BASES.has(type) || (type && typeof type === "object" && BASES.has(type.array) && Number.isInteger(type.len));
    if (!ok) fail(`param ${p} has type ${JSON.stringify(type)}; use f32, i32, u32, vec2f, vec3f, vec4f, mat4x4f, color or array(...)`);
    params.push({ name: p, type });
  }
  const textures = [];
  for (const [t, type] of Object.entries(decl.textures || {})) {
    if (type !== texture2d) fail(`texture ${t} has type ${JSON.stringify(type)}; use texture2d`);
    if (params.some((p) => p.name === t)) fail(`${t} is both a param and a texture`);
    textures.push(t);
  }
  let alphaMode = decl.alphaMode === undefined ? "opaque" : decl.alphaMode;
  if (typeof alphaMode === "object" && alphaMode !== null && typeof alphaMode.mask === "number") alphaMode = { mask: alphaMode.mask };
  else if (!ALPHA.has(alphaMode)) fail(`alphaMode ${JSON.stringify(alphaMode)}; use ${[...ALPHA].join(", ")} or { mask: 0.5 }`);
  const items = sectionItems("materials");
  if (items.some((m) => m.name === name)) fail("is declared twice");
  items.push({ name, params, textures, fragment, vertex: shader("vertex"), alphaMode, location: at || null });
  return new MaterialKind(name);
}
