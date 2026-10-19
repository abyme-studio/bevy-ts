// The `bevy-ts` module for the V8 backend.
//
// Declarations collect into the generation being loaded; Rust resolves ids and layouts and
// hands them back on commit, when accessor classes are generated with constant offsets. Each
// system run is one call from Rust (`runSystem`) with column ArrayBuffers that Rust detaches
// when the call returns. Field setters store the run's tick in the row's changed tick.

import { describe } from "bevy-ts/jsx-runtime";

const S = {
  /** The generation being loaded, or null. */
  loading: null,
  /** Loaded generations by number. */
  gens: new Map(),
  /** The live generation's number. */
  live: 0,
  /** Rust's resolution of the live generation. */
  resolution: null,
};

// ---------------------------------------------------------------------------------------------
// Source locations

const BUILTIN = /bevy-ts:/;
const FRAME = /\(?((?:file|bevy-ts):[^\s()]+?):(\d+):(\d+)\)?\s*$/;

function callerLocation() {
  const lines = String(new Error().stack).split("\n").slice(1);
  for (const line of lines) {
    if (BUILTIN.test(line)) continue;
    const m = FRAME.exec(line);
    if (m) return { file: m[1].replace(/\?gen=\d+$/, ""), line: +m[2], column: +m[3] };
  }
  return undefined;
}

function declError(message, location) {
  const e = new Error(message);
  e.bevyTsLocation = location;
  return e;
}

// ---------------------------------------------------------------------------------------------
// Field types

const SCALARS = ["f32", "f64", "i8", "u8", "i16", "u16", "i32", "u32", "bool", "vec2", "vec3", "vec4", "quat", "entity"];

class FieldType {
  constructor(spec, def, hints) {
    this.spec = spec;
    this.def = def;
    this.hintSet = hints || {};
  }
  /** A default value for new components and new fields in a migration. */
  default(value) {
    return new FieldType(this.spec, value, this.hintSet);
  }
  /** Editor hints: `min`, `max`, `step`, `label`. */
  hints(h) {
    return new FieldType(this.spec, this.def, { ...this.hintSet, ...h });
  }
  min(v) {
    return this.hints({ min: v });
  }
  max(v) {
    return this.hints({ max: v });
  }
  step(v) {
    return this.hints({ step: v });
  }
  label(v) {
    return this.hints({ label: v });
  }
}

export const f32 = new FieldType({ kind: "f32" });
export const f64 = new FieldType({ kind: "f64" });
export const i8 = new FieldType({ kind: "i8" });
export const u8 = new FieldType({ kind: "u8" });
export const i16 = new FieldType({ kind: "i16" });
export const u16 = new FieldType({ kind: "u16" });
export const i32 = new FieldType({ kind: "i32" });
export const u32 = new FieldType({ kind: "u32" });
export const bool = new FieldType({ kind: "bool" });
export const vec2 = new FieldType({ kind: "vec2" });
export const vec3 = new FieldType({ kind: "vec3" });
export const vec4 = new FieldType({ kind: "vec4" });
export const quat = new FieldType({ kind: "quat" });
export const entity = new FieldType({ kind: "entity" });
/** Rejected at declaration: components hold no heap data. */
export const string = new FieldType({ kind: "string" });

export function enumOf(...variants) {
  return new FieldType({ kind: "enum", variants });
}

export function array(of, len) {
  if (!(of instanceof FieldType)) throw new Error("array(type, n): type must be a field type");
  return new FieldType({ kind: "array", of: of.spec, len });
}

const LANES = { vec2: 2, vec3: 3, vec4: 4, quat: 4 };

function checkType(name, field, t, at) {
  if (!(t instanceof FieldType)) {
    throw declError(`component ${name} field ${field}: not a field type (use f32, vec3, enumOf(...), ...)`, at);
  }
  const k = t.spec.kind;
  if (k === "string") {
    throw declError(`component ${name} field ${field}: strings are not supported in components; components hold fixed-size data only`, at);
  }
  if (k === "array" && t.spec.of.kind === "string") {
    throw declError(`component ${name} field ${field}: strings are not supported in components`, at);
  }
}

// ---------------------------------------------------------------------------------------------
// Components

const VALUE = Symbol("bevy-ts.value");

function makeHandle(name, source, fields, native) {
  // A handle is callable: `Mover({ speed: 2 })` is a component value for spawn/insert.
  const handle = function (values) {
    return { [VALUE]: true, type: handle, values: values || {} };
  };
  Object.defineProperty(handle, "name", { value: name });
  handle.componentName = name;
  handle.source = source;
  handle.fields = fields; // { field: FieldType } or null for Rust components without a layout
  handle.native = native;
  handle.id = -1;
  handle.layout = null;
  handle.Read = null;
  handle.Write = null;
  handle.toString = () => name;
  return handle;
}

