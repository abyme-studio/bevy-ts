// The `bevy-ts` module for the V8 backend.
//
// Systems are exported functions whose parameter types the compile step read; it appends
// `__bevyts.bind` / `__bevyts.declare` after each module, so every system function carries its
// declarations (openspec/changes/systems-api, design decision 1). During a load, classes
// (`Component`, `Resource`, `Message`, `Event`, `States`) and `app.*` registrations collect into
// the generation; at its end the registrations resolve into a manifest. Rust answers with ids and
// layouts on commit, when accessor classes are generated with constant offsets. Each system run
// is one call from Rust (`runSystem`) with one frame per param; Rust detaches every buffer when
// the call returns. Field setters store the run's tick in the row's changed tick.

import { NOT_TSX, STRUCT_FIELDS, TsxContext, describe, readResource, runScene, setHost } from "bevy-ts/jsx-runtime";

// TSX's reactive surface, from the JSX runtime: no hooks, bindings read `ref.get(C)` directly.
export { signal, computed, effect, untrack, onCleanup, batch, ref, Show, For, px, percent, vw, vh, em, rem, fr, minmax, repeat, span, deg, linearGradient, radialGradient, conicGradient, sliced, tiled } from "bevy-ts/jsx-runtime";

const S = {
  /** The generation being loaded, or null. */
  loading: null,
  /** Loaded generations by number. */
  gens: new Map(),
  /** The live generation's number. */
  live: 0,
  /** Rust's resolution of the live generation. */
  resolution: null,
  /** The params a running setup closure declares, or null outside one. */
  declaring: null,
  /** The context of the call into JS under way, for tsx flush requests. */
  calling: null,
};

// In the engine, deno_core's `Deno.core`; elsewhere (Node, vitest through `bevy-ts/testing`), the
// web's encoders and the host console.
const core = globalThis.Deno && globalThis.Deno.core && globalThis.Deno.core.ops && globalThis.Deno.core.ops.op_bevyts_log ? globalThis.Deno.core : null;

// The web globals' hooks (web.js, evaluated first): the ambient generation every timer and
// socket records, `process.env`, delivery, and dropping a generation's timers and sockets.
const web = globalThis.__bevytsWeb || null;
delete globalThis.__bevytsWeb;
const encoder = core ? { encode: (s) => core.encode(s) } : new TextEncoder();
const decoder = core ? { decode: (b) => core.decode(b) } : new TextDecoder();

// ---------------------------------------------------------------------------------------------
// Source locations and console

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

if (core) globalThis.console = { debug: logger(0), log: logger(1), info: logger(1), warn: logger(2), error: logger(3), trace: logger(0) };

// ---------------------------------------------------------------------------------------------
// Field types: each is a value (for schemas) and, in the types package, a TS type

const LANES = { vec2: 2, vec3: 3, vec4: 4, quat: 4 };

