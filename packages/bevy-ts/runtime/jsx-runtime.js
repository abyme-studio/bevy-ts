// @generated from crates/bevy-ts-v8/js/jsx-runtime.js by scripts/sync-runtime.mjs. Do not edit.
// `bevy-ts/jsx-runtime`: the automatic JSX runtime, the reactive core and the tsx host call.
//
// Elements are plain descriptions; nothing here spawns or calls into Rust. Prefabs (plain
// functions) are called when their element is created, so a finished tree holds only entities,
// component elements, intrinsics and control-flow slots. `describe` turns a tree into the JSON
// the Rust reconciler diffs; function props and children become bindings, carried as ids.
// Bindings run in the `tsx` host call Rust makes after it mounted them, and again only when a
// signal or a component they read changed. There are no hooks: a binding reads
// `ref.get(Health)` directly and the read is the subscription.
//
// Self-contained (no imports), so it runs under any JS engine and in unit tests.

export const Fragment = Symbol.for("bevy-ts.fragment");

const ELEMENT = Symbol.for("bevy-ts.element");

/** Intrinsics that describe components of the enclosing entity. */
const COMPONENT_INTRINSICS = new Set(["mesh", "material", "mesh2d", "material2d", "worldText", "text2d", "bitmapText"]);
/** bevy_ui intrinsics: entities of their own, with one intrinsic component named after the tag. */
const UI_INTRINSICS = new Set([
  "node", "text", "image", "button", "span", "viewport", "input",
  "checkbox", "radioGroup", "radio", "slider", "sliderThumb", "scrollbar", "scrollbarThumb", "tabList", "tab", "listBox", "listItem",
  "menuButton", "menuPopup", "menuItem", "popover", "dialog", "modalDialogBarrier", "dialogDragHandle",
]);
/** Rich text: `<text>`, `<text2d>` and `<span>` hold strings, bindings and `<span>`s. */
const RICH = new Set(["text", "text2d", "span"]);
/** What a `<span>` takes from its parent unless it sets its own: Bevy gives a span no inheritance. */
const INHERITED = ["size", "color", "font", "weight", "italic", "lineHeight", "letterSpacing"];
/** Props every entity-making element takes besides its components. */
const PICKING = /^on[A-Z]/;
/** `on` plus a Bevy 0.20 picking event's name: an entity-local observer (operator ruling 2026-10-20). */
const POINTER_EVENTS = new Set([
  "PointerClick", "PointerPress", "PointerRelease", "PointerOver", "PointerOut", "PointerEnter", "PointerLeave", "PointerMove",
  "PointerCancel", "PointerDragStart", "PointerDrag", "PointerDragEnd", "PointerDragEnter", "PointerDragOver", "PointerDragLeave",
  "PointerDragDrop", "PointerScroll",
]);
/** `<input>`'s own events: Bevy's `TextEditChange` and `KeyboardInput`, and `Submit` (Enter) and `Cancel` (Escape). */
const INPUT_EVENTS = new Set(["TextEditChange", "KeyboardInput", "Submit", "Cancel"]);
/** Events only some elements raise, beside the pointer events every one takes: Bevy triggers each on that widget. */
const VALUE_EVENTS = new Set(["ValueChange"]);
const ELEMENT_EVENTS = {
  input: INPUT_EVENTS,
  button: new Set(["Activate"]),
  menuItem: new Set(["Activate"]),
  checkbox: VALUE_EVENTS, radio: VALUE_EVENTS, slider: VALUE_EVENTS, radioGroup: VALUE_EVENTS, tabList: VALUE_EVENTS, listBox: VALUE_EVENTS,
};
/** A value widget's state prop: controlled (the game's) or `initial*` (the widget keeps its own). */
const STATE_PROPS = { checkbox: "checked", radio: "checked", slider: "value", tabList: "selected" };
/**
 * Every UI intrinsic's but `<span>`'s: its own `FocusGained` and `FocusLost`, not a descendant's;
 * and `MenuEvent` and `RequestClose`, which Bevy sends up to an owner (design 5).
 */
const UI_EVENTS = ["FocusGained", "FocusLost", "MenuEvent", "RequestClose"];

/** Marks `Show` and `For`: control flow, described as slots rather than called as prefabs. */
const CONTROL = Symbol.for("bevy-ts.control");

let seq = 0;

/** A TS location from the dev transform's `{ fileName, lineNumber, columnNumber }`. */
function loc(source) {
  if (!source) return null;
  if (source.file !== undefined) return source;
  return { file: source.fileName, line: source.lineNumber, column: source.columnNumber };
}

function where(l) {
  return l ? ` (${l.file}:${l.line}:${l.column})` : "";
}

/** Another element's location inside a message: the file's name, line and column. */
function at(l) {
  return l ? `${String(l.file).split("/").pop()}:${l.line}:${l.column}` : "unknown location";
}

/** An error located at an element. `bevyTsLocation` carries the location; the message does not. */
function located(message, location) {
  const e = new Error(location ? message : message + where(location));
  if (location) e.bevyTsLocation = location;
  return e;
}

function isElement(v) {
  return v !== null && typeof v === "object" && v[ELEMENT] === true;
}

function isComponent(type) {
  return typeof type === "function" && typeof type.componentName === "string";
}

function typeName(type) {
  if (type === Fragment) return "Fragment";
  if (typeof type === "string") return type;
  if (isComponent(type)) return type.componentName;
  return (type && type.name) || "Anonymous";
}

/** Marks every element created after `since` in a prefab's output with the prefab call site. */
function addCallSite(node, since, site) {
  if (Array.isArray(node)) {
    for (const n of node) addCallSite(n, since, site);
    return;
  }
  if (!isElement(node) || node.seq <= since) return;
  node.chain.push(site);
  addCallSite(node.children, since, site);
}

function create(type, props, key, source, isStatic) {
  // `<For key={(item) => item.id}>`: JSX hands `key` over as the element key; it is a prop here.
  if (typeof type === "function" && type[CONTROL] === "for") {
    const k = key !== undefined ? key : props && props.key;
    if (typeof k === "function") {
      props = { ...props, by: k };
      delete props.key;
      key = undefined;
    }
  }
  const location = loc(source || (props && props.__source));
  const p = {};
  let children = [];
  if (props) {
    for (const k in props) {
      if (k === "children") children = props.children;
      else if (k !== "__source" && k !== "__self" && k !== "key") p[k] = props[k];
    }
    if (key === undefined && props.key !== undefined) key = props.key;
  }
  const explicitKey = key === undefined || key === null ? null : String(key);

  if (isComponent(type) && typeof type.scene === "function") return sceneComponent(type, p, children, explicitKey, location);

  if (typeof type === "function" && !isComponent(type) && !type[CONTROL]) {
    // A prefab: call it now, with children as a prop, and stamp its call site on what it made.
    const before = seq;
    const out = type(children === undefined || (Array.isArray(children) && children.length === 0) ? p : { ...p, children });
    if (out === null || out === undefined || out === false) return null;
    if (!isElement(out)) throw located(`prefab ${typeName(type)} must return one element`, location);
    addCallSite(out, before, location);
    // The outermost prefab names the element for keying; its explicit key wins.
    const expanded = { ...out, key: explicitKey !== null ? explicitKey : out.key, keyType: typeName(type), keyLocation: location };
    // `ref` on a prefab element names the entity it expands to.
    if (p.ref !== undefined && out.props.ref === undefined) expanded.props = { ...out.props, ref: p.ref };
    return expanded;
  }

  return {
    [ELEMENT]: true,
    seq: ++seq,
    type,
    props: p,
    children: children === undefined ? [] : children,
    // `jsxs`: several children written in source, so the top array is not from `.map`.
    staticChildren: isStatic === true,
    key: explicitKey,
    keyType: null,
    source: location,
    keyLocation: location,
    chain: [],
  };
}

/** A component's field names, from whatever the runtime knows of its layout. */
function fieldNames(C) {
  if (Array.isArray(C.fieldNames)) return C.fieldNames;
  if (C.layout && C.layout.fields) return C.layout.fields.map((f) => f.name);
  const schema = C.fields || C.schema;
  if (schema && typeof schema === "object") return Object.keys(schema);
  return [];
}