/**
 * Declares a component with native fields. The layout is computed by Rust on load.
 * @param {string} name a stable path such as `game::Mover`
 */
export function component(name, fields, options) {
  const at = callerLocation();
  const g = S.loading;
  if (!g) throw declError(`component(${name}) must be called while a module loads`, at);
  if (typeof name !== "string" || !name) throw declError("component(name, fields): name must be a non-empty string", at);
  if (!fields || typeof fields !== "object") throw declError(`component ${name}: fields must be an object`, at);
  const decl = [];
  for (const [field, t] of Object.entries(fields)) {
    checkType(name, field, t, at);
    const f = { name: field, type: t.spec, hints: t.hintSet };
    if (t.def !== undefined) f.default = t.def;
    decl.push(f);
  }
  const handle = makeHandle(name, "ts", fields, false);
  handle.options = options || {};
  g.components.push({ name, fields: decl, location: at });
  g.handles.push(handle);
  return handle;
}

/** A Rust component with a native layout (from `bevy-ts/bevy`). */
export function nativeComponent(name, fields) {
  const h = makeHandle(name, "rust", fields, true);
  NATIVE.push(h);
  return h;
}

/** A Rust component by type path or short name: usable in `with`/`without` only. */
export function rustComponent(name) {
  return makeHandle(name, "rust", null, false);
}

const NATIVE = [];

function ref(h, what, at) {
  if (typeof h !== "function" || !h.componentName) {
    throw declError(`${what}: ${String(h)} is not a component`, at);
  }
  return { name: h.componentName, source: h.source };
}

// ---------------------------------------------------------------------------------------------
// Systems, startups, entries, sections

const STAGES = ["preUpdate", "update", "postUpdate", "fixedUpdate"];

export const app = {
  /** Declares a system: `{ stage, read, write, with, without, after, run }`. */
  system(name, spec) {
    const at = callerLocation();
    const g = S.loading;
    if (!g) throw declError(`app.system(${name}) must be called while a module loads`, at);
    if (!spec || typeof spec.run !== "function") throw declError(`system ${name}: run must be a function`, at);
    const stage = spec.stage || "update";
    if (!STAGES.includes(stage)) throw declError(`system ${name}: unknown stage ${stage}; use ${STAGES.join(", ")}`, at);
    const read = (spec.read || []).map((h) => ref(h, `system ${name} read`, at));
    const write = (spec.write || []).map((h) => ref(h, `system ${name} write`, at));
    const decl = {
      name,
      stage,
      read,
      write,
      with: (spec.with || []).map((h) => ref(h, `system ${name} with`, at)),
      without: (spec.without || []).map((h) => ref(h, `system ${name} without`, at)),
      after: [].concat(spec.after || []),
      location: at,
    };
    g.systems.push(decl);
    g.systemFns.push({ name, run: spec.run, read: spec.read || [], write: spec.write || [] });
  },

  /** Declares a function run once per world, the first time a generation declaring it applies. */
  startup(name, run) {
    const at = callerLocation();
    const g = S.loading;
    if (!g) throw declError(`app.startup(${name}) must be called while a module loads`, at);
    if (typeof run !== "function") throw declError(`startup ${name}: run must be a function`, at);
    g.startups.push({ name, location: at });
    g.startupFns.push(run);
  },

  /**
   * Declares a scene root. Its function runs now, during the load, and its description goes to
   * the `tsx` reconciler with the generation; a throw fails the generation.
   */
  scene(name, fn) {
    const at = callerLocation();
    const g = S.loading;
    if (!g) throw declError(`app.scene(${name}) must be called while a module loads`, at);
    if (typeof fn !== "function") throw declError(`scene ${name}: must be a function returning elements`, at);
    if (g.scenes.has(name)) throw declError(`scene ${name} is declared twice`, at);
    const root = fn();
    let nodes = [];
    try {
      nodes = describe(root);
    } catch (e) {
      // A description error names its element: it reaches Rust in the section, which fails the
      // generation with that location (a throw here would be located at this call instead).
      if (!e || !e.bevyTsLocation) throw e;
      g.sceneErrors.push({ message: String(e.message), location: e.bevyTsLocation });
    }
    g.scenes.set(name, { name, location: at, nodes });
    section("tsx", { roots: [...g.scenes.values()], errors: g.sceneErrors });
  },

  /** Declares an entry Rust plugins call by name with a byte payload. */
  entry(name, fn) {
    const at = callerLocation();
    const g = S.loading;
    if (!g) throw declError(`app.entry(${name}) must be called while a module loads`, at);
    if (typeof fn !== "function") throw declError(`entry ${name}: must be a function`, at);
    g.entries.push({ name, location: at });
    g.entryFns.set(name, fn);
  },
};