class FieldType {
  constructor(spec, def, hints) {
    this.spec = spec;
    this.def = def;
    this.hintSet = hints || {};
  }
  /** A default for new values and for new fields in a migration. */
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

/** A field type that is also a function (`vec3(1, 2, 3)` makes a scratch vector). */
function callableField(spec, make) {
  const f = (...args) => make(...args);
  const t = new FieldType(spec);
  f.spec = spec;
  f.def = undefined;
  f.hintSet = {};
  for (const m of ["default", "hints", "min", "max", "step", "label"]) f[m] = (...a) => t[m](...a);
  return f;
}

function isFieldType(t) {
  return !!t && typeof t === "object" ? t instanceof FieldType : typeof t === "function" && !!t.spec;
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
export const vec2 = callableField({ kind: "vec2" }, (x = 0, y = 0) => new Vec3(x, y, 0));
export const vec3 = callableField({ kind: "vec3" }, (x = 0, y = 0, z = 0) => new Vec3(x, y, z));
export const vec4 = new FieldType({ kind: "vec4" });
export const quat = callableField({ kind: "quat" }, (x = 0, y = 0, z = 0, w = 1) => new Quat(x, y, z, w));
export const entity = new FieldType({ kind: "entity" });
/** Rejected at declaration: components hold fixed-size data only. */
export const string = new FieldType({ kind: "string" });
/** Bevy's `Timer`: a field type, and `Timer.fromSeconds(2, "Repeating")` a value of it. */
export const Timer = new FieldType({ kind: "timer" });
/** `from_seconds` takes an `f32`, as Bevy's does. */
Timer.fromSeconds = (seconds, mode) => new TimerValue(Math.fround(seconds), mode);
Timer.new = (seconds, mode) => new TimerValue(seconds, mode);
/** Bevy's `Stopwatch`: a field type, and `Stopwatch.new()` a value of it. */
export const Stopwatch = new FieldType({ kind: "stopwatch" });
Stopwatch.new = () => new StopwatchValue();

// Colours: sugar for Bevy's reflected `Color` shape, which every colour prop takes.
export function srgb(r, g, b, a = 1) {
  return { Srgba: { red: r, green: g, blue: b, alpha: a } };
}
export function linear(r, g, b, a = 1) {
  return { LinearRgba: { red: r, green: g, blue: b, alpha: a } };
}
export function hsl(h, s, l, a = 1) {
  return { Hsla: { hue: h, saturation: s, lightness: l, alpha: a } };
}
export function hex(s) {
  if (typeof s !== "string") throw new Error(`hex(s): s is a string like "#ff8800", not ${s}`);
  return s;
}

export function enumOf(...variants) {
  return new FieldType({ kind: "enum", variants });
}

export function array(of, len) {
  if (!isFieldType(of)) throw new Error("array(type, n): type must be a field type");
  return new FieldType({ kind: "array", of: of.spec, len });
}

function checkType(name, field, t, at) {
  if (!isFieldType(t)) throw declError(`${name} field ${field}: not a field type (use f32, vec3, enumOf(...), ...)`, at);
  const k = t.spec.kind;
  if (k === "string" || (k === "array" && t.spec.of.kind === "string")) {
    throw declError(`${name} field ${field}: strings are not supported; components hold fixed-size data only`, at);
  }
}

function fieldDecls(name, fields, at) {
  if (!fields || typeof fields !== "object") throw declError(`${name}: fields must be an object`, at);
  return Object.entries(fields).map(([field, t]) => {
    checkType(name, field, t, at);
    const f = { name: field, type: t.spec, hints: t.hintSet };
    if (t.def !== undefined) f.default = t.def;
    return f;
  });
}

// ---------------------------------------------------------------------------------------------
// Scratch vectors: `vec3()` and `quat()`, with the same in-place methods as column views

const VEC_READ = {
  length() {
    return Math.hypot(this.x, this.y, this.z);
  },
  dot(v) {
    return this.x * v.x + this.y * v.y + this.z * v.z;
  },
  distance(v) {
    return Math.hypot(this.x - v.x, this.y - v.y, this.z - v.z);
  },
  toArray() {
    return [this.x, this.y, this.z];
  },
  toJSON() {
    return [this.x, this.y, this.z];
  },
};

const VEC_WRITE = {
  set(x, y, z) {
    this.x = x;
    this.y = y;
    this.z = z;
    return this;
  },
  copy(v) {
    return this.set(v.x, v.y, v.z);
  },
  add(v) {
    return this.set(this.x + v.x, this.y + v.y, this.z + v.z);
  },
  sub(v) {
    return this.set(this.x - v.x, this.y - v.y, this.z - v.z);
  },
  scale(s) {
    return this.set(this.x * s, this.y * s, this.z * s);
  },
  addScaled(v, s) {
    return this.set(this.x + v.x * s, this.y + v.y * s, this.z + v.z * s);
  },
  normalize() {
    const l = Math.hypot(this.x, this.y, this.z);
    return l > 0 ? this.scale(1 / l) : this;
  },
  lerp(v, t) {
    return this.set(this.x + (v.x - this.x) * t, this.y + (v.y - this.y) * t, this.z + (v.z - this.z) * t);
  },
  /** this = a - b */
  subVectors(a, b) {
    return this.set(a.x - b.x, a.y - b.y, a.z - b.z);
  },
  /** Rotates by a unit quaternion. */
  rotate(q) {
    const { x, y, z } = this;
    const tx = 2 * (q.y * z - q.z * y), ty = 2 * (q.z * x - q.x * z), tz = 2 * (q.x * y - q.y * x);
    return this.set(x + q.w * tx + (q.y * tz - q.z * ty), y + q.w * ty + (q.z * tx - q.x * tz), z + q.w * tz + (q.x * ty - q.y * tx));
  },
};

const QUAT_READ = {
  toArray() {
    return [this.x, this.y, this.z, this.w];
  },
  toJSON() {
    return [this.x, this.y, this.z, this.w];
  },
};

const QUAT_WRITE = {
  set(x, y, z, w) {
    this.x = x;
    this.y = y;
    this.z = z;
    this.w = w;
    return this;
  },
  copy(q) {
    return this.set(q.x, q.y, q.z, q.w);
  },
  /** Faces `dir` (Bevy's -Z forward) with `up`. */
  lookTo(dir, up) {
    // Basis: back = -dir, right = up × back, up' = back × right.
    let bx = -dir.x, by = -dir.y, bz = -dir.z;
    const bl = Math.hypot(bx, by, bz) || 1;
    bx /= bl, by /= bl, bz /= bl;
    let rx = up.y * bz - up.z * by, ry = up.z * bx - up.x * bz, rz = up.x * by - up.y * bx;
    const rl = Math.hypot(rx, ry, rz) || 1;
    rx /= rl, ry /= rl, rz /= rl;
    const ux = by * rz - bz * ry, uy = bz * rx - bx * rz, uz = bx * ry - by * rx;
    const m00 = rx, m11 = uy, m22 = bz, tr = m00 + m11 + m22;
    if (tr > 0) {
      const s = Math.sqrt(tr + 1) * 2;
      return this.set((uz - by) / s, (bx - rz) / s, (ry - ux) / s, 0.25 * s);
    } else if (m00 > m11 && m00 > m22) {
      const s = Math.sqrt(1 + m00 - m11 - m22) * 2;
      return this.set(0.25 * s, (ux + ry) / s, (bx + rz) / s, (uz - by) / s);
    } else if (m11 > m22) {
      const s = Math.sqrt(1 + m11 - m00 - m22) * 2;
      return this.set((ux + ry) / s, 0.25 * s, (by + uz) / s, (bx - rz) / s);
    }
    const s = Math.sqrt(1 + m22 - m00 - m11) * 2;
    return this.set((bx + rz) / s, (by + uz) / s, 0.25 * s, (ry - ux) / s);
  },
  /** Rotates about Y by `radians` (this = rotation(y) * this). */
  rotateY(radians) {
    const h = radians / 2, s = Math.sin(h), c = Math.cos(h);
    const { x, y, z, w } = this;
    return this.set(c * x + s * z, c * y + s * w, c * z - s * x, c * w - s * y);
  },
  slerp(q, t) {
    let { x, y, z, w } = q;
    let d = this.x * x + this.y * y + this.z * z + this.w * w;
    if (d < 0) {
      d = -d;
      x = -x, y = -y, z = -z, w = -w;
    }
    if (d > 0.9995) {
      return this.set(this.x + (x - this.x) * t, this.y + (y - this.y) * t, this.z + (z - this.z) * t, this.w + (w - this.w) * t).normalize4();
    }
    const th = Math.acos(d), sn = Math.sin(th), a = Math.sin((1 - t) * th) / sn, b = Math.sin(t * th) / sn;
    return this.set(a * this.x + b * x, a * this.y + b * y, a * this.z + b * z, a * this.w + b * w);
  },
  normalize4() {
    const l = Math.hypot(this.x, this.y, this.z, this.w) || 1;
    return this.set(this.x / l, this.y / l, this.z / l, this.w / l);
  },
};

class Vec3 {
  constructor(x, y, z) {
    this.x = x;
    this.y = y;
    this.z = z;
  }
}
Object.assign(Vec3.prototype, VEC_READ, VEC_WRITE);

class Quat {
  constructor(x, y, z, w) {
    this.x = x;
    this.y = y;
    this.z = z;
    this.w = w;
  }
}
Object.assign(Quat.prototype, QUAT_READ, QUAT_WRITE);

// ---------------------------------------------------------------------------------------------
// Timer and Stopwatch: Bevy 0.20's `bevy_time::{Timer, Stopwatch}` ported line for line, every
// `Duration` in seconds. The getters and methods work on raw fields (`_elapsed`, ...): a column
// view generates them over its row, a value holds them; `_touch()` stores the row tick once.

const STRUCT_SIZE = { timer: 24, stopwatch: 16 };

const STOPWATCH_READ = {
  get elapsed() {
    return this._elapsed;
  },
  get elapsed_secs() {
    return Math.fround(this._elapsed);
  },
  get is_paused() {
    return this._paused;
  },
  toJSON() {
    return { elapsed: this._elapsed, paused: this._paused };
  },
};

const STOPWATCH_WRITE = {
  tick(delta) {
    if (!this._paused) this._elapsed += delta;
    this._touch();
    return this;
  },
  pause() {
    this._paused = true;
    this._touch();
  },
  unpause() {
    this._paused = false;
    this._touch();
  },
  reset() {
    this._elapsed = 0;
    this._touch();
  },
  setElapsed(seconds) {
    this._elapsed = seconds;
    this._touch();
  },
};

const TIMER_READ = {
  get elapsed() {
    return this._elapsed;
  },
  get elapsed_secs() {
    return Math.fround(this._elapsed);
  },
  get duration() {
    return this._duration;
  },
  get mode() {
    return this._mode;
  },
  get is_paused() {
    return this._paused;
  },
  get is_finished() {
    return this._finished;
  },
  get just_finished() {
    return this._times > 0;
  },
  get times_finished_this_tick() {
    return this._times;
  },
  get remaining() {
    // Bevy's `duration - elapsed`. Bevy's `Duration` subtraction panics when `elapsed` is past
    // `duration` (after `setElapsed`); TS clamps to 0.
    return Math.max(0, this._duration - this._elapsed);
  },
  get remaining_secs() {
    return Math.fround(this.remaining);
  },
  get fraction() {
    return this._duration === 0 ? 1 : Math.fround(Math.fround(this._elapsed) / Math.fround(this._duration));
  },
  get fraction_remaining() {
    return Math.fround(1 - this.fraction);
  },
  toJSON() {
    return { elapsed: this._elapsed, duration: this._duration, times_finished_this_tick: this._times, mode: this._mode, paused: this._paused, finished: this._finished };
  },
};

const TIMER_WRITE = {
  tick(delta) {
    this._tick(delta);
    this._touch();
    return this;
  },
  _tick(delta) {
    if (this._paused) {
      this._times = 0;
      if (this._mode === "Repeating") this._finished = false;
      return;
    }
    if (this._mode !== "Repeating" && this._finished) {
      this._times = 0;
      return;
    }
    let elapsed = this._elapsed + delta;
    const duration = this._duration;
    this._finished = elapsed >= duration;
    if (this._finished) {
      if (this._mode === "Repeating") {
        // Bevy divides nanoseconds: by a zero duration, u32::MAX finishes and no time left.
        this._times = duration > 0 ? Math.min(Math.floor(elapsed / duration), 4294967295) : 4294967295;
        elapsed = duration > 0 ? elapsed % duration : 0;
      } else {
        this._times = 1;
        elapsed = duration;
      }
    } else {
      this._times = 0;
    }
    this._elapsed = elapsed;
  },
  pause() {
    this._paused = true;
    this._touch();
  },
  unpause() {
    this._paused = false;
    this._touch();
  },
  reset() {
    this._elapsed = 0;
    this._finished = false;
    this._times = 0;
    this._touch();
  },
  finish() {
    this._tick(this.remaining);
    this._touch();
  },
  almostFinish() {
    this._tick(Math.max(0, this.remaining - 1e-9));
    this._touch();
  },
  setElapsed(seconds) {
    this._elapsed = seconds;
    this._touch();
  },
  setDuration(seconds) {
    this._duration = seconds;
    this._touch();
  },
  setMode(mode) {
    if (mode !== "Once" && mode !== "Repeating") throw new RangeError(`unknown TimerMode ${mode}`);
    if (this._mode !== "Repeating" && mode === "Repeating" && this._finished) {
      this._elapsed = 0;
      this._finished = this._times > 0;
    }
    this._mode = mode;
    this._touch();
  },
};

const STRUCT_METHODS = { timer: [TIMER_READ, TIMER_WRITE], stopwatch: [STOPWATCH_READ, STOPWATCH_WRITE] };

/** Copies `proto`'s properties, getters included, onto `target`, keeping what it defines itself. */
function mixIn(target, proto) {
  for (const k of Object.getOwnPropertyNames(proto)) {
    if (!Object.prototype.hasOwnProperty.call(target, k)) Object.defineProperty(target, k, Object.getOwnPropertyDescriptor(proto, k));
  }
}

/** A timer or stopwatch value: `Timer.fromSeconds(2, "Repeating")`, for an initialiser or `C.of`. */
class StructValue {
  _touch() {}
}
class TimerValue extends StructValue {
  constructor(duration, mode) {
    super();
    if (mode !== "Once" && mode !== "Repeating") throw new RangeError(`unknown TimerMode ${mode}`);
    this._elapsed = 0;
    this._duration = duration;
    this._times = 0;
    this._mode = mode;
    this._paused = false;
    this._finished = false;
  }
}
mixIn(TimerValue.prototype, TIMER_READ);
mixIn(TimerValue.prototype, TIMER_WRITE);
class StopwatchValue extends StructValue {
  constructor() {
    super();
    this._elapsed = 0;
    this._paused = false;
  }
}
mixIn(StopwatchValue.prototype, STOPWATCH_READ);
mixIn(StopwatchValue.prototype, STOPWATCH_WRITE);

/** A struct field's raw value by its layout name, from a value, a view or a plain object. */
function structPart(v, f) {
  if (v instanceof StructValue || (v && f.raw in v)) return v[f.raw];
  return v ? v[f.name] : undefined;
}

// ---------------------------------------------------------------------------------------------
// Declarations: plain classes whose fields the compile step read (operator ruling 2026-10-20)
//
// `class Health extends Component { hp: f32 = 100 }` compiles with a trailing
// `static { __bevyts.data(this, {...}) }` holding its Rust-style name, fields and scene props
// (design decision 13), so the class carries `componentName` from the moment it is defined.

/** What kind of declaration a class is. */
const KIND = Symbol.for("bevy-ts.kind");
const STATE_VALUES = Symbol("bevy-ts.states");

class Data {
  constructor(init) {
    // Class fields initialise after this constructor, so values passed here would be lost.
    if (init !== undefined) throw new TypeError(`${new.target.name}: make values with ${new.target.name}.of({ ... }), not new ${new.target.name}({ ... })`);
  }
  /** `Health.of({ hp: 3 })`, Rust's `Health { hp: 3.0, ..default() }`. */
  static of(values) {
    const v = new this();
    if (values) for (const k of Object.keys(values)) v[k] = values[k];
    return v;
  }
}
export class Component extends Data {}
/** Bevy's `#[derive(SceneComponent)]`: a component whose `static scene(props)` is spawned with it. */
export class SceneComponent extends Component {}
/** A Rust component reachable only through reflection: typed, usable in filters and TSX. */
export class ReflectComponent extends Data {}
export class Resource extends Data {}
export class Message extends Data {}
export class Event extends Data {}
/** An event that targets its `entity`; `static propagate = true` bubbles it up `ChildOf`. */
export class EntityEvent extends Data {}
/** `class EnemySet extends SystemSet {}`, Rust's `#[derive(SystemSet)] struct EnemySet;`. */
export class SystemSet {}

/** `class Game extends States("Menu", "Playing") {}`; `Game.Playing` is a value. */
export function States(...values) {
  const at = callerLocation();
  if (values.length === 0 || values.some((v) => typeof v !== "string")) throw declError("States(...values): values are strings, States(\"Menu\", \"Playing\")", at);
  const B = class {};
  B[STATE_VALUES] = values;
  return B;
}

const DATA_KIND = {
  component: "component",
  sceneComponent: "component",
  reflectComponent: "component",
  resource: "resource",
  message: "message",
  event: "event",
  entityEvent: "event",
  state: "state",
  set: "set",
};

/** Scene components defined outside a generation (built-in modules), by name and by `module#export`. */
const SCENES = new Map();
/**
 * Classes defined outside a generation: a built-in module's (a port's `bevy-ts/rapier`), which
 * evaluates once. Every generation declares them as its own.
 */
const OUTSIDE = [];

/** The scene component class a level names (`enemies::Grunt` or `src/enemies.tsx#Grunt`), in the live generation. */
export function sceneComponent(nameOrId) {
  const g = S.loading || S.gens.get(S.live);
  return (g && g.sceneComponents.get(nameOrId)) || SCENES.get(nameOrId);
}

function vectorJson(v) {
  return v && typeof v.toArray === "function" ? v.toArray() : v;
}

/** The compile step's class declaration, run by the class's static block. */
function dataFn(C, d) {
  const at = d.at;
  if (d.error) throw declError(d.error, at);
  const kind = DATA_KIND[d.kind];
  if (!kind) throw declError(`${C.name}: unknown declaration kind ${d.kind}`, at);
  const own = (k, v) => Object.defineProperty(C, k, { value: v, writable: true, configurable: true, enumerable: false });
  own("componentName", d.name);
  own(KIND, kind);
  own("id", -1);
  own("layout", null);
  own("declId", d.id);
  own("source", d.kind === "reflectComponent" ? "rust" : "ts");
  if (kind === "set") {
    own("set", d.name);
  } else if (kind === "state") {
    const values = d.values;
    own("values", values);
    own("decl", { name: d.name, values: [...values], location: at });
    for (const v of values) own(v, Object.freeze({ state: d.name, value: v, machine: C }));
  } else {
    // Defaults: the class's own initialisers (any expression), else the literal the compiler read.
    let inst = null;
    try {
      inst = new C();
    } catch {}
    const fields = d.fields.map((f) => {
      const out = { name: f.name, type: f.type, hints: {} };
      const v = inst ? vectorJson(inst[f.name]) : undefined;
      if (v !== undefined) out.default = v;
      else if (f.default !== undefined) out.default = f.default;
      return out;
    });
    const decl = { name: d.name, fields, location: at };
    if (d.id) decl.id = d.id;
    if (d.kind === "entityEvent") Object.assign(decl, { entity: true, propagate: !!d.propagate });
    if (d.kind === "event") Object.assign(decl, { entity: false, propagate: false });
    own("decl", decl);
    own("fieldNames", fields.map((f) => f.name));
    if (d.kind === "sceneComponent") {
      decl.scene = { props: d.scene.props };
      own("sceneProps", d.scene.props.map((p) => p.name));
    }
  }
  const g = S.loading;
  if (g) {
    g.classes.push(C);
    if (d.kind === "sceneComponent") {
      g.sceneComponents.set(d.name, C);
      if (d.id) g.sceneComponents.set(d.id, C);
    }
  } else {
    const i = OUTSIDE.findIndex((X) => X.componentName === d.name && X[KIND] === kind);
    if (i >= 0) OUTSIDE[i] = C;
    else OUTSIDE.push(C);
    if (d.kind === "sceneComponent") {
      SCENES.set(d.name, C);
      if (d.id) SCENES.set(d.id, C);
    }
  }
}

/** A Rust component with a native layout (`Transform`). */
function nativeClass(name, typePath, fields, kind = "component") {
  const C = class {
    constructor(init) {
      if (init) Object.assign(this, init);
    }
    /** `Velocity.of({ linear: [0, 3, 0] })`, as for TS classes. */
    static of(values) {
      return new this(values);
    }
  };
  Object.defineProperty(C, "name", { value: name });
  C.componentName = name;
  C.typePath = typePath;
  C.source = "rust";
  C.native = true;
  C[KIND] = kind;
  C.schema = fields;
  C.decl = { name, fields: fieldDecls(name, fields) };
  C.id = -1;
  C.layout = null;
  NATIVE.push(C);
  return C;
}

const NATIVE = [];

/**
 * A Rust component with a native layout registered in Rust (a port's `Velocity`, a native
 * crate's `Thrust`): `class Thrust extends nativeComponent("Thrust", { power: f32 }) {}`. The
 * fields type the accessors; Rust's layout places them.
 */
export function nativeComponent(name, fields) {
  const at = callerLocation();
  const C = class {
    constructor(init) {
      if (init) Object.assign(this, init);
    }
    /** `Velocity.of({ linear: [0, 3, 0] })`, as for TS classes. */
    static of(values) {
      return new this(values);
    }
  };
  Object.defineProperty(C, "name", { value: name.split("::").pop() });
  C.componentName = name;
  C.source = "rust";
  C.native = true;
  C[KIND] = "component";
  C.schema = fields;
  C.decl = { name, fields: fieldDecls(name, fields, at) };
  C.id = -1;
  C.layout = null;
  if (S.loading) S.loading.classes.push(C);
  else NATIVE.push(C);
  return C;
}

/** A Rust component by type path or short name: usable in `With`/`Without` only. */
export function rustComponent(name) {
  const C = class {};
  Object.defineProperty(C, "name", { value: name.split("::").pop() });
  C.componentName = name;
  C.source = "rust";
  C[KIND] = "component";
  C.id = -1;
  C.layout = null;
  return C;
}

export const Transform = nativeClass("Transform", "bevy_transform::components::transform::Transform", {
  translation: vec3.default([0, 0, 0]),
  rotation: quat.default([0, 0, 0, 1]),
  scale: vec3.default([1, 1, 1]),
});

// Bevy's clocks, read-only views built per run (docs/time.md). Every clock's fields first, then
// its own; Rust lays them out and fills them.
const CLOCK = { delta_secs: f32, elapsed_secs: f32, delta_secs_f64: f64, elapsed_secs_f64: f64, elapsed_secs_wrapped_f64: f64, wrap_period: f64, elapsed_secs_wrapped: f32 };
/** `Res<Time>`: Bevy's own field names; the fixed step in `FixedUpdate`. `Time<Real>`, `Time<Virtual>` and `Time<Fixed>` are `Time.clocks`. */
export const Time = nativeClass("Time", "bevy_time::time::Time", CLOCK, "resource");
Time.clocks = {
  Real: nativeClass("Time<Real>", "bevy_time::time::Time<bevy_time::real::Real>", CLOCK, "resource"),
  Virtual: nativeClass("Time<Virtual>", "bevy_time::time::Time<bevy_time::virt::Virtual>", {
    ...CLOCK, is_paused: bool, was_paused: bool, relative_speed: f32, effective_speed: f32, relative_speed_f64: f64, effective_speed_f64: f64, max_delta: f64,
  }, "resource"),
  Fixed: nativeClass("Time<Fixed>", "bevy_time::time::Time<bevy_time::fixed::Fixed>", { ...CLOCK, overstep_fraction: f32, timestep: f64, overstep: f64, overstep_fraction_f64: f64 }, "resource"),
};
/** Bevy's clock markers, for `Time<Real>` and the like; types only, never values at run time. */
export class Real {}
export class Virtual {}
export class Fixed {}

// ---------------------------------------------------------------------------------------------
// Schedules, sets, conditions, system configs

const sched = (kind, extra) => Object.freeze({ schedule: kind, ...extra });
export const Startup = sched("startup");
export const PreUpdate = sched("stage", { stage: "preUpdate" });
export const Update = sched("stage", { stage: "update" });
export const PostUpdate = sched("stage", { stage: "postUpdate" });
export const FixedUpdate = sched("stage", { stage: "fixedUpdate" });
export function OnEnter(v) {
  return sched("onEnter", { state: v.state, value: v.value });
}
export function OnExit(v) {
  return sched("onExit", { state: v.state, value: v.value });
}

export function systemSet(name) {
  return Object.freeze({ set: name });
}

export function inState(v) {
  return { condition: true, kind: "inState", state: v.state, value: v.value };
}
export function resourceExists(R) {
  return { condition: true, kind: "resourceExists", resource: R.componentName };
}
export function onMessage(M) {
  return { condition: true, kind: "onMessage", message: M.componentName };
}
export function not(c) {
  return { condition: true, kind: "not", condition_: c };
}

class SystemConfig {
  constructor(items, chained) {
    this.items = items;
    this.chained = chained;
    this.afters = [];
    this.befores = [];
    this.sets = [];
    this.conditions = [];
  }
  after(...x) {
    this.afters.push(...x);
    return this;
  }
  before(...x) {
    this.befores.push(...x);
    return this;
  }
  inSet(s) {
    this.sets.push(s);
    return this;
  }
  runIf(c) {
    this.conditions.push(c);
    return this;
  }
}

/** Bevy's tuple of systems: `systems(a, b).after(x).runIf(c)`. */
export function systems(...items) {
  return new SystemConfig(items, false);
}

/** Each runs after the previous, as Bevy's `.chain()`. */
export function chain(...items) {
  return new SystemConfig(items, true);
}

// ---------------------------------------------------------------------------------------------
// The compile step's declarations, and the setup-closure fallback

const DECL = Symbol.for("bevy-ts.decl");
const SETUP = Symbol.for("bevy-ts.setup");

/** A system whose param types are computed: `setup` declares params and returns the body. */
export function system(setup) {
  if (typeof setup !== "function") throw declError("system(setup): setup must be a function", callerLocation());
  const s = new SystemConfig([], false);
  s[SETUP] = setup;
  s.at = callerLocation();
  return s;
}

function declaring(what) {
  if (!S.declaring) throw declError(`${what} declares a param and may only be called in a system(() => ...) setup`, callerLocation());
  return S.declaring;
}

const dataOf = (d) =>
  d === EntityId ? { entity: true } : d && d[OPT] ? { ...dataOf(d[OPT]), optional: true } : d && d[MUT] ? { ...dataOf(d[MUT]), mut: true } : { component: d };

const MUT = Symbol("bevy-ts.mut");
const OPT = Symbol("bevy-ts.option");
/** `Mut(C)` in a setup's `query(...)`. */
export function Mut(C) {
  return { [MUT]: C };
}
/** `Option(C)` in a setup's `query(...)`. */
export function Option(C) {
  return { [OPT]: C };
}
/** `Entity` as query data in a setup's `query(...)`. */
export const EntityId = Object.freeze({ entity: true });

class SetupQuery {
  constructor(desc) {
    this.desc = desc;
  }
  with(...cs) {
    this.desc.filter.push(...cs.map((c) => ({ with: c })));
    return this;
  }
  without(...cs) {
    this.desc.filter.push(...cs.map((c) => ({ without: c })));
    return this;
  }
  changed(...cs) {
    this.desc.filter.push(...cs.map((c) => ({ changed: c })));
    return this;
  }
  added(...cs) {
    this.desc.filter.push(...cs.map((c) => ({ added: c })));
    return this;
  }
}

/** Declares a query param in a setup; the returned object is the query each run. */
export function query(...data) {
  const list = declaring("query()");
  const desc = { kind: "query", data: data.map(dataOf), filter: [], live: true };
  const holder = new SetupQuery(desc);
  list.push({ desc, holder });
  return holder;
}
function setupParam(what, desc) {
  const list = declaring(what);
  const holder = {};
  list.push({ desc, holder });
  return holder;
}
export function res(R) {
  // In tsx code (a binding, a scene), `res(R)` reads the resource; elsewhere it is a parameter.
  const v = readResource(R);
  if (v !== NOT_TSX) return v;
  return setupParam("res()", { kind: "res", resource: R, live: true });
}
export function resMut(R) {
  return setupParam("resMut()", { kind: "resMut", resource: R, live: true });
}
export function commands() {
  return setupParam("commands()", { kind: "commands", live: true });
}
export function local(fields) {
  return setupParam("local()", { kind: "local", fields: fieldDecls("Local", fields), live: true });
}

const FIELD_BY_TYPE = { f32, f64, i8, u8, i16, u16, i32, u32, bool, vec2, vec3, vec4, quat, entity, timer: Timer, stopwatch: Stopwatch };

// The compile step's hooks, installed on the host object below.
function bindModule(module, names) {
  const g = S.loading;
  if (!g) return;
  for (const [name, thunk] of Object.entries(names)) g.bindings.set(`${module}\u0000${name}`, thunk);
}

function declareFn(fn, desc) {
  if (fn && (typeof fn === "function" || typeof fn === "object")) fn[DECL] = desc;
}

// ---------------------------------------------------------------------------------------------
// The app

function flatten(schedule, items, inherited, out) {
  let prev = null;
  for (const item of items) {
    const cfg = {
      after: [...inherited.after],
      before: [...inherited.before],
      sets: [...inherited.sets],
      conditions: [...inherited.conditions],
    };
    if (item instanceof SystemConfig && !item[SETUP]) {
      const inner = { after: [...cfg.after, ...item.afters], before: [...cfg.before, ...item.befores], sets: [...cfg.sets, ...item.sets], conditions: [...cfg.conditions, ...item.conditions] };
      if (prev && inherited.chained) inner.after.push(prev);
      const first = out.length;
      flatten(schedule, item.items, { ...inner, chained: item.chained }, out);
      // A nested group's members are its own "last" for the outer chain.
      prev = out.slice(first).map((r) => r.target);
      continue;
    }
    if (prev && inherited.chained) cfg.after.push(prev);
    const target = item;
    if (item instanceof SystemConfig && item[SETUP]) {
      cfg.after.push(...item.afters);
      cfg.before.push(...item.befores);
      cfg.sets.push(...item.sets);
      cfg.conditions.push(...item.conditions);
    }
    out.push({ schedule, target, ...cfg, at: callerLocation() });
    prev = [target];
  }
}

function loadingOrThrow(what) {
  if (!S.loading) throw declError(`${what} must be called while a generation loads`, callerLocation());
  return S.loading;
}

export const app = {
  /** `app.addSystems(Update, chain(a, b).runIf(inState(Game.Playing)))` */
  addSystems(schedule, ...items) {
    const g = loadingOrThrow("addSystems");
    if (!schedule || !schedule.schedule) throw declError("addSystems(schedule, ...systems): the first argument must be a schedule (Update, Startup, OnEnter(...))", callerLocation());
    flatten(schedule, items, { after: [], before: [], sets: [], conditions: [], chained: false }, g.registrations);
    return app;
  },
  /** Registers an observer: a function whose first param is `On<E>`. */
  addObserver(fn) {
    const g = loadingOrThrow("addObserver");
    g.registrations.push({ schedule: { schedule: "observer" }, target: fn, after: [], before: [], sets: [], conditions: [], at: callerLocation() });
    return app;
  },
  /** Runs plugins (functions of the app) now; every generation re-runs them. */
  addPlugins(...plugins) {
    loadingOrThrow("addPlugins");
    for (const p of plugins.flat()) {
      if (typeof p !== "function") throw declError(`addPlugins: ${String(p)} is not a plugin function`, callerLocation());
      p(app);
    }
    return app;
  },
  insertResource(value) {
    const g = loadingOrThrow("insertResource");
    const C = value && value.constructor;
    if (!C || C[KIND] !== "resource") throw declError("insertResource: pass a Resource value (Score.of({ value: 0 }))", callerLocation());
    g.resourceInserts.set(C.componentName, { ...value });
    return app;
  },
  addMessage(M) {
    const g = loadingOrThrow("addMessage");
    if (!M || M[KIND] !== "message") throw declError("addMessage: pass a Message class", callerLocation());
    g.addedMessages.add(M.componentName);
    return app;
  },
  initState(v) {
    const g = loadingOrThrow("initState");
    if (!v || !v.machine) throw declError("initState: pass a state value (Game.Menu)", callerLocation());
    g.initialStates.set(v.state, v.value);
    return app;
  },
  /**
   * Declares a scene root. Its function runs now, during the load, and its description goes to
   * the `tsx` reconciler with the generation; a throw fails the generation.
   */
  scene(name, fn, options) {
    const at = callerLocation();
    const g = loadingOrThrow("app.scene");
    if (typeof fn !== "function") throw declError(`scene ${name}: must be a function returning elements`, at);
    if (g.scenes.has(name)) throw declError(`scene ${name} is declared twice`, at);
    let nodes = [];
    const root = runScene(g.tsx, fn);
    try {
      nodes = describe(root, g.tsx);
    } catch (e) {
      if (!e || !e.bevyTsLocation) throw e;
      g.sceneErrors.push({ message: String(e.message), location: e.bevyTsLocation });
    }
    const desc = { name, location: at, nodes };
    if (options && typeof options.level === "string") desc.level = options.level;
    g.scenes.set(name, desc);
    section("tsx", { roots: [...g.scenes.values()], errors: g.sceneErrors });
    return app;
  },
  /**
   * Declares a level root: a `.bsn` file under the asset root, loaded and reconciled by
   * `bevy-ts-levels` on every generation.
   */
  level(path) {
    const at = callerLocation();
    const g = loadingOrThrow("app.level");
    if (typeof path !== "string" || !path.endsWith(".bsn")) throw declError(`app.level(${JSON.stringify(path)}): expected a path to a .bsn file`, at);
    g.levels ??= [];
    if (g.levels.includes(path)) throw declError(`level ${path} is declared twice`, at);
    g.levels.push(path);
    section("levels", { levels: g.levels });
    return app;
  },
  /** Declares an entry Rust plugins call by name with a byte payload. */
  entry(name, fn) {
    const at = callerLocation();
    const g = loadingOrThrow("app.entry");
    if (typeof fn !== "function") throw declError(`entry ${name}: must be a function`, at);
    g.entries.push({ name, location: at });
    g.entryFns.set(name, fn);
    return app;
  },
};

/** Writes a manifest section for a Rust plugin: bytes, a string, or a JSON-serializable value. */
export function section(name, value) {
  const at = callerLocation();
  const g = loadingOrThrow(`section(${name})`);
  let bytes;
  if (value instanceof Uint8Array) bytes = value;
  else if (typeof value === "string") bytes = encoder.encode(value);
  else bytes = encoder.encode(JSON.stringify(value));
  g.sections.set(name, { bytes, location: at });
}

/**
 * The loading generation's list for a JSON section, `{ items: [...] }`, created on first use. For
 * a built-in module, which is evaluated once, whose declarations come from each generation's code
 * (`material()` in `bevy-ts/material`). The section's location is the first caller's.
 */
export function sectionItems(name) {
  const g = loadingOrThrow(`sectionItems(${name})`);
  let s = g.sections.get(name);
  if (!s || !s.items) {
    s = { items: [], location: callerLocation() };
    g.sections.set(name, s);
  }
  return s.items;
}

/** Decodes UTF-8 bytes, for entries that take text or JSON payloads. */
export function text(bytes) {
  return decoder.decode(bytes);
}

// ---------------------------------------------------------------------------------------------
// Resolving declarations at the end of a load

function refClass(g, ref, at) {
  if (typeof ref === "function" || (ref && typeof ref === "object" && !Array.isArray(ref))) return ref;
  const [module, name, clock] = ref;
  const thunk = g.bindings.get(`${module}\u0000${name}`);
  if (!thunk) throw declError(`${name} is not bound in ${module}; was the module compiled by bevy-ts?`, at);
  const v = thunk();
  if (v === undefined) throw declError(`${name} has no value at run time; import it as a value, not with import type`, at);
  // `Time<Real>`: the reader names the clock beside `Time`.
  if (clock !== undefined) {
    const c = v.clocks && v.clocks[clock];
    if (!c) throw declError(`${name}<${clock}>: ${name} has no clock ${clock}`, at);
    return c;
  }
  return v;
}

function componentRef(C, at, what) {
  if (!C || !C.componentName || (C[KIND] && C[KIND] !== "component")) throw declError(`${what}: ${String(C && C.name)} is not a component class`, at);
  return { name: C.componentName, source: C.source };
}

/** Turns a declaration's params (refs to bound names) into manifest params (names). */
function resolveParams(g, params, at) {
  const out = [];
  for (const p of params) {
    switch (p.kind) {
      case "query": {
        const data = p.data.map((d) => {
          if (d.entity) return { entity: true };
          const C = refClass(g, d.component, at);
          return { component: componentRef(C, at, "query data"), mutable: !!(d.mut || d.mutable), optional: !!d.optional, cls: C };
        });
        const filter = p.filter.map((f) => {
          const [k] = Object.keys(f);
          return { [k]: componentRef(refClass(g, f[k], at), at, `${k} filter`) };
        });
        out.push({ kind: "query", data, filter });
        break;
      }
      case "res":
      case "resMut": {
        const R = refClass(g, p.resource, at);
        if (R[KIND] !== "resource") throw declError(`${p.kind === "res" ? "Res" : "ResMut"}<${R.name}>: ${R.name} is not a Resource class`, at);
        out.push({ kind: p.kind, resource: R.componentName, cls: R });
        break;
      }
      case "local":
        out.push({ kind: "local", fields: p.fields.map((f) => (typeof f.type === "string" ? { name: f.name, type: FIELD_BY_TYPE[f.type].spec, hints: {} } : f)) });
        break;
      case "commands":
        out.push({ kind: "commands" });
        break;
      case "messageReader":
      case "messageWriter": {
        const M = refClass(g, p.message, at);
        if (M[KIND] !== "message") throw declError(`${M.name} is not a Message class`, at);
        out.push({ kind: p.kind, message: M.componentName, cls: M });
        break;
      }
      case "state":
      case "nextState": {
        const St = refClass(g, p.state, at);
        if (St[KIND] !== "state") throw declError(`${St.name} is not a States class`, at);
        out.push({ kind: p.kind, state: St.componentName, cls: St });
        break;
      }
      case "on":
        if (p.lifecycle) {
          out.push({ kind: "on", lifecycle: p.lifecycle, component: componentRef(refClass(g, p.component, at), at, "On<...>") });
        } else {
          const E = refClass(g, p.event, at);
          if (E[KIND] !== "event") throw declError(`On<${E.name}>: ${E.name} is not an Event class`, at);
          out.push({ kind: "on", event: E.componentName, cls: E });
        }
        break;
      case "bundle":
        out.push({ kind: "bundle", fields: p.fields.map((f) => ({ name: f.name, param: resolveParams(g, [f.param], at)[0] })) });
        break;
      case "port":
        out.push({ kind: "port", name: p.name });
        break;
      case "app":
        throw declError("a plugin (app: App) is registered with app.addPlugins, not as a system", at);
      default:
        throw declError(`unknown param kind ${p.kind}`, at);
    }
  }
  return out;
}

/** Strips runtime-only properties for the manifest JSON. */
function manifestParams(params) {
  return params.map((p) => {
    const { cls, ...rest } = p;
    if (rest.kind === "query") rest.data = rest.data.map(({ cls, ...d }) => d);
    if (rest.kind === "bundle") rest.fields = rest.fields.map((f) => ({ name: f.name, param: manifestParams([f.param])[0] }));
    return rest;
  });
}

function identityOf(target) {
  const d = target && target[DECL];
  return d && d.id ? d.id : null;
}

function resolveCondition(c) {
  if (c.kind === "not") return { kind: "not", condition: resolveCondition(c.condition_) };
  const { condition, ...rest } = c;
  return rest;
}

function setName(x, at) {
  if (x && typeof x.set === "string") return x.set;
  throw declError(`inSet: ${String(x && x.name)} is not a system set (class X extends SystemSet {} or systemSet(name))`, at);
}

function orderTargets(list, at) {
  return list.flat().map((x) => {
    if (x && typeof x.set === "string") return x.set;
    const id = identityOf(x);
    if (!id) throw declError(`ordering on ${String(x && x.name)}: not a registered system or set`, at);
    return id;
  });
}

function resolveRegistrations(g) {
  for (const r of g.registrations) {
    const t = r.target;
    let d = t && t[DECL];
    const name = (t && t.name) || "anonymous";
    if (t instanceof SystemConfig && t[SETUP]) {
      // The fallback: run the setup once now; its declaring calls are the params.
      S.declaring = [];
      let body;
      try {
        body = t[SETUP]();
      } finally {
        const decls = S.declaring;
        S.declaring = null;
        t.setupParams = decls;
      }
      if (typeof body !== "function") throw declError("system(setup): setup must return the per-run body", t.at);
      const id = (d && d.id) || `${(t.at && t.at.file) || "?"}#system@${t.at ? t.at.line : 0}`;
      const params = resolveParams(g, t.setupParams.map((x) => x.desc), r.at);
      g.systems.push({ r, id, params, call: "setup", body, holders: t.setupParams.map((x) => x.holder), at: d && d.at ? d.at : t.at });
      continue;
    }
    if (typeof t !== "function") throw declError(`addSystems: ${String(t)} is not a system`, r.at);
    if (!d) throw declError(`${name} was compiled without bevy-ts: its param types were never read; load it through bevy-ts (or its Vite/vitest/esbuild plugin)`, r.at);
    if (d.error) throw declError(d.error, d.at || r.at);
    const params = resolveParams(g, d.params || [], d.at || r.at);
    if (r.schedule.schedule === "observer" && (!params[0] || params[0].kind !== "on")) {
      throw declError(`${name}: an observer's first param must be On<...>`, d.at || r.at);
    }
    g.systems.push({ r, id: d.id, params, call: "fn", fn: t, at: d.at || r.at });
  }
}

function newGeneration(number) {
  return {
    number,
    classes: [...OUTSIDE],
    bindings: new Map(),
    registrations: [],
    systems: [],
    resourceInserts: new Map(),
    addedMessages: new Set(),
    initialStates: new Map(),
    entries: [],
    entryFns: new Map(),
    sections: new Map(),
    scenes: new Map(),
    sceneComponents: new Map(),
    sceneErrors: [],
    tsx: new TsxContext(),
    built: null,
  };
}

function scheduleDecl(s) {
  switch (s.schedule) {
    case "stage":
      return { kind: "stage", stage: s.stage };
    case "onEnter":
    case "onExit":
      return { kind: s.schedule, state: s.state, value: s.value };
    default:
      return { kind: s.schedule };
  }
}

function begin(gen, envJson) {
  S.loading = newGeneration(gen);
  if (web) {
    if (envJson) web.setEnv(JSON.parse(envJson));
    web.generation = gen;
  }
  // The tsx host call: Rust calls it only when this generation has bindings or slots to run.
  const g = S.loading;
  g.entries.push({ name: "tsx" });
  // The payload is a u32 header length, a JSON header, then site rows as raw bytes.
  g.entryFns.set("tsx", (payload, ctx) => {
    const n = new DataView(payload.buffer, payload.byteOffset, payload.byteLength).getUint32(0, true);
    return g.tsx.call(JSON.parse(text(payload.subarray(4, 4 + n))), ctx, payload.subarray(4 + n));
  });
}

function end() {
  const g = S.loading;
  if (!g) throw new Error("bevy-ts: end without begin");
  try {
    resolveRegistrations(g);
  } finally {
    S.loading = null;
    if (web) web.generation = S.live;
  }
  const seen = new Map();
  for (const C of g.classes) {
    if (C.source !== "ts") continue;
    const key = `${C[KIND] === "state" || C[KIND] === "set" ? C[KIND] : "data"}\u0000${C.componentName}`;
    const other = seen.get(key);
    if (other && other !== C) {
      const where = (X) => (X.decl && X.decl.location ? `${X.decl.location.file}:${X.decl.location.line}` : X.name);
      throw declError(`${C.componentName} is declared twice: ${where(other)} and ${where(C)}; pin one with static path = "..."`, C.decl && C.decl.location);
    }
    seen.set(key, C);
  }
  S.gens.set(g.number, g);
  const byKind = (k) => g.classes.filter((C) => C[KIND] === k);
  const manifest = {
    components: byKind("component").filter((C) => C.source === "ts").map((C) => C.decl),
    resources: byKind("resource").map((C) => ({ ...C.decl, insert: g.resourceInserts.has(C.componentName) ? jsonValues(C, g.resourceInserts.get(C.componentName)) : undefined })),
    messages: byKind("message").map((C) => ({ ...C.decl, added: g.addedMessages.has(C.componentName) })),
    events: byKind("event").map((C) => C.decl),
    states: byKind("state").map((C) => ({ ...C.decl, initial: g.initialStates.get(C.componentName) })),
    systems: g.systems.map((s) => ({
      name: s.id,
      schedule: scheduleDecl(s.r.schedule),
      params: manifestParams(s.params),
      after: orderTargets(s.r.after, s.at),
      before: orderTargets(s.r.before, s.at),
      sets: s.r.sets.map((x) => setName(x, s.at)),
      condition: s.r.conditions.length ? { kind: "all", conditions: s.r.conditions.map(resolveCondition) } : undefined,
      location: s.at,
    })),
    entries: g.entries,
  };
  const sections = [...g.sections].map(([name, s]) => [name, s.items ? encoder.encode(JSON.stringify({ items: s.items })) : s.bytes, s.location || null]);
  return [JSON.stringify(manifest), sections];
}

/** Field values as JSON for Rust (vectors as arrays). */
function jsonValues(C, values) {
  const out = {};
  for (const [k, v] of Object.entries(values)) out[k] = v && typeof v.toArray === "function" ? v.toArray() : v instanceof StructValue ? v.toJSON() : v;
  return out;
}

function abort() {
  if (web && S.loading) web.dropOnly(S.loading.number);
  if (web) web.generation = S.live;
  S.loading = null;
  S.declaring = null;
}

// ---------------------------------------------------------------------------------------------
// Accessors: generated per layout from Rust's offsets

const WIDTH = { f32: 4, f64: 8, i8: 1, u8: 1, i16: 2, u16: 2, i32: 4, u32: 4, bool: 1, entity: 4 };
const VIEW = { f32: "f32", f64: "f64", i8: "i8", u8: "u8", i16: "i16", u16: "u16", i32: "i32", u32: "u32", bool: "u8", entity: "u32" };
const VIEW_CTORS = { f32: Float32Array, f64: Float64Array, i8: Int8Array, u8: Uint8Array, i16: Int16Array, u16: Uint16Array, i32: Int32Array, u32: Uint32Array };

function enumView(t) {
  if (t.repr) return t.repr.width === 1 ? "u8" : t.repr.width === 2 ? "u16" : "u32";
  const n = t.variants.length;
  return n <= 256 ? "u8" : n <= 65536 ? "u16" : "u32";
}

function scalarKind(t) {
  return t.kind === "enum" ? enumView(t) : t.kind;
}

function viewsFor(layout) {
  const need = new Set();
  const add = (t) => {
    if (LANES[t.kind]) need.add("f32");
    else if (t.kind === "enum") need.add(enumView(t));
    else if (t.kind === "array") add(t.of);
    else if (STRUCT_FIELDS[t.kind]) STRUCT_FIELDS[t.kind].forEach((f) => add(f.type));
    else need.add(VIEW[t.kind]);
  };
  for (const f of layout.fields) add(f.type);
  return [...need];
}

const READONLY = "this value is read-only here: query it as Mut<C> (or take ResMut) to change it";

function setterTail(write) {
  return write ? "p.t[p.r] = p.k;" : "throw new TypeError(READONLY);";
}

/** Source for reading/writing one scalar at byte offset `off` of a `size`-byte row. */
/** `enumType`: for an enum field, its type; a Rust enum's `repr` maps variants to the bytes this
 * build stores, read at startup. */
function scalarCode(kind, off, size, enumType) {
  const enumVariants = enumType && enumType.variants;
  const w = WIDTH[kind] || (kind === "u16" ? 2 : 4);
  const view = kind === "bool" ? "u8" : kind === "entity" ? "u32" : kind;
  const idx = size % w === 0 ? `p.r * ${size / w} + ${off / w}` : `(p.r * ${size} + ${off}) / ${w}`;
  if (kind === "bool") return { get: `p.u8[${idx}] !== 0`, set: `p.u8[${idx}] = v ? 1 : 0;` };
  if (kind === "entity") {
    return {
      get: `p.u32[${idx}] + p.u32[${idx} + 1] * 4294967296`,
      set: `{ const i = ${idx}; p.u32[i] = v % 4294967296; p.u32[i + 1] = Math.floor(v / 4294967296); }`,
    };
  }
  if (enumVariants) {
    const names = JSON.stringify(enumVariants);
    const vals = enumType.repr ? JSON.stringify(enumType.repr.values) : null;
    return {
      get: vals ? `${names}[${vals}.indexOf(p.${view}[${idx}])]` : `${names}[p.${view}[${idx}]]`,
      set: `{ const i = typeof v === "string" ? ${names}.indexOf(v) : v; if (i < 0 || i >= ${enumVariants.length}) throw new RangeError("unknown variant " + v); p.${view}[${idx}] = ${vals ? `${vals}[i]` : "i"}; }`,
    };
  }
  return { get: `p.${view}[${idx}]`, set: `p.${view}[${idx}] = v;` };
}

/** Writes a whole timer or stopwatch into a view: raw fields, the row tick stored once. */
function copyStruct(view, kind, v) {
  for (const f of STRUCT_FIELDS[kind]) {
    const x = structPart(v, f);
    if (x !== undefined) view[f.raw] = x;
  }
  view._touch();
}

/** Generates the accessor class for a layout. `write` adds tick-storing setters and mutators. */
function generateAccessor(name, layout, write) {
  const size = layout.size;
  const parts = [];
  const subclasses = [];
  const init = [];
  for (const f of layout.fields) {
    const t = f.type;
    const key = JSON.stringify(f.name);
    const slot = JSON.stringify("_" + f.name);
    if (LANES[t.kind]) {
      const lanes = LANES[t.kind];
      const names = ["x", "y", "z", "w"].slice(0, lanes);
      const cls = [`class V { constructor(p) { this.p = p; }`];
      names.forEach((n, i) => {
        const c = scalarCode("f32", f.offset + i * 4, size);
        cls.push(`get ${n}() { const p = this.p; return ${c.get}; }`);
        cls.push(`set ${n}(v) { const p = this.p; ${c.set} ${setterTail(write)} }`);
      });
      if (lanes === 2) cls.push(`get z() { return 0; } set z(v) {}`);
      if (write) {
        // Every vector method routes through set(): lanes written raw, the row tick stored once.
        const args = names.join(", ");
        const raw = names.map((n, i) => scalarCode("f32", f.offset + i * 4, size).set.replace(/\bv\b/g, n)).join(" ");
        cls.push(`set(${args}) { const p = this.p; ${raw} p.t[p.r] = p.k; return this; }`);
      }
      cls.push(`}`, `return V;`);
      subclasses.push({ src: `(function () { ${cls.join("\n")} })()`, quat: t.kind === "quat" || t.kind === "vec4" });
      init.push(`this[${slot}] = new V_${subclasses.length - 1}(this);`);
      parts.push(`get ${key}() { return this[${slot}]; }`);
      parts.push(
        write
          ? `set ${key}(v) { const s = this[${slot}]; if (Array.isArray(v)) s.set(...v); else s.set(${names.map((n) => "v." + n).join(", ")}); }`
          : `set ${key}(v) { throw new TypeError(READONLY); }`,
      );
    } else if (STRUCT_FIELDS[t.kind]) {
      // A timer or stopwatch: raw fields over the row, Bevy's getters and (write) methods mixed in.
      const cls = [`class S { constructor(p) { this.p = p; }`];
      for (const sf of STRUCT_FIELDS[t.kind]) {
        const c = scalarCode(scalarKind(sf.type), f.offset + sf.offset, size, sf.type.kind === "enum" ? sf.type : undefined);
        cls.push(`get ${sf.raw}() { const p = this.p; return ${c.get}; }`);
        // Raw setters write without a tick: only a written view, whose methods store it once.
        if (write) cls.push(`set ${sf.raw}(v) { const p = this.p; ${c.set} }`);
      }
      cls.push(`_touch() { const p = this.p; p.t[p.r] = p.k; }`);
      cls.push(`}`, `return S;`);
      subclasses.push({ src: `(function () { ${cls.join("\n")} })()`, struct: t.kind });
      init.push(`this[${slot}] = new V_${subclasses.length - 1}(this);`);
      parts.push(`get ${key}() { return this[${slot}]; }`);
      parts.push(write ? `set ${key}(v) { copyStruct(this[${slot}], ${JSON.stringify(t.kind)}, v); }` : `set ${key}(v) { throw new TypeError(READONLY); }`);
    } else if (t.kind === "array") {
      const el = t.of;
      const w = el.kind === "enum" ? WIDTH[enumView(el)] : LANES[el.kind] ? LANES[el.kind] * 4 : WIDTH[el.kind];
      const cls = [`class A { constructor(p) { this.p = p; } get length() { return ${t.len}; }`];
      for (let i = 0; i < t.len; i++) {
        const off = f.offset + i * w;
        let c;
        if (LANES[el.kind]) {
          const n = LANES[el.kind];
          const g = [...Array(n).keys()].map((j) => scalarCode("f32", off + j * 4, size).get);
          const s = [...Array(n).keys()].map((j) => scalarCode("f32", off + j * 4, size).set.replace(/\bv\b/g, `v[${j}]`));
          c = { get: `[${g.join(", ")}]`, set: s.join(" ") };
        } else {
          c = scalarCode(scalarKind(el), off, size, el.kind === "enum" ? el : undefined);
        }
        cls.push(`get ${i}() { const p = this.p; return ${c.get}; }`);
        cls.push(`set ${i}(v) { const p = this.p; ${c.set} ${setterTail(write)} }`);
      }
      cls.push(`get(i) { return this[i]; } set(i, v) { if (i < 0 || i >= ${t.len}) throw new RangeError("index " + i); this[i] = v; }`);
      cls.push(`toArray() { return Array.from({ length: ${t.len} }, (_, i) => this[i]); } toJSON() { return this.toArray(); }`);
      cls.push(`*[Symbol.iterator]() { for (let i = 0; i < ${t.len}; i++) yield this[i]; }`);
      cls.push(`}`, `return A;`);
      subclasses.push({ src: `(function () { ${cls.join("\n")} })()` });
      init.push(`this[${slot}] = new V_${subclasses.length - 1}(this);`);
      parts.push(`get ${key}() { return this[${slot}]; }`);
      parts.push(
        write
          ? `set ${key}(v) { const a = this[${slot}]; for (let i = 0; i < ${t.len}; i++) a[i] = v[i]; }`
          : `set ${key}(v) { throw new TypeError(READONLY); }`,
      );
    } else {
      const c = scalarCode(scalarKind(t), f.offset, size, t.kind === "enum" ? t : undefined);
      parts.push(`get ${key}() { const p = this; return ${c.get}; }`);
      parts.push(`set ${key}(v) { const p = this; ${c.set} ${setterTail(write)} }`);
    }
  }
  const views = viewsFor(layout);
  const src = `
    ${subclasses.map((s, i) => `const V_${i} = ${s.src}; mix(V_${i}.prototype, ${s.struct ? JSON.stringify(s.struct) : s.quat ? "true" : "false"});`).join("\n")}
    return class ${write ? "Write" : "Read"} {
      constructor() {
        this.r = 0; this.k = 0; this.t = null;
        ${views.map((v) => `this.${v} = null;`).join(" ")}
        ${init.join("\n")}
      }
      toJSON() { const o = {}; ${layout.fields.map((f) => `o[${JSON.stringify(f.name)}] = this[${JSON.stringify(f.name)}];`).join(" ")} return o; }
      ${parts.join("\n")}
    };`;
  // Shared methods, except those the generated class defines itself (the raw `set`).
  const mix = (proto, isQuat) => {
    if (typeof isQuat === "string") {
      const [read, writes] = STRUCT_METHODS[isQuat];
      mixIn(proto, read);
      if (write) mixIn(proto, writes);
      return;
    }
    const add = (k, fn) => Object.prototype.hasOwnProperty.call(proto, k) || Object.defineProperty(proto, k, { value: fn, writable: true });
    for (const [k, fn] of Object.entries(isQuat ? QUAT_READ : VEC_READ)) add(k, fn);
    if (write) for (const [k, fn] of Object.entries(isQuat ? QUAT_WRITE : VEC_WRITE)) add(k, fn);
  };
  try {
    return new Function("READONLY", "mix", "copyStruct", src)(READONLY, mix, copyStruct);
  } catch (e) {
    throw new Error(`bevy-ts: generating accessors for ${name}: ${e.message}`);
  }
}

/** Gives a class its id, layout and accessor classes from Rust's resolution. */
function resolveClass(C, r) {
  if (!r) return;
  if (C.layout && C.id === r.id && JSON.stringify(C.layout) === JSON.stringify(r.layout)) return;
  C.id = r.id;
  C.layout = r.layout;
  C.defaults = r.defaults ? Uint8Array.from(r.defaults) : null;
  C.rawWrites = r.rawWrites !== false;
  C.views = viewsFor(r.layout);
  C.offsets = Object.fromEntries(r.layout.fields.map((f) => [f.name, f.offset]));
  C.size = r.layout.size;
  C.Read = generateAccessor(C.componentName, r.layout, false);
  C.Write = generateAccessor(C.componentName, r.layout, true);
}

/** Accessor classes for a layout that is not a class (a `Local`). */
function accessorPair(name, layout) {
  return { layout, views: viewsFor(layout), size: layout.size, Read: generateAccessor(name, layout, false), Write: generateAccessor(name, layout, true) };
}

/** Points an accessor at one buffer, row 0. */
function bindRow(acc, T, buffer, ticks, tick) {
  for (const v of T.views) acc[v] = buffer ? new VIEW_CTORS[v](buffer) : null;
  acc.t = ticks ? new Uint32Array(ticks) : DUMMY_TICKS;
  acc.k = tick;
  acc.r = 0;
}

const DUMMY_TICKS = new Uint32Array(1);

/** C layout rules, mirrored for `Local`s (Rust lays them out the same way). */
function cLayout(fields) {
  const size = (t) => {
    if (LANES[t.kind]) return LANES[t.kind] * 4;
    if (t.kind === "enum") return { u8: 1, u16: 2, u32: 4 }[enumView(t)];
    if (t.kind === "array") return size(t.of) * t.len;
    if (STRUCT_SIZE[t.kind]) return STRUCT_SIZE[t.kind];
    return { f32: 4, f64: 8, i8: 1, u8: 1, i16: 2, u16: 2, i32: 4, u32: 4, bool: 1, entity: 8 }[t.kind];
  };
  const align = (t) => (LANES[t.kind] ? 4 : t.kind === "array" ? align(t.of) : STRUCT_SIZE[t.kind] ? 8 : size(t));
  let offset = 0, max = 1;
  const out = [];
  for (const f of fields) {
    const a = align(f.type);
    max = Math.max(max, a);
    offset = Math.ceil(offset / a) * a;
    out.push({ name: f.name, type: f.type, offset });
    offset += size(f.type);
  }
  return { size: Math.ceil(offset / max) * max, align: max, fields: out };
}

// ---------------------------------------------------------------------------------------------
// Param objects: built once per generation, bound to each run's frames

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
  /** For `Changed`/`Added` queries: the rows that passed. */
  get rows() {
    return this.q.raw[this.i][4];
  }
  entity(row) {
    const e = this.q.raw[this.i][1];
    return e[row * 2] + e[row * 2 + 1] * 4294967296;
  }
  col(T) {
    const c = this.q.columnOf.get(T) ?? this.q.columnOf.get(Object.getPrototypeOf(T));
    if (c === undefined) throw new Error(`${T.name} is not data of this query`);
    return c;
  }
  arrays(T, c) {
    const buf = this.q.raw[this.i][2][c];
    const C = this.q.data[this.q.dataIndexOfColumn[c]].cls;
    const out = { buffer: buf };
    for (const n of C.views) out[n] = buf ? new VIEW_CTORS[n](buf) : null;
    out.offsets = C.offsets;
    out.size = C.size;
    out.stride = C.size / 4;
    return out;
  }
  /** A component's typed arrays and byte offsets. */
  read(T) {
    return this.arrays(T, this.col(T));
  }
  /** A written component's arrays; every row of this table is marked changed after the run. */
  write(T) {
    const c = this.col(T);
    if (!this.q.columnWrites[c]) throw new Error(`${T.name} is not Mut in this query`);
    rawWritable(T);
    this.q.marks.push(this.q.paramIndex, this.i, c);
    return this.arrays(T, c);
  }
  /** A written component's arrays plus its `ticks`; mark rows with `ticks[row] = q.tick`. */
  split(T) {
    const c = this.col(T);
    if (!this.q.columnWrites[c]) throw new Error(`${T.name} is not Mut in this query`);
    rawWritable(T);
    const out = this.arrays(T, c);
    out.ticks = new Uint32Array(this.q.raw[this.i][3][c]);
    return out;
  }
}

/** Rust components with hidden fields, or bool, enum and Entity fields, are written only through
 * accessors, which keep every value valid for Rust. */
function rawWritable(T) {
  if (T.rawWrites === false) {
    throw new Error(`${T.name} has fields only accessors may write (bool, enum, Entity, or fields TS cannot see); write it in q.each`);
  }
}

class QueryParam {
  constructor(desc, marks) {
    // `data`: per datum, { entity } or { cls, write, optional, column }.
    this.data = [];
    this.columnWrites = [];
    this.dataIndexOfColumn = [];
    this.columnOf = new Map();
    for (const d of desc.data) {
      if (d.entity) {
        this.data.push({ entity: true });
        continue;
      }
      const column = this.columnWrites.length;
      this.columnWrites.push(!!d.mutable);
      this.dataIndexOfColumn.push(this.data.length);
      if (!this.columnOf.has(d.cls)) this.columnOf.set(d.cls, column);
      this.data.push({ cls: d.cls, write: !!d.mutable, optional: !!d.optional, column });
    }
    this.marks = marks;
    this.raw = [];
    this.tick = 0;
    this.paramIndex = 0;
    this.accs = null;
    this._tables = null;
    this.row = new Array(this.data.length);
  }
  bind(raw, tick, paramIndex) {
    this.raw = raw;
    this.tick = tick;
    this.paramIndex = paramIndex;
    this._tables = null;
    this._index = null;
    if (!this.accs) this.accs = this.data.map((d) => (d.entity ? null : new (d.write ? d.cls.Write : d.cls.Read)()));
  }
  /** Points the reused accessors at table `t`; returns false for tables whose optional data is absent. */
  enter(t, accs) {
    const tb = this.raw[t];
    for (let i = 0; i < this.data.length; i++) {
      const d = this.data[i];
      if (d.entity) continue;
      const buf = tb[2][d.column];
      const a = accs[i];
      if (!buf) {
        a.present = false;
        continue;
      }
      a.present = true;
      for (const v of d.cls.views) a[v] = new VIEW_CTORS[v](buf);
      a.t = tb[3][d.column] ? new Uint32Array(tb[3][d.column]) : DUMMY_TICKS;
      a.k = this.tick;
    }
  }
  fill(row, accs, ents, r) {
    for (let i = 0; i < this.data.length; i++) {
      const d = this.data[i];
      if (d.entity) row[i] = ents[r * 2] + ents[r * 2 + 1] * 4294967296;
      else {
        const a = accs[i];
        a.r = r;
        row[i] = a.present === false ? undefined : a;
      }
    }
    return row;
  }
  /** Calls `fn(...row)` per match: the entity or a reused accessor per datum. No iterator protocol. */
  each(fn) {
    const accs = this.accs;
    const row = this.row;
    for (let t = 0; t < this.raw.length; t++) {
      const tb = this.raw[t];
      this.enter(t, accs);
      const ents = tb[1];
      const rows = tb[4];
      if (rows) {
        for (let k = 0; k < rows.length; k++) fn(...this.fill(row, accs, ents, rows[k]));
      } else {
        const len = tb[0];
        if (this.data.length === 2 && !this.data[0].entity && !this.data[1].entity) {
          const a0 = accs[0], a1 = accs[1];
          const p0 = a0.present !== false, p1 = a1.present !== false;
          for (let r = 0; r < len; r++) {
            a0.r = r;
            a1.r = r;
            fn(p0 ? a0 : undefined, p1 ? a1 : undefined);
          }
        } else {
          for (let r = 0; r < len; r++) fn(...this.fill(row, accs, ents, r));
        }
      }
    }
  }
  *[Symbol.iterator]() {
    const accs = this.accs;
    for (let t = 0; t < this.raw.length; t++) {
      const tb = this.raw[t];
      this.enter(t, accs);
      const rows = tb[4];
      const n = rows ? rows.length : tb[0];
      for (let k = 0; k < n; k++) yield this.fill(new Array(this.data.length), accs, tb[1], rows ? rows[k] : k);
    }
  }
  /** Number of matches. */
  get len() {
    let n = 0;
    for (const tb of this.raw) n += tb[4] ? tb[4].length : tb[0];
    return n;
  }
  /** The matched tables, for hand-written loops over the typed arrays. */
  get tables() {
    if (!this._tables) this._tables = this.raw.map((_, i) => new Table(this, i));
    return this._tables;
  }
  /** A row with fresh accessors (safe to keep while calling `get` again in this run). */
  rowAt(t, r) {
    const accs = this.data.map((d) => (d.entity ? null : new (d.write ? d.cls.Write : d.cls.Read)()));
    this.enter(t, accs);
    return this.fill(new Array(this.data.length), accs, this.raw[t][1], r);
  }
  /** Exactly one match, or throws. */
  single() {
    if (this.len !== 1) throw new Error(`single(): expected exactly one match, found ${this.len}`);
    for (let t = 0; t < this.raw.length; t++) {
      const tb = this.raw[t];
      const n = tb[4] ? tb[4].length : tb[0];
      if (n) return this.rowAt(t, tb[4] ? tb[4][0] : 0);
    }
  }
  /** The row of `entity`, or `undefined`. */
  get(entity) {
    if (!this._index) {
      this._index = new Map();
      for (let t = 0; t < this.raw.length; t++) {
        const tb = this.raw[t];
        const ents = tb[1];
        const visit = (r) => this._index.set(ents[r * 2] + ents[r * 2 + 1] * 4294967296, [t, r]);
        if (tb[4]) for (const r of tb[4]) visit(r);
        else for (let r = 0; r < tb[0]; r++) visit(r);
      }
    }
    const at = this._index.get(entity);
    return at ? this.rowAt(at[0], at[1]) : undefined;
  }
}

class MessageReaderParam {
  constructor(cls) {
    this.cls = cls;
    this.parts = [];
    this.acc = null;
  }
  bind(frame) {
    // [bufferA, countA, bufferB, countB, stride]
    this.parts = [[frame[0], frame[1]], [frame[2], frame[3]]];
    if (!this.acc) this.acc = new this.cls.Read();
  }
  get len() {
    return this.parts[0][1] + this.parts[1][1];
  }
  each(fn) {
    for (const [buf, n] of this.parts) {
      if (!n) continue;
      bindRow(this.acc, this.cls, buf, null, 0);
      for (let r = 0; r < n; r++) {
        this.acc.r = r;
        fn(this.acc);
      }
    }
  }
  *[Symbol.iterator]() {
    for (const [buf, n] of this.parts) {
      if (!n) continue;
      for (let r = 0; r < n; r++) {
        const a = new this.cls.Read();
        bindRow(a, this.cls, buf, null, 0);
        a.r = r;
        yield a;
      }
    }
  }
}

class MessageWriterParam {
  constructor(cls, ctx) {
    this.cls = cls;
    this.ctx = ctx;
    this.pending = null;
  }
  /** Queues a message: a `Message` value or a plain object of its fields. */
  write(value) {
    this.flush();
    this.ctx.commands.write(value instanceof this.cls ? value : this.cls.of(value));
  }
  /** A writable view on a fresh slot; written when the run ends or at the next `next`/`write`. */
  next() {
    this.flush();
    const bytes = new Uint8Array(this.cls.size);
    const acc = new this.cls.Write();
    bindRow(acc, this.cls, bytes.buffer, null, 0);
    this.pending = bytes;
    return acc;
  }
  flush() {
    if (this.pending) {
      this.ctx.commands.writeBytes(this.cls, this.pending);
      this.pending = null;
    }
  }
}

/**
 * An observer's `On<E>`: the event's fields directly (`explode.radius`, as Rust's `On` derefs to
 * the event), the target as `entity`, and `propagate(on)`.
 */
function makeOn(cls) {
  const o = cls ? new cls.Read() : {};
  Object.defineProperty(o, "entity", { value: undefined, writable: true, enumerable: true });
  Object.defineProperty(o, "stopped", { value: false, writable: true });
  Object.defineProperty(o, "propagate", {
    value(on) {
      o.stopped = !on;
    },
  });
  Object.defineProperty(o, "bind", {
    value(frame) {
      // [ArrayBuffer, entity]
      if (cls) bindRow(o, cls, frame[0], null, 0);
      o.entity = frame[1] === 18446744073709552000 ? undefined : frame[1];
      o.stopped = false;
    },
  });
  return o;
}

// ---------------------------------------------------------------------------------------------
// Exported Rust params: each method is one host call (a fast op), arguments and results as f64
// slots in two scratch arrays, slices passed in place.

const HOST_SLOTS = 512;
const HOST = {
  slots: new Float64Array(HOST_SLOTS),
  result: new Float64Array(HOST_SLOTS),
  EMPTY: new Uint8Array(0),
  /** A slice argument's bytes, in place. */
  bytes(v, name) {
    if (ArrayBuffer.isView(v)) return new Uint8Array(v.buffer, v.byteOffset, v.byteLength);
    if (v instanceof ArrayBuffer) return new Uint8Array(v);
    throw new TypeError(`${name}: expected a typed array`);
  },
  call(id, _n, s0, s1, s2, s3) {
    if (!core) throw new Error("Rust params need the engine");
    const ss = [s0, s1, s2, s3];
    for (let i = 0; i < 4; i++) {
      for (let j = i + 1; j < 4; j++) {
        const a = ss[i], b = ss[j];
        if (a.byteLength && b.byteLength && a.buffer === b.buffer && a.byteOffset < b.byteOffset + b.byteLength && b.byteOffset < a.byteOffset + a.byteLength) {
          throw new TypeError("two slice arguments share memory; pass separate arrays");
        }
      }
    }
    return core.ops.op_bevyts_host(id, HOST.slotBytes, HOST.resultBytes, s0, s1, s2, s3);
  },
  fail(param, method) {
    const msg = core ? core.ops.op_bevyts_host_error() : "";
    throw new Error(msg || `${param}.${method} failed`);
  },
};
HOST.slotBytes = new Uint8Array(HOST.slots.buffer);
HOST.resultBytes = new Uint8Array(HOST.result.buffer);

const PORT_CLASSES = new Map();

/** The class of an exported param, built from the glue Rust sent with the resolution. */
function portClass(res, name) {
  const src = res && res.ports && res.ports[name];
  if (!src) throw new Error(`${name} is not a system param any loaded crate exports`);
  let P = PORT_CLASSES.get(src);
  if (!P) {
    P = new Function("H", src)(HOST);
    PORT_CLASSES.set(src, P);
  }
  return P;
}

/** One built system: its params, flattened frames' consumers, and how to call it. */
function buildSystem(g, s) {
  const ctx = makeCtx();
  const marks = [];
  const consumers = []; // per flat frame: (frame, tick, index) => void
  const make = (p) => {
    switch (p.kind) {
      case "query": {
        const q = new QueryParam(p, marks);
        consumers.push((frame, tick, i) => q.bind(frame, tick, i));
        return q;
      }
      case "res":
      case "resMut": {
        const C = p.cls;
        const acc = new (p.kind === "resMut" ? C.Write : C.Read)();
        consumers.push((frame, tick) => bindRow(acc, C, frame[0], frame[1], tick));
        return acc;
      }
      case "local": {
        const T = accessorPair("Local", cLayout(p.fields));
        const acc = new T.Write();
        consumers.push((frame) => bindRow(acc, T, frame[0], null, 0));
        return acc;
      }
      case "commands":
        consumers.push(() => {});
        return ctx.commands;
      case "messageReader": {
        const r = new MessageReaderParam(p.cls);
        consumers.push((frame) => r.bind(frame));
        return r;
      }
      case "messageWriter": {
        const w = new MessageWriterParam(p.cls, ctx);
        ctx.writers.push(w);
        consumers.push(() => {});
        return w;
      }
      case "state": {
        const St = p.cls;
        const o = { current: undefined };
        consumers.push((frame) => (o.current = frame >= 0 && frame < St.values.length ? St[St.values[frame]] : undefined));
        return o;
      }
      case "nextState": {
        const St = p.cls;
        consumers.push(() => {});
        return {
          set(v) {
            if (!v || v.state !== St.componentName) throw new TypeError(`next.set: not a ${St.name} value`);
            ctx.commands.setState(St, v.value);
          },
        };
      }
      case "on": {
        const o = makeOn(p.cls);
        ctx.on = o;
        consumers.push((frame) => o.bind(frame));
        return o;
      }
      case "bundle": {
        const o = {};
        for (const f of p.fields) o[f.name] = make(f.param);
        return o;
      }
      case "port": {
        const P = portClass(g.res, p.name);
        consumers.push(() => {});
        return new P();
      }
    }
  };
  const args = s.params.map(make);
  if (s.call === "setup") {
    // The setup's holders become the param objects: copy their prototypes' behaviour in.
    s.holders.forEach((h, i) => {
      const a = args[i];
      Object.setPrototypeOf(h, Object.getPrototypeOf(a));
      Object.assign(h, a);
      args[i] = h;
    });
    // Accessor-backed params (res, local) are objects bound in place: rebind consumers to holders.
    let ci = 0;
    s.params.forEach((p, i) => {
      if (p.kind === "res" || p.kind === "resMut" || p.kind === "local") {
        const h = args[i];
        const C = p.kind === "local" ? accessorPair("Local", cLayout(p.fields)) : p.cls;
        const at = ci;
        consumers[at] = (frame, tick) => bindRow(h, C, frame[0], frame[1], tick);
      } else if (p.kind === "query") {
        const h = args[i];
        consumers[ci] = (frame, tick, idx) => h.bind(frame, tick, idx);
      }
      ci += p.kind === "bundle" ? 0 : 1;
    });
  }
  return { s, ctx, marks, consumers, args };
}

function makeCtx() {
  return { commands: new Commands(), writers: [], on: null };
}

// ---------------------------------------------------------------------------------------------
// Commands: a binary arena in Rust memory, overflow kept in JS

const OP_SPAWN = 1, OP_INSERT = 2, OP_DESPAWN = 3, OP_REMOVE = 4, OP_SET_PARENT = 5, OP_SPAWN_NATIVE = 6, OP_PLUGIN = 7;
const OP_WRITE_MESSAGE = 8, OP_TRIGGER = 9, OP_SET_STATE = 10;

class EntityCommands {
  constructor(commands, id) {
    this.commands = commands;
    this.id = id;
  }
  insert(...parts) {
    const els = parts.filter(isElement);
    if (els.length) for (const el of els) this.commands.mount(el, { parent: this.id });
    const comps = parts.filter((p) => !isElement(p));
    if (comps.length) this.commands.op(encodeComponentsOp(OP_INSERT, this.id, comps));
    return this;
  }
  remove(...classes) {
    const w = new Writer();
    w.u8(OP_REMOVE);
    w.entity(this.id);
    w.u16(classes.length);
    for (const C of classes) w.u32(idOf(C));
    this.commands.op(w.bytes());
    return this;
  }
  setParent(parent) {
    const w = new Writer();
    w.u8(OP_SET_PARENT);
    w.entity(this.id);
    if (parent === null || parent === undefined) {
      w.u32(0xffffffff);
      w.u32(0xffffffff);
    } else w.entity(parent);
    this.commands.op(w.bytes());
    return this;
  }
  despawn() {
    const w = new Writer();
    w.u8(OP_DESPAWN);
    w.entity(this.id);
    this.commands.op(w.bytes());
  }
}

function isElement(p) {
  return !!p && typeof p === "object" && "type" in p && "props" in p && !(p.constructor && p.constructor.componentName);
}

class Commands {
  constructor() {
    this.arena = null;
    this.at = 0;
    this.overflow = null;
    this.pool = null;
    this.poolUsed = 0;
    this.kinds = null;
    this.res = null;
  }
  begin(arena, pool, kinds, res) {
    this.arena = new Uint8Array(arena);
    this.at = 0;
    this.overflow = null;
    this.pool = new Float64Array(pool);
    this.poolUsed = 0;
    this.kinds = kinds;
    this.res = res;
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
    if (this.poolUsed >= this.pool.length) throw new Error(`spawn pool exhausted: at most ${this.pool.length} spawns per call`);
    return this.pool[this.poolUsed++];
  }
  /** Spawns an entity with components (`Health.of({ hp: 3 })`, or a class for its defaults), or mounts a prefab element. */
  spawn(...parts) {
    if (parts.length === 1 && isElement(parts[0])) {
      this.mount(parts[0]);
      return undefined;
    }
    const e = this.alloc();
    this.op(encodeComponentsOp(OP_SPAWN, e, parts.filter((p) => !isElement(p))));
    for (const el of parts.filter(isElement)) this.mount(el, { parent: e });
    return e;
  }
  /** Commands for one entity. */
  entity(e) {
    return new EntityCommands(this, e);
  }
  /** Triggers an event; an entity event carries its target, `Explode.of({ entity: e, radius: 3 })`. */
  trigger(event) {
    const E = event && event.constructor;
    if (!E || E[KIND] !== "event") throw new TypeError("trigger: pass an Event or EntityEvent value (Explode.of({ entity, radius: 3 }))");
    const r = this.res && this.res.events[E.componentName];
    if (!r) throw new Error(`${E.componentName} is not a declared event`);
    let target;
    let values = event;
    if (E.decl.entity) {
      target = event.entity;
      if (target === undefined || target === null) throw new TypeError(`${E.name} is an EntityEvent: give its target, ${E.name}.of({ entity, ... })`);
      values = { ...event };
      delete values.entity;
    }
    const bytes = encodeWithLayout(E, r.layout, values);
    const w = new Writer();
    w.u8(OP_TRIGGER);
    w.u32(r.index);
    if (target === undefined) {
      w.u32(0xffffffff);
      w.u32(0xffffffff);
    } else w.entity(target);
    w.u32(bytes.length);
    w.raw(bytes);
    this.op(w.bytes());
  }
  /** Writes a message (`Damage.of({ ... })`). */
  write(message) {
    const M = message && message.constructor;
    if (!M || M[KIND] !== "message") throw new TypeError("write: pass a Message value (Damage.of({ ... }))");
    const r = this.res && this.res.messages[M.componentName];
    if (!r) throw new Error(`${M.componentName} is not a declared message`);
    this.writeBytes(M, encodeWithLayout(M, r.layout, message));
  }
  writeBytes(M, bytes) {
    const r = this.res.messages[M.componentName];
    const w = new Writer();
    w.u8(OP_WRITE_MESSAGE);
    w.u32(r.index);
    w.u32(bytes.length);
    w.raw(bytes);
    this.op(w.bytes());
  }
  setState(St, value) {
    const index = this.res && this.res.states[St.componentName];
    if (index === undefined) throw new Error(`${St.componentName} is not a declared state`);
    const w = new Writer();
    w.u8(OP_SET_STATE);
    w.u32(index);
    w.u32(St.values.indexOf(value));
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
    const g = S.gens.get(S.live);
    this.custom("tsx/mount", { nodes: describe(element, g ? g.tsx : undefined), parent });
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

function idOf(C) {
  if (typeof C !== "function" || !C.componentName) throw new TypeError(`not a component: ${C}`);
  if (C.id < 0 || !C.layout) throw new Error(`${C.componentName} has no native layout to write from TS`);
  return C.id;
}

function encodeComponentsOp(op, e, parts) {
  const w = new Writer();
  w.u8(op);
  w.entity(e);
  w.u16(parts.length);
  for (const part of parts) {
    // A class alone means its defaults.
    const C = typeof part === "function" ? part : part && part.constructor;
    const values = typeof part === "function" ? {} : part;
    if (!C || !C.componentName || (C[KIND] && C[KIND] !== "component")) throw new TypeError(`spawn/insert: ${String(part)} is not a component (Health.of({ hp: 3 }))`);
    const bytes = encodeWithLayout(C, C.layout, values);
    w.u32(idOf(C));
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
      let i = typeof v === "string" ? t.variants.indexOf(v) : v;
      if (i < 0 || i >= t.variants.length) throw new RangeError(`unknown variant ${v}`);
      if (t.repr) i = t.repr.values[i];
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
    if (arr.length < lanes) throw new TypeError(`${path} expects ${t.kind}`);
    for (let i = 0; i < lanes; i++) dv.setFloat32(off + i * 4, arr[i], true);
  } else if (STRUCT_FIELDS[t.kind]) {
    if (!v || typeof v !== "object") throw new TypeError(`${path} expects ${t.kind === "timer" ? "a Timer" : "a Stopwatch"}`);
    for (const f of STRUCT_FIELDS[t.kind]) {
      const x = structPart(v, f);
      if (x !== undefined) encodeScalar(dv, f.type.kind, off + f.offset, x, f.type);
    }
  } else if (t.kind === "array") {
    const el = t.of;
    const w = el.kind === "enum" ? WIDTH[enumView(el)] : LANES[el.kind] ? LANES[el.kind] * 4 : WIDTH[el.kind] * (el.kind === "entity" ? 2 : 1);
    for (let i = 0; i < t.len; i++) encodeField(dv, el, off + i * w, v[i], `${path}[${i}]`);
  } else {
    if (t.kind !== "enum" && t.kind !== "bool" && typeof v !== "number") throw new TypeError(`${path} expects ${t.kind}, got ${typeof v}`);
    encodeScalar(dv, t.kind, off, v, t);
  }
}

/** Native bytes for a value; omitted fields take their declared default. */
function encodeWithLayout(C, layout, values) {
  if (!layout) throw new Error(`${C.componentName} has no layout yet`);
  const out = new Uint8Array(layout.size);
  // A Rust component starts from its Default, so its holes (AmbientLight's colour) keep it.
  if (C.defaults && C.defaults.length === layout.size && C.layout === layout) out.set(C.defaults);
  const dv = new DataView(out.buffer);
  const defaults = (C.decl && C.decl.fields) || [];
  for (const f of layout.fields) {
    let v = values ? values[f.name] : undefined;
    if (v === undefined) v = (defaults.find((d) => d.name === f.name) || {}).default;
    if (v === undefined) continue;
    encodeField(dv, f.type, f.offset, v, `${C.componentName}.${f.name}`);
  }
  if (values) {
    for (const k of Object.keys(values)) {
      if (!layout.fields.some((f) => f.name === k)) throw new TypeError(`${C.componentName} has no field ${k}`);
    }
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// The host interface Rust calls

function commit(gen, resolutionJson) {
  const g = S.gens.get(gen);
  if (!g) throw new Error(`bevy-ts: generation ${gen} is not loaded`);
  const res = JSON.parse(resolutionJson);
  for (const C of g.classes) {
    const kind = C[KIND];
    if (kind === "component" || kind === "resource") resolveClass(C, res.components[C.componentName]);
    else if (kind === "message") resolveClass(C, res.messages[C.componentName]);
    else if (kind === "event") resolveClass(C, res.events[C.componentName]);
  }
  for (const C of NATIVE) resolveClass(C, res.components[C.componentName]);
  g.res = res;
  g.built = g.systems.map((s) => buildSystem(g, s));
  const old = S.live;
  S.live = gen;
  S.resolution = res;
  if (old && old !== gen) S.gens.delete(old);
  if (web) {
    web.dropExcept(gen);
    web.generation = gen;
  }
}

function discard(gen) {
  S.gens.delete(gen);
  if (web) web.dropOnly(gen);
}

/** One frame's socket events, then the due timers; returns the next deadline. Callbacks run
 * outside any system, so a Rust param they call throws. */
function deliver(now, events) {
  return web ? web.deliver(now, events) : Infinity;
}

/** One call per run: frames are one entry per flat param (bundles flattened). */
function runSystem(gen, index, dt, elapsed, tick, frame, frames, arena, pool) {
  const g = S.gens.get(gen);
  const b = g.built[index];
  b.ctx.commands.begin(arena, pool, g.res.commandKinds, g.res);
  b.marks.length = 0;
  for (let i = 0; i < b.consumers.length; i++) b.consumers[i](frames[i], tick, i);
  S.calling = b.ctx;
  try {
    if (b.s.call === "setup") b.s.body();
    else b.s.fn(...b.args);
  } finally {
    S.calling = null;
  }
  for (const w of b.ctx.writers) w.flush();
  const [len, over, used] = b.ctx.commands.end();
  return [len, over, used, b.marks, !!(b.ctx.on && b.ctx.on.stopped)];
}

function callEntry(name, payload, dt, elapsed, tick, frame, arena, pool) {
  const g = S.gens.get(S.live);
  const fn = g && g.entryFns.get(name);
  if (!fn) throw new Error(`no entry named ${name}`);
  const ctx = makeCtx();
  ctx.commands.begin(arena, pool, g.res.commandKinds, g.res);
  // Entries keep the bridge's context shape: `ctx.commands` plus the frame numbers.
  const entryCtx = { commands: ctx.commands, dt, elapsed, tick, frame };
  S.calling = entryCtx;
  let out;
  try {
    out = fn(new Uint8Array(payload), entryCtx);
  } finally {
    S.calling = null;
  }
  if (out === undefined || out === null) out = new Uint8Array(0);
  else if (typeof out === "string") out = encoder.encode(out);
  else if (!(out instanceof Uint8Array)) out = encoder.encode(JSON.stringify(out));
  const [len, over, used] = ctx.commands.end();
  return [out.slice(), len, over, used];
}

globalThis.__bevyts = { begin, end, abort, commit, discard, runSystem, callEntry, deliver, bind: bindModule, declare: declareFn, data: dataFn };

// A binding that changed outside the tsx host call (a signal a system set) asks Rust for one
// through the running call's commands.
setHost({
  sceneComponent,
  requestFlush() {
    const c = S.calling;
    if (!c) return false;
    try {
      c.commands.custom("tsx/flush", "");
      return true;
    } catch {
      return false;
    }
  },
});