/**
 * A scene component (a class extending `SceneComponent` with `static scene(props)`): its props
 * split into the component's fields (`team`, `initialTeam`) and its scene's props (the rest);
 * `scene` builds the subtree, and the component is always added to its root entity, so a query
 * for it finds every instance.
 */
function sceneComponent(type, p, children, explicitKey, location) {
  const fields = new Set(fieldNames(type));
  const declared = Array.isArray(type.sceneProps) ? new Set(type.sceneProps) : null;
  if (declared) {
    for (const f of fields) {
      if (declared.has(f)) throw located(`${typeName(type)}: ${f} is both a field and a scene prop; rename one`, location);
    }
  }
  const fieldProps = {};
  const sceneProps = {};
  for (const k in p) {
    const base = k.startsWith("initial") && k.length > 7 ? k[7].toLowerCase() + k.slice(8) : null;
    if (k === "ref") sceneProps.ref = p[k];
    else if (fields.has(k) || (base !== null && fields.has(base))) fieldProps[k] = p[k];
    else sceneProps[k] = p[k];
  }
  const before = seq;
  const hasChildren = !(children === undefined || (Array.isArray(children) && children.length === 0));
  const out = type.scene(hasChildren ? { ...sceneProps, children } : sceneProps);
  if (!isElement(out) || (out.type !== "entity" && !UI_INTRINSICS.has(out.type))) {
    throw located(`${typeName(type)}.scene must return one <entity> (or UI element)`, location);
  }
  addCallSite(out, before, location);
  // The component itself, on the root, with the field props; never expanded again.
  const own = {
    [ELEMENT]: true,
    seq: ++seq,
    type,
    props: fieldProps,
    children: [],
    staticChildren: false,
    key: null,
    keyType: null,
    source: location,
    keyLocation: location,
    chain: [],
    componentOnly: true,
  };
  const kids = out.children === undefined ? [] : out.children;
  const expanded = {
    ...out,
    children: [own].concat(kids),
    staticChildren: true,
    key: explicitKey !== null ? explicitKey : out.key,
    keyType: typeName(type),
    keyLocation: location,
  };
  if (sceneProps.ref !== undefined && out.props.ref === undefined) expanded.props = { ...out.props, ref: sceneProps.ref };
  return expanded;
}

export function jsx(type, props, key) {
  return create(type, props, key, null, false);
}

export function jsxs(type, props, key) {
  return create(type, props, key, null, true);
}

/** For `bevy-ts/jsx-dev-runtime`: the dev transform's static flag and source argument. */
export function jsxWithSource(type, props, key, isStatic, source) {
  return create(type, props, key, source, isStatic);
}

// ---------------------------------------------------------------------------------------------
// Description: the JSON Rust reconciles

const warned = new Set();

function warnOnce(message, location) {
  const k = message + where(location);
  if (warned.has(k)) return;
  warned.add(k);
  console.warn(message + where(location));
}

/**
 * Flattens children into `{ el, key, keyType, location }` items: fragments and arrays inline,
 * null/false/true/undefined dropped. A keyed fragment (or a prefab returning one) prefixes its
 * children's keys with its own.
 */
function flatten(children, out, prefix, inArray) {
  if (children === null || children === undefined || children === false || children === true) return;
  if (Array.isArray(children)) {
    const fromMap = inArray || children !== out.top || !out.isStatic;
    for (const c of children) flatten(c, out, prefix, fromMap);
    return;
  }
  if (!isElement(children)) {
    out.items.push({ el: children, prefix });
    return;
  }
  if (children.type === Fragment) {
    const own = children.key !== null || children.keyType !== null;
    if (!own) {
      const inner = { items: out.items, top: children.children, isStatic: children.staticChildren };
      flatten(children.children, inner, prefix, inArray);
      return;
    }
    out.items.push({ el: children, prefix, group: true, inArray });
    return;
  }
  out.items.push({ el: children, prefix, inArray });
}

/** The flattened children of an element, or of a scene's return value. */
function siblings(el) {
  const children = isElement(el) ? el.children : el;
  const out = { items: [], top: children, isStatic: isElement(el) ? el.staticChildren : true };
  flatten(children, out, "", false);
  return out.items;
}

/** Assigns `key` or `type#n` (n counted per type among siblings) and rejects duplicate keys. */
function keyed(items, prefix) {
  const counts = new Map();
  const seen = new Map();
  const result = [];
  for (const item of items) {
    const el = item.el;
    const type = el.keyType || typeName(el.type);
    let key;
    if (el.key !== null) key = el.key;
    else {
      const n = counts.get(type) || 0;
      counts.set(type, n + 1);
      key = `${type}#${n}`;
      if (item.inArray) warnOnce(`<${type}> in an array has no key; give it key={...} so reordering keeps its state`, el.keyLocation);
    }
    key = prefix + key;
    const prev = seen.get(key);
    if (prev) {
      throw located(`two siblings have the key ${JSON.stringify(key)} (the other at ${at(prev.keyLocation)})`, el.keyLocation);
    }
    seen.set(key, el);
    if (item.group) {
      result.push(...keyed(siblings(el), `${key}/`));
    } else result.push({ el, key, type, rich: item.rich });
  }
  return result;
}

// ---------------------------------------------------------------------------------------------
// Reactive core: signals, computations, owners. Components run once; only computations re-run.

/** Thrown by a read whose value Rust has not delivered yet; the computation re-runs when it is. */
const PENDING = { pending: true, toString: () => "bevy-ts: value not delivered yet" };

let Listener = null;
let CurrentOwner = null;
let batchDepth = 0;
const queue = new Set();
/** Set while elements are being described, so untracked reads there can warn. */
let describing = 0;
let devMode = true;

class Owner {
  constructor(parent) {
    this.parent = parent;
    this.children = new Set();
    this.cleanups = [];
    this.disposed = false;
    if (parent) parent.children.add(this);
  }
  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    for (const c of [...this.children]) c.dispose();
    this.children.clear();
    for (const f of this.cleanups.splice(0)) f();
    if (this.parent) this.parent.children.delete(this);
  }
}

class Source {
  constructor() {
    this.observers = new Set();
  }
  track() {
    if (Listener && !Listener.disposed) {
      Listener.sources.add(this);
      this.observers.add(Listener);
    }
  }
  notify() {
    for (const o of [...this.observers]) queue.add(o);
    if (batchDepth === 0) flush();
  }
}

class Computation extends Owner {
  constructor(fn, onError) {
    super(CurrentOwner);
    this.fn = fn;
    this.onError = onError;
    this.sources = new Set();
    // Reads (`ref.get`) resolve in the generation that made the computation, wherever it re-runs.
    this.ctx = Ctx;
  }
  unsubscribe() {
    for (const s of this.sources) s.observers.delete(this);
    this.sources.clear();
  }
  run() {
    if (this.disposed) return;
    this.unsubscribe();
    for (const c of [...this.children]) c.dispose();
    for (const f of this.cleanups.splice(0)) f();
    const [l, o, x] = [Listener, CurrentOwner, Ctx];
    Listener = this;
    CurrentOwner = this;
    if (this.ctx) Ctx = this.ctx;
    try {
      this.fn();
    } catch (e) {
      if (e !== PENDING) (this.onError || reportError)(e);
    } finally {
      Listener = l;
      CurrentOwner = o;
      Ctx = x;
    }
  }
  dispose() {
    this.unsubscribe();
    queue.delete(this);
    super.dispose();
  }
}

/** An error as it crosses to Rust. */
function errorRecord(e, location) {
  return { message: e && e.message ? `${e.name || "Error"}: ${e.message}` : String(e), location: location || null, stack: e && e.stack ? String(e.stack) : null };
}

function reportError(e) {
  console.error(e && e.stack ? e.stack : String(e));
}

function flush() {
  batchDepth++;
  try {
    let guard = 0;
    while (queue.size) {
      if (++guard > 100000) throw new Error("bevy-ts: a binding or effect keeps re-triggering itself");
      const c = queue.values().next().value;
      queue.delete(c);
      c.run();
    }
  } finally {
    batchDepth--;
  }
}