const core = globalThis.Deno.core;
const encoder = { encode: (s) => core.encode(s) };
const decoder = { decode: (b) => core.decode(b) };

// console.* goes to Rust's script log with the caller's TS location.
function format(args) {
  return args
    .map((a) => {
      if (typeof a === "string") return a;
      if (a instanceof Error) return a.stack || String(a);
      try {
        return JSON.stringify(a);
      } catch {
        return String(a);
      }
    })
    .join(" ");
}

function logger(level) {
  return (...args) => {
    const at = callerLocation();
    core.ops.op_bevyts_log(level, format(args), at ? `${at.file}:${at.line}:${at.column}` : "");
  };
}

const bevyConsole = {
  debug: logger(0),
  log: logger(1),
  info: logger(1),
  warn: logger(2),
  error: logger(3),
  trace: logger(0),
};
globalThis.console = bevyConsole;

/** Writes a manifest section for a Rust plugin: bytes, a string, or a JSON-serializable value. */
export function section(name, value) {
  const at = callerLocation();
  const g = S.loading;
  if (!g) throw declError(`section(${name}) must be called while a module loads`, at);
  let bytes;
  if (value instanceof Uint8Array) bytes = value;
  else if (typeof value === "string") bytes = encoder.encode(value);
  else bytes = encoder.encode(JSON.stringify(value));
  g.sections.set(name, { bytes, location: at });
}

// ---------------------------------------------------------------------------------------------
// Accessors: generated per component from Rust's layout

const WIDTH = { f32: 4, f64: 8, i8: 1, u8: 1, i16: 2, u16: 2, i32: 4, u32: 4, bool: 1, entity: 4 };
const VIEW = { f32: "f32", f64: "f64", i8: "i8", u8: "u8", i16: "i16", u16: "u16", i32: "i32", u32: "u32", bool: "u8", entity: "u32" };
const VIEW_CTORS = {
  f32: Float32Array,
  f64: Float64Array,
  i8: Int8Array,
  u8: Uint8Array,
  i16: Int16Array,
  u16: Uint16Array,
  i32: Int32Array,
  u32: Uint32Array,
};

function enumView(t) {
  const n = t.variants.length;
  return n <= 256 ? "u8" : n <= 65536 ? "u16" : "u32";
}

function scalarKind(t) {
  if (t.kind === "enum") return enumView(t);
  return t.kind;
}

/** Views a component needs, by name. */
function viewsFor(layout) {
  const need = new Set();
  const add = (t) => {
    if (LANES[t.kind]) need.add("f32");
    else if (t.kind === "enum") need.add(enumView(t));
    else if (t.kind === "array") add(t.of);
    else need.add(VIEW[t.kind]);
  };
  for (const f of layout.fields) add(f.type);
  return [...need];
}

function setterTail(write) {
  return write ? "p.t[p.r] = p.k;" : "throw new TypeError(READONLY);";
}

/** Source for reading/writing one scalar at byte offset `off` (relative to the row). */
function scalarCode(kind, off, size, enumVariants) {
  const w = WIDTH[kind] || (kind === "u16" ? 2 : 4);
  const view = kind === "bool" ? "u8" : kind === "entity" ? "u32" : kind;
  const idx = `p.r * ${size / w} + ${off / w}`;
  if (kind === "bool") return { get: `p.u8[${idx}] !== 0`, set: `p.u8[${idx}] = v ? 1 : 0;` };
  if (kind === "entity") {
    return {
      get: `p.u32[${idx}] + p.u32[${idx} + 1] * 4294967296`,
      set: `{ const i = ${idx}; p.u32[i] = v % 4294967296; p.u32[i + 1] = Math.floor(v / 4294967296); }`,
    };
  }
  if (enumVariants) {
    const names = JSON.stringify(enumVariants);
    return {
      get: `${names}[p.${view}[${idx}]]`,
      set: `{ const i = typeof v === "string" ? ${names}.indexOf(v) : v; if (i < 0 || i >= ${enumVariants.length}) throw new RangeError("unknown variant " + v); p.${view}[${idx}] = i; }`,
    };
  }
  return { get: `p.${view}[${idx}]`, set: `p.${view}[${idx}] = v;` };
}

const READONLY = "this component is declared in `read`; declare it in `write` to change it";

