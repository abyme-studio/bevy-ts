// @generated from crates/bevy-ts-material/ts/material.js by scripts/sync-runtime.mjs. Do not edit.
// `bevy-ts/material`: script materials. A material is a name, a params schema in WGSL types and
// `.wgsl` files imported like modules; Rust lays out the params, generates their declarations and
// draws it with one erased material. A pass is a fullscreen fragment over a `Camera2d`'s image,
// declared the same way. Served by the engine; evaluated once, so each declaration goes into the
// loading generation's `materials` or `passes` section.
import { asset, assets, sectionItems } from "bevy-ts";

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

/** Each kind's params, textures and instance params, by name, for `Kind.add`. */
const SCHEMAS = new Map();

/** The asset class of every script material instance, for `assets.add`. */
const ScriptMaterial = asset("bevy_ts_material::render::ScriptMaterial", { of: false });

/** A material kind: what `<material kind={...} />` names. Only `name` crosses into a scene. */
class MaterialKind {
  constructor(name) {
    this.name = name;
    Object.freeze(this);
  }

  /**
   * An owned instance: one asset outside the shared cache, written in place with
   * `assets.set(ref, patch)` and given as `<material asset={ref} />`. Its schema goes with it, so
   * an instance added while the script loads is encoded before the generation's kinds apply.
   */
  add(props = {}) {
    if (props && typeof props === "object" && ("kind" in props || "$schema" in props)) throw new Error(`${this.name}.add(props): the kind is ${this.name}; leave kind out`);
    return assets.add(ScriptMaterial, { ...props, kind: this.name, $schema: SCHEMAS.get(this.name) });
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
    if (!["params", "textures", "instance", "fragment", "vertex", "alphaMode"].includes(k)) fail(`unknown option ${k}`);
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
  // Per-entity params: on `<material kind>` they go to the entity, not the shared asset. Rust lays
  // them out and refuses more than 256 bytes.
  const instance = [];
  for (const [p, type] of Object.entries(decl.instance || {})) {
    const ok = BASES.has(type) || (type && typeof type === "object" && BASES.has(type.array) && Number.isInteger(type.len));
    if (!ok) fail(`instance param ${p} has type ${JSON.stringify(type)}; use f32, i32, u32, vec2f, vec3f, vec4f, mat4x4f, color or array(...)`);
    if (params.some((q) => q.name === p) || textures.includes(p)) fail(`${p} is both an instance param and a param or texture`);
    instance.push({ name: p, type });
  }
  let alphaMode = decl.alphaMode === undefined ? "opaque" : decl.alphaMode;
  if (typeof alphaMode === "object" && alphaMode !== null && typeof alphaMode.mask === "number") alphaMode = { mask: alphaMode.mask };
  else if (!ALPHA.has(alphaMode)) fail(`alphaMode ${JSON.stringify(alphaMode)}; use ${[...ALPHA].join(", ")} or { mask: 0.5 }`);
  const items = sectionItems("materials");
  if (items.some((m) => m.name === name)) fail("is declared twice");
  items.push({ name, params, textures, instance, fragment, vertex: shader("vertex"), alphaMode, location: at || null });
  SCHEMAS.set(name, { params, textures, instance });
  return new MaterialKind(name);
}

/** A pass kind: what `<pass kind={...} />` names. Only `name` crosses into a scene. */
class PassKind {
  constructor(name) {
    this.name = name;
    Object.freeze(this);
  }
}

/**
 * `pass("game::Dusk", { params: { amount: f32, tint: color }, fragment: dusk, order: 0 })`, where
 * `dusk` is `import dusk from "./dusk.wgsl"`. Rust lays the params out after the frame fields and
 * refuses reserved names, more than 256 bytes and a fifth kind, at this call.
 */
export function pass(name, decl) {
  const at = callerLocation();
  const fail = (m) => {
    throw located(`pass ${name}: ${m}`, at);
  };
  if (typeof name !== "string" || !name) throw located("pass(name, ...): the name is a string (\"game::Dusk\")", at);
  if (!decl || typeof decl !== "object") fail("takes { params, fragment }");
  for (const k of Object.keys(decl)) {
    if (!["params", "fragment", "order"].includes(k)) fail(`unknown option ${k}`);
  }
  if (typeof decl.fragment === "string") fail("fragment is a WGSL file, import it: import shader from \"./fragment.wgsl\"; then fragment: shader");
  if (!isShaderFile(decl.fragment)) fail("needs a fragment shader, a shader file from a .wgsl import");
  const order = decl.order === undefined ? 0 : decl.order;
  if (typeof order !== "number" || !Number.isFinite(order)) fail(`order ${JSON.stringify(decl.order)} is a number`);
  const params = [];
  for (const [p, type] of Object.entries(decl.params || {})) {
    const ok = BASES.has(type) || (type && typeof type === "object" && BASES.has(type.array) && Number.isInteger(type.len));
    if (!ok) fail(`param ${p} has type ${JSON.stringify(type)}; use f32, i32, u32, vec2f, vec3f, vec4f, mat4x4f, color or array(...)`);
    params.push({ name: p, type });
  }
  const items = sectionItems("passes");
  if (items.some((m) => m.name === name)) fail("is declared twice");
  items.push({ name, params, fragment: decl.fragment.path, order, location: at || null });
  return new PassKind(name);
}