/** Runs `fn` with signal writes batched: computations re-run once, at the end. */
export function batch(fn) {
  batchDepth++;
  try {
    return fn();
  } finally {
    batchDepth--;
    if (batchDepth === 0) flush();
  }
}

/** Reads inside `fn` do not subscribe the running computation. */
export function untrack(fn) {
  const l = Listener;
  Listener = null;
  try {
    return fn();
  } finally {
    Listener = l;
  }
}

/** Runs `fn` when the current computation re-runs or its owner is disposed. */
export function onCleanup(fn) {
  if (CurrentOwner) CurrentOwner.cleanups.push(fn);
}

const readWarned = new Set();

/** Under dev, a reactive read in a component body or a prop value never updates: say so once. */
function untrackedRead(what) {
  if (!devMode || Listener || describing === 0) return;
  const at = callerLocation();
  const key = `${what}@${at ? `${at.file}:${at.line}` : "?"}`;
  if (readWarned.has(key)) return;
  readWarned.add(key);
  console.warn(`${what} is read outside a binding, so it will never update; wrap it: {() => ...}${where(at)}`);
}

const FRAME = /\(?((?:file|https?|bevy-ts):[^\s()]+?):(\d+):(\d+)\)?\s*$/;

function callerLocation() {
  const lines = String(new Error().stack).split("\n").slice(1);
  for (const line of lines) {
    if (/bevy-ts:|jsx-runtime\.js/.test(line)) continue;
    const m = FRAME.exec(line);
    if (m) return { file: m[1].replace(/\?gen=\d+$/, ""), line: +m[2], column: +m[3] };
  }
  return null;
}

/** A value and its setter: `const [open, setOpen] = signal(false)`. For UI-local state. */
export function signal(value, options) {
  const src = new Source();
  const equals = (options && options.equals) || Object.is;
  const read = () => {
    untrackedRead("a signal");
    src.track();
    return value;
  };
  const write = (next) => {
    const v = typeof next === "function" ? next(value) : next;
    if (equals(v, value)) return value;
    value = v;
    src.notify();
    return value;
  };
  return [read, write];
}

/** Runs `fn` now and again whenever what it read changes. */
export function effect(fn) {
  const c = new Computation(fn);
  // Made while a scene is described: it first runs in the host call, once entities exist.
  if (describing > 0 && Ctx) Ctx.deferred.push(c);
  else c.run();
  return () => c.dispose();
}

/** A derived value, recomputed when what it read changes; readers re-run only when it differs. */
export function computed(fn, options) {
  const equals = (options && options.equals) || Object.is;
  const src = new Source();
  let value;
  let ready = false;
  let error = null;
  const c = new Computation(() => {
    try {
      const v = fn();
      error = null;
      if (!ready || !equals(v, value)) {
        value = v;
        ready = true;
        src.notify();
      }
    } catch (e) {
      error = e;
      src.notify();
    }
  });
  c.run();
  return () => {
    src.track();
    if (error) throw error;
    return value;
  };
}

// ---------------------------------------------------------------------------------------------
// Refs and component reads

/** The context of the generation whose code is running (describe, host call, system run). */
let Ctx = null;

/** A handle to an entity a scene made: `ref={player}` on an element, `player.get(Health)` in a binding. */
class NodeRef {
  constructor(entity) {
    this.src = new Source();
    this.resolved = entity !== undefined && entity !== null ? entity : null;
  }
  /** The entity, once mounted. */
  get entity() {
    this.src.track();
    return this.resolved;
  }
  /** The component's current value; inside a binding, the binding re-runs when it changes. */
  get(C) {
    if (typeof C !== "function" || typeof C.componentName !== "string") {
      throw new TypeError(`ref.get(C): ${String(C)} is not a component`);
    }
    untrackedRead(`ref.get(${C.componentName})`);
    this.src.track();
    if (this.resolved === null) throw PENDING;
    const ctx = Ctx;
    if (!ctx) throw new Error(`ref.get(${C.componentName}) outside a scene, binding or system`);
    if (RowInstance) return RowInstance.site.read(RowInstance, this.resolved, C);
    return ctx.read(this.resolved, C);
  }
  /**
   * Whether the entity carries component `C` (a marker such as `Pressed` included); inside a
   * binding, the binding re-runs when that changes. Rust compares presence each frame, no JS.
   */
  has(C) {
    if (typeof C !== "function" || typeof C.componentName !== "string") {
      throw new TypeError(`ref.has(C): ${String(C)} is not a component`);
    }
    untrackedRead(`ref.has(${C.componentName})`);
    if (RowInstance) throw new Error(`ref.has(${C.componentName}) in a bound component field: bind a UI prop instead, which Rust watches by presence`);
    this.src.track();
    if (this.resolved === null) throw PENDING;
    const ctx = Ctx;
    if (!ctx) throw new Error(`ref.has(${C.componentName}) outside a scene or binding`);
    return ctx.readPresence(this.resolved, C);
  }
}

/** A node ref: unbound until an element takes it as `ref`, or wrapping an `Entity`. */
export function ref(entity) {
  return new NodeRef(entity);
}

/** What `readResource` returns outside tsx code, so `res(R)` stays a system parameter there. */
export const NOT_TSX = Symbol.for("bevy-ts.not-tsx");

/**
 * `res(R)` in a binding, a scene or a site: the resource's current value; inside a binding, the
 * binding re-runs when it changes. Rust finds the resource's entity and watches it as any pair.
 */
export function readResource(R) {
  const ctx = Ctx;
  if (!ctx || (!Listener && !RowInstance && describing === 0)) return NOT_TSX;
  if (typeof R !== "function" || typeof R.componentName !== "string") throw new TypeError(`res(R): ${String(R)} is not a resource`);
  untrackedRead(`res(${R.componentName})`);
  if (RowInstance) throw new SitePairRead();
  return ctx.read(null, R);
}

const LANE_NAMES = ["x", "y", "z", "w"];

/** A delivered component value: plain fields, vectors with `.x`/`.y`/`.z` and `toArray()`. */
function componentValue(C, json, row) {
  if (C && C.fromValue) return C.fromValue(json);
  if (json === null || typeof json !== "object") return json;
  const out = {};
  for (const k of Object.keys(json)) {
    const v = json[k];
    const schema = C.fields || C.schema;
    const kind = schema && schema[k] && schema[k].spec ? schema[k].spec.kind : null;
    if (Array.isArray(v) && (kind === "vec2" || kind === "vec3" || kind === "vec4" || kind === "quat")) {
      const vec = { toArray: () => v.slice() };
      v.forEach((n, i) => (vec[LANE_NAMES[i]] = n));
      out[k] = Object.freeze(vec);
    } else out[k] = v;
  }
  // Row values are fresh per delivery and read once; freezing them costs more than it guards.
  return row ? out : Object.freeze(out);
}

// ---------------------------------------------------------------------------------------------
// The per-generation context: bindings, refs, slots and the pairs Rust watches

let hostFlush = null;

/** Lets the host say how to ask Rust for a host call from outside one (a signal set by a system). */
let hostSceneComponent = null;

export function setHost(host) {
  hostFlush = host && host.requestFlush;
  hostSceneComponent = host && host.sceneComponent;
  if (host && host.dev !== undefined) devMode = host.dev;
}

function requestFlush(ctx) {
  if (!ctx || ctx.inCall || !ctx.hasOutput()) return;
  if (!ctx.flushRequested && hostFlush && hostFlush()) ctx.flushRequested = true;
}

class Pair {
  /** `presence`: a `has(C)` read, whose value is whether the entity carries `C`; Rust names it `?<name>`. */
  constructor(entity, C, presence = false) {
    this.entity = entity;
    this.C = C;
    this.presence = presence;
    this.name = presence ? `?${C.componentName}` : C.componentName;
    this.src = new Source();
    this.has = false;
    this.value = undefined;
    this.watched = false;
  }
}