/** Generates the accessor class for a layout. `write` adds tick-storing setters. */
function generateAccessor(name, layout, write) {
  const size = layout.size;
  const parts = [];
  const subclasses = [];
  const init = [];
  for (const f of layout.fields) {
    const t = f.type;
    const key = JSON.stringify(f.name);
    if (LANES[t.kind]) {
      const lanes = LANES[t.kind];
      const names = ["x", "y", "z", "w"].slice(0, lanes);
      const cls = [`class V { constructor(p) { this.p = p; }`];
      names.forEach((n, i) => {
        const c = scalarCode("f32", f.offset + i * 4, size);
        cls.push(`get ${n}() { const p = this.p; return ${c.get}; }`);
        cls.push(`set ${n}(v) { const p = this.p; ${c.set} ${setterTail(write)} }`);
      });
      cls.push(`toArray() { const p = this.p; return [${names.map((_, i) => scalarCode("f32", f.offset + i * 4, size).get).join(", ")}]; }`);
      cls.push(`set(${names.join(", ")}) { const p = this.p; ${names.map((n, i) => scalarCode("f32", f.offset + i * 4, size).set.replace(/\bv\b/g, n)).join(" ")} ${setterTail(write)} return this; }`);
      cls.push(`toJSON() { return this.toArray(); }`);
      cls.push(`}`);
      cls.push(`return V;`);
      subclasses.push(`const V_${subclasses.length} = (function () { ${cls.join("\n")} })();`);
      const vi = subclasses.length - 1;
      init.push(`this[${JSON.stringify("_" + f.name)}] = new V_${vi}(this);`);
      parts.push(`get ${key}() { return this[${JSON.stringify("_" + f.name)}]; }`);
      parts.push(
        `set ${key}(v) { const s = this[${JSON.stringify("_" + f.name)}]; if (Array.isArray(v)) s.set(...v); else s.set(${names.map((n) => "v." + n).join(", ")}); }`,
      );
    } else if (t.kind === "array") {
      const el = t.of;
      const w = el.kind === "enum" ? WIDTH[enumView(el)] : LANES[el.kind] ? LANES[el.kind] * 4 : WIDTH[el.kind];
      const cls = [`class A { constructor(p) { this.p = p; } get length() { return ${t.len}; }`];
      const getters = [];
      for (let i = 0; i < t.len; i++) {
        const off = f.offset + i * w;
        if (LANES[el.kind]) {
          const lanes = LANES[el.kind];
          const g = [...Array(lanes).keys()].map((j) => scalarCode("f32", off + j * 4, size).get);
          const s = [...Array(lanes).keys()].map((j) => scalarCode("f32", off + j * 4, size).set.replace(/\bv\b/g, `v[${j}]`));
          getters.push({ get: `[${g.join(", ")}]`, set: s.join(" ") });
        } else {
          getters.push(scalarCode(scalarKind(el), off, size, el.kind === "enum" ? el.variants : undefined));
        }
      }
      getters.forEach((c, i) => {
        cls.push(`get ${i}() { const p = this.p; return ${c.get}; }`);
        cls.push(`set ${i}(v) { const p = this.p; ${c.set} ${setterTail(write)} }`);
      });
      cls.push(`get(i) { return this[i]; } set(i, v) { if (i < 0 || i >= ${t.len}) throw new RangeError("index " + i); this[i] = v; }`);
      cls.push(`toArray() { return Array.from({ length: ${t.len} }, (_, i) => this[i]); } toJSON() { return this.toArray(); }`);
      cls.push(`*[Symbol.iterator]() { for (let i = 0; i < ${t.len}; i++) yield this[i]; }`);
      cls.push(`}`, `return A;`);
      subclasses.push(`const V_${subclasses.length} = (function () { ${cls.join("\n")} })();`);
      const vi = subclasses.length - 1;
      init.push(`this[${JSON.stringify("_" + f.name)}] = new V_${vi}(this);`);
      parts.push(`get ${key}() { return this[${JSON.stringify("_" + f.name)}]; }`);
      parts.push(`set ${key}(v) { const a = this[${JSON.stringify("_" + f.name)}]; for (let i = 0; i < ${t.len}; i++) a[i] = v[i]; }`);
    } else {
      const c = scalarCode(scalarKind(t), f.offset, size, t.kind === "enum" ? t.variants : undefined);
      parts.push(`get ${key}() { const p = this; return ${c.get}; }`);
      parts.push(`set ${key}(v) { const p = this; ${c.set} ${setterTail(write)} }`);
    }
  }
  const views = viewsFor(layout);
  const src = `
    ${subclasses.join("\n")}
    return class ${write ? "Write" : "Read"} {
      constructor() {
        this.r = 0; this.k = 0; this.t = null;
        ${views.map((v) => `this.${v} = null;`).join(" ")}
        ${init.join("\n")}
      }
      toJSON() { const o = {}; ${layout.fields.map((f) => `o[${JSON.stringify(f.name)}] = this[${JSON.stringify(f.name)}];`).join(" ")} return o; }
      ${parts.join("\n")}
    };`;
  try {
    return new Function("READONLY", src)(READONLY);
  } catch (e) {
    throw new Error(`bevy-ts: generating accessors for ${name}: ${e.message}`);
  }
}

