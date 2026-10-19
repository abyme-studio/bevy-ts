// `bevy-ts/jsx-runtime`: the automatic JSX runtime. Elements are plain descriptions; nothing
// here spawns or calls into Rust. Prefabs (plain functions) are called when their element is
// created, so a finished tree holds only entities, component elements and intrinsics.
// `describe` turns a tree into the JSON the Rust reconciler diffs. Self-contained (no imports),
// so it runs under any JS engine and in unit tests.

export const Fragment = Symbol.for("bevy-ts.fragment");

const ELEMENT = Symbol.for("bevy-ts.element");

/** Intrinsics that describe components of the enclosing entity. */
const COMPONENT_INTRINSICS = new Set(["mesh", "material"]);
/** Intrinsics that are entities of their own (bevy_ui); they arrive with the UI tasks. */
const ENTITY_INTRINSICS = new Set(["entity"]);

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

  if (typeof type === "function" && !isComponent(type)) {
    // A prefab: call it now, with children as a prop, and stamp its call site on what it made.
    const before = seq;
    const out = type(children === undefined || (Array.isArray(children) && children.length === 0) ? p : { ...p, children });
    if (out === null || out === undefined || out === false) return null;
    if (!isElement(out)) throw located(`prefab ${typeName(type)} must return one element`, location);
    addCallSite(out, before, location);
    // The outermost prefab names the element for keying; its explicit key wins.
    return { ...out, key: explicitKey !== null ? explicitKey : out.key, keyType: typeName(type), keyLocation: location };
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
    } else result.push({ el, key, type });
  }
  return result;
}

function propValue(v, name, el) {
  if (typeof v === "function") {
    throw located(`<${typeName(el.type)} ${name}={...}>: function props (bindings and handlers) are not supported yet`, el.source);
  }
  if (isElement(v)) throw located(`<${typeName(el.type)} ${name}={...}>: an element is not a prop value`, el.source);
  if (v !== null && typeof v === "object" && !Array.isArray(v) && typeof v.toArray === "function") return v.toArray();
  return v;
}

function plainProps(el) {
  const out = {};
  for (const k in el.props) {
    const v = el.props[k];
    if (v === undefined) continue;
    out[k] = propValue(v, k, el);
  }
  return out;
}

function describeComponent(el) {
  const kind = isComponent(el.type) ? (el.type.source === "ts" ? "ts" : "rust") : "intrinsic";
  const d = { type: typeName(el.type), kind, props: plainProps(el) };
  if (el.source) d.source = el.source;
  if (el.children && (!Array.isArray(el.children) || el.children.length)) {
    throw located(`<${d.type}> takes no children`, el.source);
  }
  return d;
}

function describeEntity(el, key, type) {
  const props = plainProps(el);
  const node = { key, type, components: [], children: [] };
  if (props.name !== undefined) node.name = String(props.name);
  for (const k in props) if (k !== "name") throw located(`<entity> has no prop ${k}; put components inside it`, el.source);
  node.source = [el.source, ...el.chain].filter(Boolean);
  const byType = new Map();
  const childItems = [];
  for (const item of siblings(el)) {
    const c = item.el;
    if (!isElement(c)) {
      throw located(`<entity> children are components and entities, not ${JSON.stringify(c)}`, el.source);
    }
    if (isComponent(c.type) || COMPONENT_INTRINSICS.has(c.type)) {
      const name = typeName(c.type);
      const prev = byType.get(name);
      if (prev) throw located(`${name} is given twice on one entity (first at ${at(prev.source)})`, c.source);
      byType.set(name, c);
      node.components.push(describeComponent(c));
    } else childItems.push(item);
  }
  node.children = describeEntities(childItems);
  return node;
}

function describeEntities(items) {
  const out = [];
  for (const { el, key, type } of keyed(items, "")) {
    if (el.type === "entity" || ENTITY_INTRINSICS.has(el.type)) out.push(describeEntity(el, key, type));
    else if (isComponent(el.type) || COMPONENT_INTRINSICS.has(el.type)) {
      throw located(`<${typeName(el.type)}> is a component element outside an <entity>`, el.source);
    } else if (typeof el.type === "string") {
      throw located(`unknown intrinsic <${el.type}>`, el.source);
    } else throw located(`cannot describe <${typeName(el.type)}>`, el.source);
  }
  return out;
}

/** The scene description of an element tree: its top-level entities, keyed. */
export function describe(root) {
  const items = siblings(isElement(root) ? [root] : root);
  for (const item of items) {
    if (!isElement(item.el)) throw new Error(`a scene must return elements, not ${JSON.stringify(item.el)}`);
  }
  return describeEntities(items);
}