/** One generation's tsx state. Dies with the generation, as all JS state does on reload. */
export class TsxContext {
  constructor() {
    this.root = new Owner(null);
    this.bindings = new Map();
    this.nextBinding = 1;
    this.refs = new Map();
    this.refIds = new Map();
    this.slots = new Map();
    this.nextSlot = 1;
    this.sites = new Map();
    this.sitesByKey = new Map();
    /** Site instances waiting for a first own value, by entity. */
    this.rowWaiting = new Map();
    /** Work registered since the last host call, started there. */
    this.pendingBindings = [];
    this.pendingSlots = [];
    this.waitingSites = new Set();
    this.handlers = new Map();
    this.nextHandler = 1;
    /** Site reads fixed since the last call, for Rust: `[siteId, [component names]]`. */
    this.siteReads = [];
    this.pairs = new Map();
    this.byName = new Map();
    this.patches = [];
    this.mounts = [];
    this.unmounts = [];
    /** `[slot, keys]`: a `<For>`'s keys in item order, when its entities are out of that order. */
    this.orders = [];
    this.errors = [];
    this.needs = new Set();
    /** Computations made while describing; they first run in the host call, once mounted. */
    this.deferred = [];
    /**
     * Work a slot's mount registered: `waiting` until its mount leaves in an output, then
     * `mounted`, started at the start of the next call, once Rust has mounted its nodes.
     */
    this.waiting = [];
    this.mounted = [];
    this.inCall = false;
    this.flushRequested = false;
  }

  hasOutput() {
    return this.patches.length > 0 || this.mounts.length > 0 || this.unmounts.length > 0 || this.orders.length > 0;
  }

  refId(r) {
    if (!(r instanceof NodeRef)) throw new TypeError("ref={...} takes a ref() from bevy-ts");
    let id = this.refIds.get(r);
    if (id === undefined) {
      id = this.refIds.size + 1;
      this.refIds.set(r, id);
      this.refs.set(id, r);
    }
    return id;
  }

  /** A component of an entity, as last delivered by Rust; tracked by the running computation. */
  read(entity, C) {
    this.byName.set(C.componentName, C);
    const key = `${entity}|${C.componentName}`;
    let p = this.pairs.get(key);
    if (!p) {
      p = new Pair(entity, C);
      this.pairs.set(key, p);
    }
    p.src.track();
    if (!p.has) {
      this.needs.add(p);
      throw PENDING;
    }
    return p.value;
  }

  /** Whether an entity carries `C`, as last delivered; tracked like `read`. Rust compares presence each frame. */
  readPresence(entity, C) {
    const key = `${entity}|?${C.componentName}`;
    let p = this.pairs.get(key);
    if (!p) {
      p = new Pair(entity, C, true);
      this.pairs.set(key, p);
    }
    p.src.track();
    if (!p.has) {
      this.needs.add(p);
      throw PENDING;
    }
    return p.value;
  }

  /** A site row's own component value, cached on the instance; no subscriptions, Rust finds changes. */
  peek(b, C) {
    const name = C.componentName;
    const v = b.own[name];
    if (v !== undefined || name in b.own) return v;
    this.byName.set(name, C);
    this.rowWaiting.set(b.entity, b);
    this.needs.add({ entity: b.entity, C });
    throw PENDING;
  }

  /**
   * A binding on a native-layout field: one instance of a site (the element and prop in
   * source). Instances run without effects; Rust finds the rows whose reads changed.
   */
  siteBind(fn, key, owner, location) {
    let site = this.sitesByKey.get(key);
    if (!site) {
      site = new Site(this, this.sites.size + 1, key, location);
      this.sites.set(site.id, site);
      this.sitesByKey.set(key, site);
    }
    const id = this.nextBinding++;
    const b = { id, fn, owner, location, last: undefined, sent: false, computation: null, site, entity: null, started: false, own: {} };
    this.bindings.set(id, b);
    site.instances.set(id, b);
    return { $b: id, $s: site.id };
  }

  /** One scene component instance, by name or `module#export`: its description, or the error it raised. */
  expand(name, props, at) {
    const C = hostSceneComponent ? hostSceneComponent(name) : null;
    if (!C || typeof C.scene !== "function") return { failure: errorRecord(new Error(`no scene component named ${name}`), at) };
    try {
      const nodes = runScene(this, () => describe(create(C, props || {}, undefined, at, false), this));
      return { node: nodes[0] || null };
    } catch (e) {
      return { failure: errorRecord(e, (e && e.bevyTsLocation) || at) };
    }
  }

  /** A picking handler: Rust observes the event on the entity and delivers it by id. */
  handler(fn, location) {
    const id = this.nextHandler++;
    this.handlers.set(id, { fn, location });
    return id;
  }

  /** A binding: `fn` runs in the host call once its node is mounted, under `owner`. */
  bind(fn, owner, location) {
    const id = this.nextBinding++;
    const b = { id, fn, owner, location, last: undefined, computation: null };
    this.bindings.set(id, b);
    this.pendingBindings.push(b);
    return { $b: id };
  }

  startBinding(b) {
    const ctx = this;
    const prevOwner = CurrentOwner;
    CurrentOwner = b.owner && !b.owner.disposed ? b.owner : this.root;
    try {
      b.computation = new Computation(
        () => {
          const v = plainValue(b.fn());
          if (b.computation && b.computation.sent && sameValue(v, b.last)) return;
          b.last = v;
          if (b.computation) b.computation.sent = true;
          ctx.patches.push([b.id, v === undefined ? null : v]);
          requestFlush(ctx);
        },
        (e) => ctx.error(e, b.location),
      );
    } finally {
      CurrentOwner = prevOwner;
    }
    b.computation.run();
  }

  error(e, location) {
    this.errors.push(errorRecord(e, location));
  }

  /**
   * The `tsx` host call: deliver refs and values, run handlers, start new computations, collect
   * output. `rowBytes` carries site rows as raw component bytes (see `decodeRows`).
   */
  call(payload, callCtx, rowBytes) {
    const prev = Ctx;
    Ctx = this;
    this.inCall = true;
    try {
      batch(() => {
        for (const [id, entity] of payload.refs || []) {
          const r = this.refs.get(id);
          if (r && r.resolved !== entity) {
            r.resolved = entity;
            r.src.notify();
          }
        }
        let delivered = false;
        for (const [entity, name, json] of payload.values || []) {
          const key = `${entity}|${name}`;
          const p = this.pairs.get(key);
          if (p) {
            p.value = p.presence ? json === true : json === null ? undefined : componentValue(p.C, json);
            p.has = true;
            this.needs.delete(p);
            p.src.notify();
          }
          const b = name[0] === "?" ? undefined : this.rowWaiting.get(entity);
          if (b) {
            const C = this.byName.get(name);
            b.own[name] = json === null ? undefined : componentValue(C, json, true);
            this.rowWaiting.delete(entity);
            delivered = true;
          }
        }
        // Site instances that paused for a first value try again.
        if (delivered) for (const site of this.sites.values()) if (site.paused.size) site.resume();
        for (const id of payload.drop || []) {
          const b = this.bindings.get(id);
          if (b && b.computation) b.computation.dispose();
          if (b && b.site) b.site.instances.delete(id);
          this.bindings.delete(id);
        }
        for (const [id, entity] of payload.instances || []) {
          const b = this.bindings.get(id);
          if (!b || !b.site) continue;
          b.entity = entity;
          b.site.waiting.push(b);
          this.waitingSites.add(b.site);
        }
        // Rows whose own reads changed: refresh the cache, re-run those instances only.
        const rerun = new Set();
        for (const [id, values] of payload.rows || []) {
          const b = this.bindings.get(id);
          if (!b || b.entity === null) continue;
          for (const [name, json] of values) b.own[name] = json === null ? undefined : componentValue(this.byName.get(name), json, true);
          rerun.add(b);
        }
        if (rowBytes && rowBytes.byteLength) decodeRows(this, rowBytes, rerun);
        for (const b of rerun) b.site.run(b);
        this.startPending();
        // Picking and input events, in arrival order, with the call's commands and frame.
        for (const [id, ev] of payload.events || []) {
          const h = this.handlers.get(id);
          if (!h) continue;
          const event = { ...ev };
          const hctx = { commands: callCtx ? callCtx.commands : null, frame: callCtx ? callCtx.frame : 0, entity: ev.entity };
          try {
            h.fn(Object.assign(hctx, event), event);
          } catch (e) {
            this.error(e, h.location);
          }
        }
      });
      // Scene component instances a Rust plugin (levels) asks for: described after this call's
      // computations started, so their bindings wait until the plugin has mounted them.
      // Each failed item's own error goes beside it, not among the call's.
      const expanded = (payload.expand || []).map(([name, props, at]) => this.expand(name, props, at));
      const out = this.output();
      if (payload.expand) {
        out.expanded = expanded.map((x) => x.node || null);
        out.failures = expanded.map((x) => x.failure || null);
      }
      return out;
    } finally {
      this.inCall = false;
      this.flushRequested = false;
      Ctx = prev;
    }
  }