function resolveHandle(h, res) {
  const r = res.components[h.componentName];
  if (!r) return;
  if (h.layout && h.id === r.id && JSON.stringify(h.layout) === JSON.stringify(r.layout)) return;
  h.id = r.id;
  h.layout = r.layout;
  h.views = viewsFor(r.layout);
  h.offsets = Object.fromEntries(r.layout.fields.map((f) => [f.name, f.offset]));
  h.size = r.layout.size;
  h.Read = generateAccessor(h.componentName, r.layout, false);
  h.Write = generateAccessor(h.componentName, r.layout, true);
}

// ---------------------------------------------------------------------------------------------
// Queries

function makeViews(buffer, names) {
  const v = { buffer };
  for (const n of names) v[n] = new VIEW_CTORS[n](buffer);
  return v;
}

class Table {
  constructor(q, i) {
    this.q = q;
    this.i = i;
  }
  get len() {
    return this.q.raw[this.i][0];
  }
  /** (index, generation) pairs as `u32`s; use `entity(row)` for an id. */
  get entities() {
    return this.q.raw[this.i][1];
  }
  entity(row) {
    const e = this.q.raw[this.i][1];
    return e[row * 2] + e[row * 2 + 1] * 4294967296;
  }
  col(T) {
    const c = this.q.index.get(T);
    if (c === undefined) throw new Error(`${T} is not in this system's read or write`);
    return c;
  }
  arrays(T, c) {
    const buf = this.q.raw[this.i][2][c];
    const out = makeViews(buf, T.views);
    out.offsets = T.offsets;
    out.size = T.size;
    out.stride = (w) => T.size / w;
    return out;
  }
  /** A component's typed arrays and byte offsets, for reading. */
  read(T) {
    return this.arrays(T, this.col(T));
  }
  /** A `write` component's arrays; every row of this table is marked changed after the run. */
  write(T) {
    const c = this.col(T);
    if (!this.q.writes[c]) throw new Error(`${T} is declared in read; declare it in write`);
    this.q.marks.push(this.i, c);
    return this.arrays(T, c);
  }
  /** A `write` component's arrays plus its `ticks`; mark rows yourself with `ticks[row] = ctx.tick`. */
  split(T) {
    const c = this.col(T);
    if (!this.q.writes[c]) throw new Error(`${T} is declared in read; declare it in write`);
    const out = this.arrays(T, c);
    out.ticks = new Uint32Array(this.q.raw[this.i][3][c]);
    return out;
  }
}