  startPending() {
    for (const w of this.mounted.splice(0)) {
      if (w.owner.disposed) {
        // Unmounted (or withdrawn) before it started: its bindings never run.
        for (const b of w.bindings) this.bindings.delete(b.id);
        continue;
      }
      this.pendingBindings.push(...w.bindings);
      this.pendingSlots.push(...w.slots);
      this.deferred.push(...w.deferred);
    }
    for (const b of this.pendingBindings.splice(0)) if (!b.computation && this.bindings.has(b.id)) this.startBinding(b);
    for (const site of this.waitingSites) site.start();
    this.waitingSites.clear();
    for (const s of this.pendingSlots.splice(0)) if (!s.computation) s.start();
    for (const c of this.deferred.splice(0)) c.run();
  }

  output() {
    const subscribe = [];
    const unsubscribe = [];
    for (const [key, p] of this.pairs) {
      const watched = p.src.observers.size > 0;
      if (watched && !p.watched) subscribe.push([p.entity, p.name]);
      if (!watched && p.watched) unsubscribe.push([p.entity, p.name]);
      p.watched = watched;
      if (!watched && !this.needs.has(p)) this.pairs.delete(key);
    }
    // A binding that changed twice since the last call sends its last value only.
    const last = new Map();
    for (const [id, v] of this.patches.splice(0)) {
      last.delete(id);
      last.set(id, v);
    }
    const out = {
      patches: [...last],
      // A mount withdrawn before it left is dropped here, not searched for when withdrawn.
      mounts: this.mounts.splice(0).filter((m) => {
        m.pending = false;
        return !m.withdrawn;
      }),
      unmounts: this.unmounts.splice(0),
      orders: this.orders.splice(0),
      subscribe,
      unsubscribe,
      needs: [...this.needs].map((p) => [p.entity, p.name || p.C.componentName]),
      sites: this.siteReads.splice(0),
      errors: this.errors.splice(0),
    };
    // Slot content leaves now: its bindings start in the call after Rust mounted it.
    this.mounted.push(...this.waiting.splice(0));
    if (this.mounted.some((w) => !w.owner.disposed)) out.more = true;
    this.needs.clear();
    return out;
  }
}

/** Makes `ctx` the running generation's context for the duration of `fn` (describe, a system). */
export function withContext(ctx, fn) {
  const prev = Ctx;
  Ctx = ctx;
  try {
    return fn();
  } finally {
    Ctx = prev;
  }
}

function plainValue(v) {
  if (v !== null && typeof v === "object" && !Array.isArray(v) && typeof v.toArray === "function") return v.toArray();
  return v;
}

function sameValue(a, b) {
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}

// ---------------------------------------------------------------------------------------------
// Sites: bindings on native-layout fields, grouped by element and prop (design decision 11)

/** The site instance being evaluated: `ref.get` on its own entity reads the row cache. */
let RowInstance = null;

class Site {
  constructor(ctx, id, key, location) {
    this.ctx = ctx;
    this.id = id;
    this.key = key;
    this.location = location;
    this.instances = new Map();
    /** Component names the site reads on its own entity; fixed at its first evaluation. */
    this.own = null;
    this.discovered = null;
    /** Re-runs every instance when a signal the site read changes. */
    this.watcher = null;
    /** Instances given their entity, not run yet; and instances paused for a first value. */
    this.waiting = [];
    this.paused = new Set();
  }

  read(b, entity, C) {
    const name = C.componentName;
    if (entity !== b.entity) {
      // Another entity's component: this instance takes the per-pair path (decision 9).
      throw new SitePairRead();
    }
    if (this.own && !this.own.has(name)) {
      throw located(`binding ${this.key} reads ${name}, which it did not read on its first run; read every component it may need on each run, or move the branch into the value`, this.location);
    }
    if (!this.own) this.discovered.add(name);
    return this.ctx.peek(b, C);
  }

  /** Starts instances that have entities and are not running yet. */
  start() {
    if (!this.watcher) {
      const prevOwner = CurrentOwner;
      CurrentOwner = this.ctx.root;
      try {
        this.watcher = new Computation(
          () => {
            // Tracks only what the closures read besides own rows: signals and refs.
            for (const b of this.instances.values()) if (b.started) this.run(b, true);
          },
          (e) => this.ctx.error(e, this.location),
        );
      } finally {
        CurrentOwner = prevOwner;
      }
    }
    const fresh = [];
    for (const b of this.waiting.splice(0)) {
      if (b.started || b.computation || !this.instances.has(b.id)) continue;
      b.started = true;
      fresh.push(b);
    }
    // New instances run under the watcher, so it learns the signals they read, without
    // re-running the instances it already has.
    this.runUnderWatcher(fresh);
  }

  resume() {
    const list = [...this.paused];
    this.paused.clear();
    this.runUnderWatcher(list);
  }

  runUnderWatcher(list) {
    if (!list.length) return;
    const [l, o] = [Listener, CurrentOwner];
    Listener = this.watcher;
    CurrentOwner = this.watcher;
    try {
      for (const b of list) this.run(b, true);
    } finally {
      Listener = l;
      CurrentOwner = o;
    }
  }

  /** Evaluates one instance against its row; patches when the value differs. */
  run(b, tracked) {
    if (b.computation) return;
    const prevRow = RowInstance;
    const prevListener = Listener;
    RowInstance = b;
    if (!tracked) Listener = null;
    if (!this.own) this.discovered = new Set();
    let v;
    try {
      v = plainValue(b.fn());
    } catch (e) {
      if (e instanceof SitePairRead) {
        RowInstance = prevRow;
        Listener = prevListener;
        this.ctx.startBinding(b);
        return;
      }
      if (e === PENDING) {
        this.paused.add(b);
        return;
      }
      this.ctx.error(e, this.location);
      return;
    } finally {
      RowInstance = prevRow;
      Listener = prevListener;
    }
    if (!this.own) {
      this.own = this.discovered;
      this.ctx.siteReads.push([this.id, [...this.own]]);
    }
    if (b.sent && sameValue(v, b.last)) return;
    b.last = v;
    b.sent = true;
    this.ctx.patches.push([b.id, v === undefined ? null : v]);
    requestFlush(this.ctx);
  }
}

class SitePairRead extends Error {}

/** The fields of Rust's `timer_layout()` and `stopwatch_layout()` (layout.rs tests them). */
export const STRUCT_FIELDS = {
  timer: [
    { name: "elapsed", raw: "_elapsed", type: { kind: "f64" }, offset: 0 },
    { name: "duration", raw: "_duration", type: { kind: "f64" }, offset: 8 },
    { name: "times_finished_this_tick", raw: "_times", type: { kind: "u32" }, offset: 16 },
    { name: "mode", raw: "_mode", type: { kind: "enum", variants: ["Once", "Repeating"] }, offset: 20 },
    { name: "paused", raw: "_paused", type: { kind: "bool" }, offset: 21 },
    { name: "finished", raw: "_finished", type: { kind: "bool" }, offset: 22 },
  ],
  stopwatch: [
    { name: "elapsed", raw: "_elapsed", type: { kind: "f64" }, offset: 0 },
    { name: "paused", raw: "_paused", type: { kind: "bool" }, offset: 8 },
  ],
};
const SCALAR_WIDTH = { f32: 4, f64: 8, i8: 1, u8: 1, i16: 2, u16: 2, i32: 4, u32: 4, bool: 1, entity: 8, vec2: 8, vec3: 12, vec4: 16, quat: 16 };

function fieldWidth(t) {
  if (t.kind === "enum") return t.repr ? t.repr.width : t.variants.length <= 256 ? 1 : t.variants.length <= 65536 ? 2 : 4;
  if (t.kind === "array") return fieldWidth(t.of) * t.len;
  if (t.kind === "timer") return 24;
  if (t.kind === "stopwatch") return 16;
  return SCALAR_WIDTH[t.kind];
}

function decodeField(dv, o, t) {
  switch (t.kind) {
    case "f32": return dv.getFloat32(o, true);
    case "f64": return dv.getFloat64(o, true);
    case "i8": return dv.getInt8(o);
    case "u8": return dv.getUint8(o);
    case "i16": return dv.getInt16(o, true);
    case "u16": return dv.getUint16(o, true);
    case "i32": return dv.getInt32(o, true);
    case "u32": return dv.getUint32(o, true);
    case "bool": return dv.getUint8(o) !== 0;
    case "entity": return dv.getUint32(o, true) + dv.getUint32(o + 4, true) * 4294967296;
    case "vec2": case "vec3": case "vec4": case "quat": {
      const n = t.kind === "vec2" ? 2 : t.kind === "vec3" ? 3 : 4;
      const a = [];
      for (let i = 0; i < n; i++) a.push(dv.getFloat32(o + i * 4, true));
      const vec = { toArray: () => a.slice() };
      a.forEach((x, i) => (vec[LANE_NAMES[i]] = x));
      return vec;
    }
    case "enum": {
      const w = fieldWidth(t);
      const raw = w === 1 ? dv.getUint8(o) : w === 2 ? dv.getUint16(o, true) : dv.getUint32(o, true);
      // A Rust enum stores the bytes this build uses per variant (`repr`), read at startup.
      const i = t.repr ? t.repr.values.indexOf(raw) : raw;
      return i >= 0 && i < t.variants.length ? t.variants[i] : raw;
    }
    case "array": {
      const w = fieldWidth(t.of);
      return Array.from({ length: t.len }, (_, i) => decodeField(dv, o + i * w, t.of));
    }
    // A timer or stopwatch: a struct of its fields, as the protocol and the inspector show it.
    case "timer":
    case "stopwatch":
      return Object.fromEntries(STRUCT_FIELDS[t.kind].map((f) => [f.name, decodeField(dv, o + f.offset, f.type)]));
  }
  return undefined;
}

/**
 * Site rows as Rust packs them: u32 sites; per site: u32 id, u16 reads, per read (u16 name
 * length, name, u32 size); u32 rows; per row: u32 instance, then each read's bytes. Values go on
 * the instances; each instance joins `rerun`.
 */