class Query {
  constructor(handles, writes) {
    this.handles = handles;
    this.writes = writes;
    this.index = new Map(handles.map((h, i) => [h, i]));
    this.raw = null;
    this.tick = 0;
    this.marks = [];
    this.accs = null;
  }
  bind(raw, tick) {
    this.raw = raw;
    this.tick = tick;
    this.marks = [];
    this._tables = null;
    if (!this.accs || this.accs.some((a, i) => !(a instanceof (this.writes[i] ? this.handles[i].Write : this.handles[i].Read)))) {
      this.accs = this.handles.map((h, i) => new (this.writes[i] ? h.Write : h.Read)());
    }
  }
  /** The matched tables, for hand-written loops over the typed arrays. */
  get tables() {
    if (!this._tables) this._tables = this.raw.map((_, i) => new Table(this, i));
    return this._tables;
  }
  /** Number of matched entities. */
  get count() {
    let n = 0;
    for (const t of this.raw) n += t[0];
    return n;
  }
  /** Calls `fn(entity, ...rows)` per matched entity; rows are reused accessors. */
  each(fn) {
    const accs = this.accs;
    const n = accs.length;
    const handles = this.handles;
    for (const t of this.raw) {
      const len = t[0];
      const ents = t[1];
      for (let c = 0; c < n; c++) {
        const a = accs[c];
        const buf = t[2][c];
        for (const v of handles[c].views) a[v] = new VIEW_CTORS[v](buf);
        a.t = t[3][c] ? new Uint32Array(t[3][c]) : null;
        a.k = this.tick;
      }
      if (n === 1) {
        const a0 = accs[0];
        for (let r = 0; r < len; r++) {
          a0.r = r;
          fn(ents[r * 2] + ents[r * 2 + 1] * 4294967296, a0);
        }
      } else if (n === 2) {
        const a0 = accs[0];
        const a1 = accs[1];
        for (let r = 0; r < len; r++) {
          a0.r = r;
          a1.r = r;
          fn(ents[r * 2] + ents[r * 2 + 1] * 4294967296, a0, a1);
        }
      } else {
        const args = new Array(n + 1);
        for (let r = 0; r < len; r++) {
          for (let c = 0; c < n; c++) {
            accs[c].r = r;
            args[c + 1] = accs[c];
          }
          args[0] = ents[r * 2] + ents[r * 2 + 1] * 4294967296;
          fn(...args);
        }
      }
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Commands: a binary arena in Rust memory, overflow kept in JS

const OP_SPAWN = 1, OP_INSERT = 2, OP_DESPAWN = 3, OP_REMOVE = 4, OP_SET_PARENT = 5, OP_SPAWN_NATIVE = 6, OP_PLUGIN = 7;

class Commands {
  constructor() {
    this.arena = null;
    this.view = null;
    this.at = 0;
    this.overflow = null;
    this.pool = null;
    this.poolUsed = 0;
    this.kinds = null;
  }
  begin(arena, pool, kinds) {
    this.arena = new Uint8Array(arena);
    this.view = new DataView(arena);
    this.at = 0;
    this.overflow = null;
    this.pool = new Float64Array(pool);
    this.poolUsed = 0;
    this.kinds = kinds;
  }
  /** Returns [arena bytes used, overflow bytes or null, pool ids used]. */
  end() {
    let over = null;
    if (this.overflow) {
      const total = this.overflow.reduce((n, c) => n + c.length, 0);
      over = new Uint8Array(total);
      let o = 0;
      for (const c of this.overflow) {
        over.set(c, o);
        o += c.length;
      }
    }
    return [this.overflow ? this.overflowAt : this.at, over, this.poolUsed];
  }
  // Ops are built in a scratch buffer, then appended whole, so an op never straddles.
  op(bytes) {
    if (!this.overflow && this.at + bytes.length <= this.arena.length) {
      this.arena.set(bytes, this.at);
      this.at += bytes.length;
    } else {
      if (!this.overflow) {
        this.overflow = [];
        this.overflowAt = this.at;
      }
      this.overflow.push(bytes.slice());
    }
  }
  alloc() {
    if (this.poolUsed >= this.pool.length) {
      throw new Error(`spawn pool exhausted: at most ${this.pool.length} spawns per call`);
    }
    return this.pool[this.poolUsed++];
  }
  /** Spawns an entity with components (handles for defaults, or `Type({...})` values). */
  spawn(...components) {
    const e = this.alloc();
    this.op(encodeComponentsOp(OP_SPAWN, e, components));
    return e;
  }
  insert(entity, ...components) {
    this.op(encodeComponentsOp(OP_INSERT, entity, components));
  }
  despawn(entity) {
    const w = new Writer();
    w.u8(OP_DESPAWN);
    w.entity(entity);
    this.op(w.bytes());
  }
  remove(entity, ...types) {
    const w = new Writer();
    w.u8(OP_REMOVE);
    w.entity(entity);
    w.u16(types.length);
    for (const T of types) w.u32(idOf(T));
    this.op(w.bytes());
  }
  setParent(child, parent) {
    const w = new Writer();
    w.u8(OP_SET_PARENT);
    w.entity(child);
    if (parent === null || parent === undefined) {
      w.u32(0xffffffff);
      w.u32(0xffffffff);
    } else w.entity(parent);
    this.op(w.bytes());
  }
  /** Spawns through a native Rust recipe (`camera3d`, `directionalLight`, `cuboid`). */
  spawnNative(recipe, params) {
    const e = this.alloc();
    const w = new Writer();
    w.u8(OP_SPAWN_NATIVE);
    w.entity(e);
    const name = encoder.encode(recipe);
    w.u16(name.length);
    w.raw(name);
    const p = encoder.encode(JSON.stringify(params === undefined ? null : params));
    w.u32(p.length);
    w.raw(p);
    this.op(w.bytes());
    return e;
  }
  /** Mounts an element tree under the tsx runtime root (`{ parent }`: an entity to nest under). */
  mount(element, options) {
    const parent = options && options.parent !== undefined && options.parent !== null ? options.parent : null;
    this.custom("tsx/mount", { nodes: describe(element), parent });
  }
  /** Unmounts a runtime-mounted node by its key, or any tsx node by its entity. */
  unmount(target) {
    this.custom("tsx/unmount", typeof target === "string" ? { key: target } : { entity: target });
  }
  /** Appends an op of a plugin's command kind. */
  custom(kind, payload) {
    const opcode = this.kinds ? this.kinds[kind] : undefined;
    if (opcode === undefined) throw new Error(`unregistered command kind ${kind}`);
    const bytes = payload instanceof Uint8Array ? payload : encoder.encode(typeof payload === "string" ? payload : JSON.stringify(payload));
    const w = new Writer();
    w.u8(OP_PLUGIN);
    w.u16(opcode);
    w.u32(bytes.length);
    w.raw(bytes);
    this.op(w.bytes());
  }
}

class Writer {
  constructor() {
    this.buf = new Uint8Array(64);
    this.dv = new DataView(this.buf.buffer);
    this.n = 0;
  }
  room(k) {
    if (this.n + k <= this.buf.length) return;
    const b = new Uint8Array(Math.max(this.buf.length * 2, this.n + k));
    b.set(this.buf);
    this.buf = b;
    this.dv = new DataView(b.buffer);
  }
  u8(v) {
    this.room(1);
    this.dv.setUint8(this.n, v);
    this.n += 1;
  }
  u16(v) {
    this.room(2);
    this.dv.setUint16(this.n, v, true);
    this.n += 2;
  }
  u32(v) {
    this.room(4);
    this.dv.setUint32(this.n, v, true);
    this.n += 4;
  }
  entity(e) {
    if (typeof e !== "number" || !(e >= 0)) throw new TypeError(`not an entity: ${e}`);
    this.u32(e % 4294967296);
    this.u32(Math.floor(e / 4294967296));
  }
  raw(bytes) {
    this.room(bytes.length);
    this.buf.set(bytes, this.n);
    this.n += bytes.length;
  }
  bytes() {
    return this.buf.subarray(0, this.n);
  }
}

function idOf(T) {
  if (typeof T !== "function" || !T.componentName) throw new TypeError(`not a component: ${T}`);
  if (T.id < 0 || !T.layout) throw new Error(`${T.componentName} has no native layout to write from TS`);
  return T.id;
}

function encodeComponentsOp(op, e, components) {
  const w = new Writer();
  w.u8(op);
  w.entity(e);
  w.u16(components.length);
  for (const c of components) {
    const T = c && c[VALUE] ? c.type : c;
    const values = c && c[VALUE] ? c.values : {};
    const id = idOf(T);
    const bytes = encodeValue(T, values);
    w.u32(id);
    w.u32(bytes.length);
    w.raw(bytes);
  }
  return w.bytes();
}

function encodeScalar(dv, kind, off, v, t) {
  switch (kind) {
    case "f32":
      return dv.setFloat32(off, v, true);
    case "f64":
      return dv.setFloat64(off, v, true);
    case "i8":
      return dv.setInt8(off, v);
    case "u8":
      return dv.setUint8(off, v);
    case "i16":
      return dv.setInt16(off, v, true);
    case "u16":
      return dv.setUint16(off, v, true);
    case "i32":
      return dv.setInt32(off, v, true);
    case "u32":
      return dv.setUint32(off, v, true);
    case "bool":
      return dv.setUint8(off, v ? 1 : 0);
    case "entity":
      dv.setUint32(off, v % 4294967296, true);
      return dv.setUint32(off + 4, Math.floor(v / 4294967296), true);
    case "enum": {
      const i = typeof v === "string" ? t.variants.indexOf(v) : v;
      if (i < 0 || i >= t.variants.length) throw new RangeError(`unknown variant ${v}`);
      const w = enumView(t);
      if (w === "u8") return dv.setUint8(off, i);
      if (w === "u16") return dv.setUint16(off, i, true);
      return dv.setUint32(off, i, true);
    }
  }
}

function encodeField(dv, t, off, v, path) {
  if (LANES[t.kind]) {
    const lanes = LANES[t.kind];
    const arr = Array.isArray(v) ? v : v && typeof v.toArray === "function" ? v.toArray() : ["x", "y", "z", "w"].slice(0, lanes).map((n) => v[n]);
    if (arr.length !== lanes) throw new TypeError(`${path} expects ${t.kind}`);
    for (let i = 0; i < lanes; i++) dv.setFloat32(off + i * 4, arr[i], true);
  } else if (t.kind === "array") {
    const el = t.of;
    const w = el.kind === "enum" ? WIDTH[enumView(el)] : LANES[el.kind] ? LANES[el.kind] * 4 : WIDTH[el.kind];
    for (let i = 0; i < t.len; i++) encodeField(dv, el, off + i * w, v[i], `${path}[${i}]`);
  } else {
    if (t.kind !== "enum" && t.kind !== "bool" && typeof v !== "number") throw new TypeError(`${path} expects ${t.kind}, got ${typeof v}`);
    encodeScalar(dv, t.kind, off, v, t);
  }
}

/** Native bytes for a component value; omitted fields take their declared default. */
function encodeValue(T, values) {
  const layout = T.layout;
  const out = new Uint8Array(layout.size);
  const dv = new DataView(out.buffer);
  for (const f of layout.fields) {
    let v = values[f.name];
    if (v === undefined) {
      const ft = T.fields && T.fields[f.name];
      v = ft ? ft.def : undefined;
    }
    if (v === undefined) continue;
    encodeField(dv, f.type, f.offset, v, `${T.componentName}.${f.name}`);
  }
  for (const k of Object.keys(values)) {
    if (!layout.fields.some((f) => f.name === k)) throw new TypeError(`${T.componentName} has no field ${k}`);
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// The host interface Rust calls

function newGeneration() {
  return {
    components: [],
    handles: [],
    systems: [],
    systemFns: [],
    startups: [],
    startupFns: [],
    entries: [],
    entryFns: new Map(),
    sections: new Map(),
    scenes: new Map(),
    sceneErrors: [],
    built: null,
  };
}

function makeCtx() {
  return { dt: 0, elapsed: 0, tick: 0, frame: 0, commands: new Commands() };
}

function begin(gen) {
  S.loading = newGeneration();
  S.loading.number = gen;
}

function end() {
  const g = S.loading;
  S.loading = null;
  if (!g) throw new Error("bevy-ts: end without begin");
  S.gens.set(g.number, g);
  const manifest = {
    components: g.components,
    systems: g.systems,
    startups: g.startups,
    entries: g.entries,
  };
  const sections = [...g.sections].map(([name, s]) => [name, s.bytes, s.location || null]);
  return [JSON.stringify(manifest), sections];
}

function abort() {
  S.loading = null;
}

function commit(gen, resolutionJson) {
  const g = S.gens.get(gen);
  if (!g) throw new Error(`bevy-ts: generation ${gen} is not loaded`);
  const res = JSON.parse(resolutionJson);
  for (const h of g.handles) resolveHandle(h, res);
  for (const h of NATIVE) resolveHandle(h, res);
  g.built = g.systemFns.map((s) => {
    const handles = [...s.read, ...s.write];
    const writes = [...s.read.map(() => false), ...s.write.map(() => true)];
    return { run: s.run, query: new Query(handles, writes), ctx: makeCtx() };
  });
  g.startupCtx = makeCtx();
  g.kinds = res.commandKinds;
  const old = S.live;
  S.live = gen;
  S.resolution = res;
  if (old && old !== gen) S.gens.delete(old);
}

function discard(gen) {
  S.gens.delete(gen);
}

function fillCtx(ctx, dt, elapsed, tick, frame, arena, pool, kinds) {
  ctx.dt = dt;
  ctx.elapsed = elapsed;
  ctx.tick = tick;
  ctx.frame = frame;
  ctx.commands.begin(arena, pool, kinds);
}

function runSystem(gen, index, dt, elapsed, tick, frame, tables, arena, pool) {
  const g = S.gens.get(gen);
  const s = g.built[index];
  fillCtx(s.ctx, dt, elapsed, tick, frame, arena, pool, g.kinds);
  s.query.bind(tables, tick);
  s.run(s.query, s.ctx);
  const [len, over, used] = s.ctx.commands.end();
  return [len, over, used, s.query.marks];
}

function runStartup(gen, index, dt, elapsed, tick, frame, arena, pool) {
  const g = S.gens.get(gen);
  const ctx = g.startupCtx;
  fillCtx(ctx, dt, elapsed, tick, frame, arena, pool, g.kinds);
  g.startupFns[index](ctx);
  const [len, over, used] = ctx.commands.end();
  return [len, over, used, []];
}

function callEntry(name, payload, dt, elapsed, tick, frame, arena, pool) {
  const g = S.gens.get(S.live);
  const fn = g && g.entryFns.get(name);
  if (!fn) throw new Error(`no entry named ${name}`);
  const ctx = makeCtx();
  fillCtx(ctx, dt, elapsed, tick, frame, arena, pool, g.kinds);
  let out = fn(new Uint8Array(payload), ctx);
  if (out === undefined || out === null) out = new Uint8Array(0);
  else if (typeof out === "string") out = encoder.encode(out);
  else if (!(out instanceof Uint8Array)) out = encoder.encode(JSON.stringify(out));
  const [len, over, used] = ctx.commands.end();
  return [out.slice(), len, over, used];
}

globalThis.__bevyts = { begin, end, abort, commit, discard, runSystem, runStartup, callEntry };

/** Decodes UTF-8 bytes, for entries that take text or JSON payloads. */
export function text(bytes) {
  return decoder.decode(bytes);
}