function decodeRows(ctx, bytes, rerun) {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let o = 0;
  const sites = dv.getUint32(o, true);
  o += 4;
  for (let s = 0; s < sites; s++) {
    o += 4;
    const nreads = dv.getUint16(o, true);
    o += 2;
    const reads = [];
    for (let r = 0; r < nreads; r++) {
      const len = dv.getUint16(o, true);
      o += 2;
      let name = "";
      for (let i = 0; i < len; i++) name += String.fromCharCode(bytes[o + i]);
      o += len;
      const size = dv.getUint32(o, true);
      o += 4;
      const C = ctx.byName.get(name);
      reads.push({ name, size, fields: C && C.layout ? C.layout.fields : null });
    }
    const rows = dv.getUint32(o, true);
    o += 4;
    for (let r = 0; r < rows; r++) {
      const b = ctx.bindings.get(dv.getUint32(o, true));
      o += 4;
      for (const read of reads) {
        if (b && read.fields) {
          const v = {};
          for (const f of read.fields) v[f.name] = decodeField(dv, o + f.offset, f.type);
          b.own[read.name] = v;
        }
        o += read.size;
      }
      if (b && b.entity !== null) rerun.add(b);
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Control flow: <Show> and <For>, mounted and unmounted as slots

function control(name, kind) {
  const f = function () {
    throw new Error(`<${name}> is a JSX tag`);
  };
  Object.defineProperty(f, "name", { value: name });
  f[CONTROL] = kind;
  return f;
}

/** `<Show when={() => ...} fallback={...}>`: mounts its children while `when` is truthy. */
export const Show = control("Show", "show");
/** `<For each={() => items()} key={(i) => i.id}>{(item) => ...}</For>`: one subtree per item, by key. */
export const For = control("For", "for");

function evaluate(v) {
  return typeof v === "function" ? v() : v;
}

class Slot {
  /** `parentType`: the element the slot sits in, so its content is checked as that element's children. */
  constructor(ctx, id, el, owner, parentType) {
    this.ctx = ctx;
    this.id = id;
    this.el = el;
    this.owner = owner;
    this.parentType = parentType;
    this.groups = new Map();
    /** A `<For>`'s keys in the order its entities are in: mount order, or the last order sent. */
    this.order = [];
    this.current = undefined;
    this.computation = null;
  }
  start() {
    const prevOwner = CurrentOwner;
    CurrentOwner = this.owner && !this.owner.disposed ? this.owner : this.ctx.root;
    try {
      this.computation = new Computation(
        () => (this.el.type[CONTROL] === "show" ? this.show() : this.for()),
        (e) => this.ctx.error(e, this.el.source),
      );
    } finally {
      CurrentOwner = prevOwner;
    }
    this.computation.run();
  }
  /**
   * Mounts `content` (elements, or a function making them) as group `key` under its own owner,
   * a child of the group the slot sits in, so unmounting that group disposes this one too.
   */
  mount(key, content) {
    const ctx = this.ctx;
    const owner = new Owner(this.owner && !this.owner.disposed ? this.owner : ctx.root);
    const at = [ctx.pendingBindings.length, ctx.pendingSlots.length, ctx.deferred.length];
    let nodes;
    try {
      nodes = untrack(() => runScene(ctx, () => describeIn(ctx, evaluateContent(content), owner, this.parentType), owner));
    } catch (e) {
      // Content that failed to describe mounts nothing: what it registered goes with it.
      for (const b of ctx.pendingBindings.splice(at[0])) ctx.bindings.delete(b.id);
      ctx.pendingSlots.splice(at[1]);
      ctx.deferred.splice(at[2]);
      owner.dispose();
      throw e;
    }
    // What the content registered waits for its mount, not for whichever call comes next.
    const w = { owner, bindings: ctx.pendingBindings.splice(at[0]), slots: ctx.pendingSlots.splice(at[1]), deferred: ctx.deferred.splice(at[2]) };
    if (w.bindings.length || w.slots.length || w.deferred.length) ctx.waiting.push(w);
    owner.mount = [this.id, key, nodes];
    owner.mount.pending = true;
    this.groups.set(key, owner);
    ctx.mounts.push(owner.mount);
    requestFlush(ctx);
  }
  unmount(key) {
    const owner = this.groups.get(key);
    if (!owner) return;
    owner.dispose();
    this.groups.delete(key);
    // A mount not sent yet is withdrawn rather than undone: Rust applies a call's unmounts
    // before its mounts, so then -> else -> then in one batch would leave else mounted.
    if (owner.mount.pending) owner.mount.withdrawn = true;
    else this.ctx.unmounts.push([this.id, key]);
    requestFlush(this.ctx);
  }
  show() {
    const on = !!evaluate(this.el.props.when);
    const key = on ? "then" : "else";
    if (this.current === key) return;
    this.current = key;
    for (const k of [...this.groups.keys()]) this.unmount(k);
    const content = on ? this.el.children : this.el.props.fallback;
    if (content !== undefined && content !== null && content !== false) this.mount(key, content);
  }
  for() {
    const items = evaluate(this.el.props.each) || [];
    const keyOf = this.el.props.by;
    const render = Array.isArray(this.el.children) ? this.el.children[0] : this.el.children;
    if (typeof render !== "function") throw located("<For> takes one child: a function from an item to an element", this.el.source);
    if (typeof keyOf !== "function" && devMode && items.length > 1) {
      warnOnce("<For> without key={(item) => ...} keys items by index, so removing one shifts the rest", this.el.source);
    }
    const seen = new Set();
    const keys = [];
    const added = [];
    items.forEach((item, i) => {
      const k = String(typeof keyOf === "function" ? keyOf(item, i) : i);
      if (seen.has(k)) throw located(`<For> gives two items the key ${JSON.stringify(k)}`, this.el.source);
      seen.add(k);
      keys.push(k);
      if (!this.groups.has(k)) {
        this.mount(k, () => render(item, i));
        if (this.groups.has(k)) added.push(k);
      }
    });
    for (const k of [...this.groups.keys()]) if (!seen.has(k)) this.unmount(k);
    // Kept items stay in their order and new ones are spawned after them: when that is not the
    // items' order, Rust sorts the entities by the keys sent.
    const kept = this.order.filter((k) => seen.has(k) && this.groups.has(k));
    const now = kept.concat(added);
    const live = keys.filter((k) => this.groups.has(k));
    if (now.length !== live.length || now.some((k, i) => k !== live[i])) {
      this.ctx.orders.push([this.id, live]);
      // A reorder mounts nothing: outside a call, it asks for one itself.
      requestFlush(this.ctx);
    }
    this.order = live;
  }
}

function evaluateContent(content) {
  return typeof content === "function" && !isComponent(content) ? content() : content;
}

// ---------------------------------------------------------------------------------------------
// Description: the JSON Rust reconciles

/** Length helpers for `style`: `px(8)` is 8 logical pixels. */
export const px = (n) => `${n}px`;
export const percent = (n) => `${n}%`;
export const vw = (n) => `${n}vw`;
export const vh = (n) => `${n}vh`;
export const em = (n) => `${n}em`;
export const rem = (n) => `${n}rem`;
/** Grid helpers, named after Bevy's grid types; each returns plain data `style` parses. */
export const fr = (n) => `${n}fr`;
export const minmax = (min, max) => ({ minmax: [min, max] });
export const repeat = (count, tracks) => ({ repeat: count, tracks: Array.isArray(tracks) ? tracks : [tracks] });
export const span = (n) => ({ span: n });
/** Degrees to radians, for `transform.rotation`. */
export const deg = (n) => (n * Math.PI) / 180;
/** Gradients for `background` and `border`. Angles are degrees, 0 up and clockwise, as CSS's. */
export const linearGradient = (angle, stops) => ({ gradient: "linear", angle, stops });
export const radialGradient = (stops, options) => ({ gradient: "radial", stops, ...options });
export const conicGradient = (stops, options) => ({ gradient: "conic", stops, ...options });
/** `<image mode>`: Bevy's `NodeImageMode::Sliced` and `Tiled`, in their reflected shape. */
export const sliced = ({ border, center, sides, maxCornerScale = 1 } = {}) => {
  if (border === undefined) throw new Error("sliced takes a border");
  const { left = 0, right = 0, top = 0, bottom = 0 } = typeof border === "number" ? { left: border, right: border, top: border, bottom: border } : border;
  return { Sliced: { border: { min_inset: [left, top], max_inset: [right, bottom] }, center_scale_mode: sliceScale(center), sides_scale_mode: sliceScale(sides), max_corner_scale: maxCornerScale } };
};
/** `SliceScaleMode`: `"stretch"` (the default) or `{ tile: stretchValue }`. */
const sliceScale = (m = "stretch") => (m === "stretch" ? "Stretch" : { Tile: { stretch_value: m.tile } });
export const tiled = ({ x = true, y = true, stretch = 1 } = {}) => ({ Tiled: { tile_x: x, tile_y: y, stretch_value: stretch } });

/** The describe in progress: its context, and the owner bindings belong to. */
let Describing = null;

function propValue(v, name, el, nested) {
  if (typeof v === "function" && nested) {
    throw located(`<${typeName(el.type)} ${name}>: bind the whole prop instead, ${name.split(".")[0]}={() => ({ ... })}`, el.source);
  }
  if (typeof v === "function") {
    if (PICKING.test(name)) {
      throw located(`<${typeName(el.type)} ${name}>: picking props go on an <entity> or a UI element, not a component`, el.source);
    }
    if (!Describing) throw located(`<${typeName(el.type)} ${name}={...}>: a binding needs a scene`, el.source);
    return Describing.ctx.bind(v, Describing.owner, el.source);
  }
  if (isElement(v)) throw located(`<${typeName(el.type)} ${name}={...}>: an element is not a prop value`, el.source);
  if (v instanceof NodeRef) throw located(`<${typeName(el.type)} ${name}={...}>: a ref is not a prop value; read it in a binding`, el.source);
  // A state value (`Game.Playing`): its machine and value by name.
  if (v !== null && typeof v === "object" && v.machine && typeof v.state === "string") return { state: v.state, value: v.value };
  if (Array.isArray(v)) return v.map((x, i) => propValue(x, `${name}[${i}]`, el, true));
  if (v !== null && typeof v === "object") {
    if (typeof v.toArray === "function") return v.toArray();
    const o = {};
    for (const k in v) if (v[k] !== undefined) o[k] = propValue(v[k], `${name}.${k}`, el, true);
    return o;
  }
  return v;
}

function plainProps(el, skip) {
  const out = {};
  for (const k in el.props) {
    const v = el.props[k];
    if (v === undefined || (skip && skip.has(k))) continue;
    out[k] = propValue(v, k, el);
  }
  return out;
}

/** `spans`, for `<text2d>`: where its `<span>`s go, as child entities of its entity. */
function describeComponent(el, spans) {
  const kind = isComponent(el.type) ? (el.type.source === "ts" ? "ts" : "rust") : "intrinsic";
  const native = isComponent(el.type) && (el.type.source === "ts" || el.type.native === true);
  const props = {};
  for (const k in el.props) {
    const v = el.props[k];
    if (v === undefined) continue;
    if (native && typeof v === "function" && Describing) {
      // A binding on a native-layout field runs per site: the element and prop in source.
      const at = el.source;
      const key = `${at ? `${at.file}:${at.line}:${at.column}` : "?"} ${typeName(el.type)}.${k}`;
      props[k] = Describing.ctx.siteBind(v, key, Describing.owner, el.source);
    } else props[k] = propValue(v, k, el);
  }
  const d = { type: typeName(el.type), kind, props };
  if (el.source) d.source = el.source;
  if (el.type === "worldText" || el.type === "bitmapText") {
    const t = worldTextOf(el);
    props.text = typeof t === "function" && Describing ? Describing.ctx.bind(t, Describing.owner, el.source) : t;
  } else if (el.type === "text2d") {
    const rich = richText(el);
    const t = rich.text;
    props.text = typeof t === "function" && Describing ? Describing.ctx.bind(t, Describing.owner, el.source) : t;
    spans.push(...rich.items.map((item) => ({ ...item, rich: true })));
  } else if (el.children && (!Array.isArray(el.children) || el.children.length)) {
    throw located(`<${d.type}> takes no children`, el.source);
  }
  return d;
}

/** `<worldText>`'s and `<bitmapText>`'s children, strings, numbers and bindings, joined as `<text>` joins them; no element. */
function worldTextOf(el) {
  const parts = [];
  const walk = (c) => {
    if (c === null || c === undefined || c === false || c === true) return;
    if (Array.isArray(c)) return c.forEach(walk);
    if (isElement(c)) throw located(`<${el.type}> takes strings, numbers and bindings, not <${typeName(c.type)}>`, c.source || el.source);
    parts.push(c);
  };
  walk(el.children);
  return joined(parts);
}

const ENTITY_PROPS = new Set(["name", "ref"]);

/** Strings, numbers and functions, joined into one `text` prop (a binding when any is one). */
function joined(parts) {
  if (parts.every((p) => typeof p !== "function")) return parts.join("");
  return () => parts.map((p) => (typeof p === "function" ? p() : p)).join("");
}

/**
 * `<text>`'s and `<span>`'s children: the text before the first `<span>` is the element's own;
 * each `<span>`, and each run of text after one, is a child span, as Bevy builds rich text. A
 * child span takes its parent's font and colour props it does not set.
 */
function richText(el) {
  const lead = [];
  const spans = [];
  let run = null;
  const walk = (c) => {
    if (c === null || c === undefined || c === false || c === true) return;
    if (Array.isArray(c)) return c.forEach(walk);
    if (isElement(c)) {
      if (c.type !== "span") throw located(`<${el.type}> takes strings, bindings and <span>, not <${typeName(c.type)}>`, c.source || el.source);
      spans.push(c);
      run = null;
      return;
    }
    if (!spans.length) return void lead.push(c);
    if (!run) {
      run = [];
      spans.push(run);
    }
    run.push(c);
  };
  walk(el.children);
  const items = spans.map((s) => {
    const span = Array.isArray(s) ? create("span", { children: s }, undefined, el.source, true) : s;
    const props = { ...span.props };
    for (const k of INHERITED) if (props[k] === undefined && el.props[k] !== undefined) props[k] = el.props[k];
    return { el: { ...span, props }, inArray: false };
  });
  return { text: joined(lead), items };
}
/**
 * `camera={ref}`: the ref's entity, bound, so Rust writes it once the ref mounts. On `<viewport>`
 * it is the camera shown; on any other UI element it is `UiTargetCamera`, which Bevy reads only on
 * a root UI node.
 */
function cameraProp(el, parentType) {
  const v = el.props.camera;
  if (v === undefined) return undefined;
  if (el.type !== "viewport" && UI_INTRINSICS.has(parentType)) {
    throw located(`<${el.type} camera>: camera goes on a root UI element, not one inside <${parentType}>`, el.source);
  }
  if (v instanceof NodeRef) return Describing.ctx.bind(() => v.entity, Describing.owner, el.source);
  if (typeof v === "number" || v === null) return v;
  throw located(`<${el.type} camera>: takes a ref (or an entity)`, el.source);
}

/** `target={ref}` on `<scrollbar>`: the ref's entity, bound, as `camera`. */
function targetProp(el) {
  const v = el.props.target;
  if (v instanceof NodeRef) return Describing.ctx.bind(() => v.entity, Describing.owner, el.source);
  if (typeof v === "number" || v === null) return v;
  throw located(`<${el.type} target>: takes a ref (or an entity)`, el.source);
}


/**
 * A value widget is uncontrolled, and gets Bevy's self-update, when it has no controlled prop
 * (design 7). Giving both forms fails here, naming the prop. A `<radioGroup>` is settled in Rust
 * from the radios mounted under it, which a `<For>` or a bound child hides from this description.
 */
function selfUpdate(el) {
  if (el.props.selfUpdate !== undefined) throw located(`<${el.type}> has no prop selfUpdate`, el.source);
  const state = STATE_PROPS[el.type];
  if (state) {
    const initial = "initial" + state[0].toUpperCase() + state.slice(1);
    if (el.props[state] !== undefined && el.props[initial] !== undefined) {
      throw located(`<${el.type}> takes ${state} or ${initial}, not both: ${state} is the game's state, ${initial} lets the widget keep its own`, el.source);
    }
    return el.type !== "radio" && el.props[state] === undefined;
  }
  return false;
}

function describeEntity(el, key, type, parentType) {
  const ui = UI_INTRINSICS.has(el.type);
  const node = { key, type, components: [], children: [] };
  if (el.props.name !== undefined) {
    if (typeof el.props.name === "function") throw located("<entity name> takes a string, not a binding", el.source);
    node.name = String(el.props.name);
  }
  if (el.props.ref !== undefined) node.ref = Describing.ctx.refId(el.props.ref);
  const skip = new Set(ENTITY_PROPS);
  for (const k in el.props) {
    const v = el.props[k];
    if (v === undefined || !PICKING.test(k)) continue;
    const event = k.slice(2);
    const own = [...(ELEMENT_EVENTS[el.type] || []), ...(ui && el.type !== "span" ? UI_EVENTS : [])];
    if (!POINTER_EVENTS.has(event) && !own.includes(event)) {
      const known = [...POINTER_EVENTS, ...own];
      throw located(`<${typeName(el.type)} ${k}>: no such event; use ${known.map((e) => `on${e}`).join(", ")}`, el.source);
    }
    if (typeof v !== "function") throw located(`<${typeName(el.type)} ${k}> takes a function`, el.source);
    (node.on || (node.on = {}))[event] = Describing.ctx.handler(v, el.source);
    skip.add(k);
  }
  if (!ui) {
    for (const k in el.props) {
      if (!skip.has(k) && el.props[k] !== undefined) throw located(`<entity> has no prop ${k}; put components inside it`, el.source);
    }
  }
  node.source = [el.source, ...el.chain].filter(Boolean);
  const byType = new Map();
  const childItems = [];
  const rich = RICH.has(el.type) ? richText(el) : null;
  if (ui) {
    if (el.props.camera !== undefined) skip.add("camera");
    if (el.type === "scrollbar" && el.props.target !== undefined) skip.add("target");
    const props = plainProps(el, skip);
    const cam = cameraProp(el, parentType);
    if (cam !== undefined) props.camera = cam;
    if (el.type === "scrollbar" && el.props.target !== undefined) props.target = targetProp(el);
    if (selfUpdate(el)) props.selfUpdate = true;
    if (rich) {
      const t = rich.text;
      props.text = typeof t === "function" ? Describing.ctx.bind(t, Describing.owner, el.source) : t;
    }
    node.components.push({ type: el.type, kind: "intrinsic", props, source: el.source });
    byType.set(el.type, el);
  }
  if (rich) childItems.push(...rich.items);
  for (const item of rich ? [] : siblings(el)) {
    const c = item.el;
    if (!isElement(c)) {
      throw located(`<${typeName(el.type)}> children are components and entities, not ${JSON.stringify(c)}`, el.source);
    }
    if (isComponent(c.type) || COMPONENT_INTRINSICS.has(c.type)) {
      const name = typeName(c.type);
      const prev = byType.get(name);
      if (prev) throw located(`${name} is given twice on one entity (first at ${at(prev.source)})`, c.source);
      byType.set(name, c);
      node.components.push(describeComponent(c, childItems));
    } else childItems.push(item);
  }
  node.children = describeEntities(childItems, el.type);
  return node;
}

function describeEntities(items, parentType) {
  const out = [];
  for (const { el, key, type, rich } of keyed(items, "")) {
    if (el.type === "span" && !rich && !RICH.has(parentType)) throw located("<span> goes inside a <text>, a <text2d> or a <span>", el.source);
    if (el.type === "entity" || UI_INTRINSICS.has(el.type)) out.push(describeEntity(el, key, type, rich ? "text2d" : parentType));
    else if (typeof el.type === "function" && el.type[CONTROL]) {
      const id = Describing.ctx.nextSlot++;
      const slot = new Slot(Describing.ctx, id, el, Describing.owner, parentType);
      Describing.ctx.slots.set(id, slot);
      Describing.ctx.pendingSlots.push(slot);
      out.push({ key, type, slot: id, source: [el.source, ...el.chain].filter(Boolean) });
    } else if (isComponent(el.type) || COMPONENT_INTRINSICS.has(el.type)) {
      throw located(`<${typeName(el.type)}> is a component element outside an <entity>`, el.source);
    } else if (typeof el.type === "string") {
      throw located(`unknown intrinsic <${el.type}>`, el.source);
    } else throw located(`cannot describe <${typeName(el.type)}>`, el.source);
  }
  return out;
}

function describeIn(ctx, root, owner, parentType) {
  const prev = Describing;
  const prevCtx = Ctx;
  Describing = { ctx, owner: owner || ctx.root };
  Ctx = ctx;
  describing++;
  try {
    const items = siblings(isElement(root) ? [root] : root);
    for (const item of items) {
      if (!isElement(item.el)) throw new Error(`a scene must return elements, not ${JSON.stringify(item.el)}`);
    }
    return describeEntities(items, parentType);
  } finally {
    describing--;
    Describing = prev;
    Ctx = prevCtx;
  }
}

/**
 * The scene description of an element tree: its top-level entities, keyed. Bindings, refs and
 * slots register in `ctx` (a throwaway context when none is given, as in unit tests).
 */
export function describe(root, ctx) {
  return describeIn(ctx || new TsxContext(), root, null);
}

/** Runs a scene function in `ctx`, so effects it makes wait for the host call. */
export function runScene(ctx, fn, owner) {
  const prevOwner = CurrentOwner;
  const prevCtx = Ctx;
  CurrentOwner = owner || ctx.root;
  Ctx = ctx;
  describing++;
  try {
    return fn();
  } finally {
    describing--;
    CurrentOwner = prevOwner;
    Ctx = prevCtx;
  }
}
