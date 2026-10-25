// The web globals for the V8 backend: what a network client needs (openspec/changes/script-networking).
//
// Evaluated once per isolate, before core.js and any user module. It installs `EventTarget`,
// `Event`, `CloseEvent`, `MessageEvent`, `DOMException`, `AbortController`/`AbortSignal`,
// `URL`/`URLSearchParams`, the four timer functions, `WebSocket`, `Blob`, `fetch` with `Headers`,
// `Request` and `Response`, and `process` (with `env` and `argv`) (openspec/changes/script-networking-2).
//
// Timers live here in a min-heap keyed by deadline, then creation; sockets and fetches are ids
// that Rust drives through ops. Rust calls `deliver(now, events)` (through core.js) once a frame
// with the socket and fetch events received since the last one; it dispatches them, fires the due
// timers and answers the next deadline, so Rust calls in only when there is something to do. Every
// timer, socket and fetch belongs to the generation that was ambient when it was made; core.js sets
// that generation and drops other generations' timers, sockets and fetches on commit, discard and
// restart (a dropped fetch's promise never settles).
//
// Outside the engine (Node, vitest) this module does nothing.

const core = globalThis.Deno && globalThis.Deno.core && globalThis.Deno.core.ops && globalThis.Deno.core.ops.op_bevyts_now ? globalThis.Deno.core : null;

/** A callback that threw: logged with its stack, never stops the loop that called it. */
function report(e) {
  const c = globalThis.console;
  if (c && c.error) c.error("Uncaught", e);
}

// ---------------------------------------------------------------------------------------------
// UTF-8 and percent-encoding

/** The UTF-8 bytes of a string; a lone surrogate is U+FFFD. */
function utf8Encode(s) {
  const out = [];
  for (let i = 0; i < s.length; i++) {
    let c = s.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdfff) {
      const d = s.charCodeAt(i + 1);
      if (c <= 0xdbff && d >= 0xdc00 && d <= 0xdfff) {
        c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00);
        i++;
      } else c = 0xfffd;
    }
    if (c < 0x80) out.push(c);
    else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
    else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
  }
  return out;
}

/** The byte length of a string's UTF-8. */
function utf8Length(s) {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length && s.charCodeAt(i + 1) >= 0xdc00 && s.charCodeAt(i + 1) <= 0xdfff) {
      n += 4;
      i++;
    } else n += 3;
  }
  return n;
}

/** UTF-8 decode with replacement, as the Encoding standard does. */
function utf8Decode(bytes) {
  const cps = [];
  let need = 0;
  let seen = 0;
  let cp = 0;
  let lower = 0x80;
  let upper = 0xbf;
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i];
    if (need === 0) {
      if (b <= 0x7f) cps.push(b);
      else if (b >= 0xc2 && b <= 0xdf) {
        need = 1;
        cp = b & 0x1f;
      } else if (b >= 0xe0 && b <= 0xef) {
        if (b === 0xe0) lower = 0xa0;
        if (b === 0xed) upper = 0x9f;
        need = 2;
        cp = b & 0xf;
      } else if (b >= 0xf0 && b <= 0xf4) {
        if (b === 0xf0) lower = 0x90;
        if (b === 0xf4) upper = 0x8f;
        need = 3;
        cp = b & 7;
      } else cps.push(0xfffd);
      continue;
    }
    if (b < lower || b > upper) {
      need = seen = cp = 0;
      lower = 0x80;
      upper = 0xbf;
      cps.push(0xfffd);
      i--;
      continue;
    }
    lower = 0x80;
    upper = 0xbf;
    cp = (cp << 6) | (b & 0x3f);
    if (++seen === need) {
      cps.push(cp);
      need = seen = cp = 0;
    }
  }
  if (need) cps.push(0xfffd);
  let out = "";
  for (let i = 0; i < cps.length; i += 4096) out += String.fromCodePoint(...cps.slice(i, i + 4096));
  return out;
}

const HEX = "0123456789ABCDEF";
const pct = (b) => "%" + HEX[b >> 4] + HEX[b & 15];
const isHex = (c) => (c >= 0x30 && c <= 0x39) || (c >= 0x41 && c <= 0x46) || (c >= 0x61 && c <= 0x66);

// The percent-encode sets, as predicates on a code point.
const C0_SET = (c) => c < 0x20 || c > 0x7e;
const FRAGMENT_SET = (c) => C0_SET(c) || c === 0x20 || c === 0x22 || c === 0x3c || c === 0x3e || c === 0x60;
const QUERY_SET = (c) => C0_SET(c) || c === 0x20 || c === 0x22 || c === 0x23 || c === 0x3c || c === 0x3e;
const SPECIAL_QUERY_SET = (c) => QUERY_SET(c) || c === 0x27;
const PATH_SET = (c) => QUERY_SET(c) || c === 0x3f || c === 0x5e || c === 0x60 || c === 0x7b || c === 0x7d;
const USERINFO_SET = (c) => PATH_SET(c) || c === 0x2f || c === 0x3a || c === 0x3b || c === 0x3d || c === 0x40 || (c >= 0x5b && c <= 0x5d) || c === 0x7c;

/** One code point (a string of one), percent-encoded if it is in `set`. */
function encodeCodePoint(c, set) {
  if (!set(c.codePointAt(0))) return c;
  let out = "";
  for (const b of utf8Encode(c)) out += pct(b);
  return out;
}

function encodeString(s, set) {
  let out = "";
  for (const c of s) out += encodeCodePoint(c, set);
  return out;
}

/** The bytes of a string with its `%XX` sequences decoded. */
function percentDecode(s) {
  const bytes = utf8Encode(s);
  const out = [];
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i];
    if (b === 0x25 && i + 2 < bytes.length && isHex(bytes[i + 1]) && isHex(bytes[i + 2])) {
      out.push(parseInt(String.fromCharCode(bytes[i + 1], bytes[i + 2]), 16));
      i += 2;
    } else out.push(b);
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// DOMException

const DOM_CODES = {
  IndexSizeError: 1,
  HierarchyRequestError: 3,
  WrongDocumentError: 4,
  InvalidCharacterError: 5,
  NoModificationAllowedError: 7,
  NotFoundError: 8,
  NotSupportedError: 9,
  InvalidStateError: 11,
  SyntaxError: 12,
  InvalidModificationError: 13,
  NamespaceError: 14,
  InvalidAccessError: 15,
  TypeMismatchError: 17,
  SecurityError: 18,
  NetworkError: 19,
  AbortError: 20,
  URLMismatchError: 21,
  QuotaExceededError: 22,
  TimeoutError: 23,
  InvalidNodeTypeError: 24,
  DataCloneError: 25,
};

// `name` is an own property, and the stack is taken again once it is set, so its first line names it.
class DOMException extends Error {
  constructor(message = "", name = "Error") {
    super(String(message));
    Object.defineProperty(this, "name", { value: String(name), writable: true, configurable: true, enumerable: false });
    if (Error.captureStackTrace) Error.captureStackTrace(this, DOMException);
  }
  get code() {
    return DOM_CODES[this.name] || 0;
  }
}
for (const [name, code] of Object.entries(DOM_CODES)) {
  const key = name.replace(/Error$/, "").replace(/([a-z])([A-Z])/g, "$1_$2").toUpperCase() + "_ERR";
  Object.defineProperty(DOMException, key, { value: code, enumerable: true });
  Object.defineProperty(DOMException.prototype, key, { value: code, enumerable: true });
}

// ---------------------------------------------------------------------------------------------
// Events

const now = () => (core ? core.ops.op_bevyts_now() : 0);
let setEventTarget;
let isDispatching;

class Event {
  #type;
  #bubbles;
  #cancelable;
  #composed;
  #target = null;
  #current = null;
  #phase = 0;
  #canceled = false;
  #stopped = false;
  #stoppedNow = false;
  #passive = false;
  #dispatching = false;
  #time = now();

  constructor(type, init = {}) {
    if (arguments.length === 0) throw new TypeError("Event: the type argument is required");
    this.#type = String(type);
    init = init == null ? {} : init;
    this.#bubbles = !!init.bubbles;
    this.#cancelable = !!init.cancelable;
    this.#composed = !!init.composed;
  }
  get type() {
    return this.#type;
  }
  get target() {
    return this.#target;
  }
  get srcElement() {
    return this.#target;
  }
  get currentTarget() {
    return this.#current;
  }
  get eventPhase() {
    return this.#phase;
  }
  get bubbles() {
    return this.#bubbles;
  }
  get cancelable() {
    return this.#cancelable;
  }
  get composed() {
    return this.#composed;
  }
  get defaultPrevented() {
    return this.#canceled;
  }
  get returnValue() {
    return !this.#canceled;
  }
  set returnValue(v) {
    if (!v) this.preventDefault();
  }
  get cancelBubble() {
    return this.#stopped;
  }
  set cancelBubble(v) {
    if (v) this.#stopped = true;
  }
  get timeStamp() {
    return this.#time;
  }
  get isTrusted() {
    return false;
  }
  composedPath() {
    return this.#current ? [this.#current] : [];
  }
  preventDefault() {
    if (this.#cancelable && !this.#passive) this.#canceled = true;
  }
  stopPropagation() {
    this.#stopped = true;
  }
  stopImmediatePropagation() {
    this.#stopped = this.#stoppedNow = true;
  }

  static {
    // The dispatch loop's access to an event's state.
    setEventTarget = (e, target, current, phase, passive) => {
      if (target !== undefined) e.#target = target;
      e.#current = current;
      e.#phase = phase;
      e.#passive = passive;
      e.#dispatching = phase !== 0;
      if (phase === 0) e.#stopped = e.#stoppedNow = false;
      return e.#stoppedNow;
    };
    isDispatching = (e) => e.#dispatching;
  }
}
for (const [i, k] of ["NONE", "CAPTURING_PHASE", "AT_TARGET", "BUBBLING_PHASE"].entries()) {
  Object.defineProperty(Event, k, { value: i, enumerable: true });
  Object.defineProperty(Event.prototype, k, { value: i, enumerable: true });
}

class CloseEvent extends Event {
  #wasClean;
  #code;
  #reason;
  constructor(type, init = {}) {
    super(type, init);
    init = init == null ? {} : init;
    this.#wasClean = !!init.wasClean;
    this.#code = init.code === undefined ? 0 : Number(init.code) & 0xffff;
    this.#reason = init.reason === undefined ? "" : String(init.reason);
  }
  get wasClean() {
    return this.#wasClean;
  }
  get code() {
    return this.#code;
  }
  get reason() {
    return this.#reason;
  }
}

class MessageEvent extends Event {
  #data;
  #origin;
  #lastEventId;
  #source;
  #ports;
  constructor(type, init = {}) {
    super(type, init);
    init = init == null ? {} : init;
    this.#data = init.data === undefined ? null : init.data;
    this.#origin = init.origin === undefined ? "" : String(init.origin);
    this.#lastEventId = init.lastEventId === undefined ? "" : String(init.lastEventId);
    this.#source = init.source === undefined ? null : init.source;
    this.#ports = Object.freeze([...(init.ports || [])]);
  }
  get data() {
    return this.#data;
  }
  get origin() {
    return this.#origin;
  }
  get lastEventId() {
    return this.#lastEventId;
  }
  get source() {
    return this.#source;
  }
  get ports() {
    return this.#ports;
  }
}

class EventTarget {
  /** Listeners by type: `{ callback, capture, once, passive, removed }`. */
  #listeners = new Map();

  addEventListener(type, callback, options) {
    if (arguments.length < 2) throw new TypeError("addEventListener: two arguments are required");
    if (callback == null) return;
    if (typeof callback !== "function" && typeof callback !== "object") throw new TypeError("addEventListener: the listener is not an object");
    const o = typeof options === "object" && options !== null ? options : { capture: !!options };
    const capture = !!o.capture;
    const signal = o.signal;
    if (signal !== undefined && !(signal instanceof AbortSignal)) throw new TypeError("addEventListener: signal is not an AbortSignal");
    if (signal && signal.aborted) return;
    type = String(type);
    let list = this.#listeners.get(type);
    if (!list) this.#listeners.set(type, (list = []));
    if (list.some((l) => l.callback === callback && l.capture === capture)) return;
    const l = { callback, capture, once: !!o.once, passive: !!o.passive, removed: false };
    list.push(l);
    if (signal) signal.addEventListener("abort", () => this.removeEventListener(type, callback, { capture }), { once: true });
  }

  removeEventListener(type, callback, options) {
    const capture = typeof options === "object" && options !== null ? !!options.capture : !!options;
    const list = this.#listeners.get(String(type));
    if (!list) return;
    const i = list.findIndex((l) => l.callback === callback && l.capture === capture);
    if (i < 0) return;
    list[i].removed = true;
    list.splice(i, 1);
  }

  dispatchEvent(event) {
    if (!(event instanceof Event)) throw new TypeError("dispatchEvent: the argument is not an Event");
    if (isDispatching(event)) throw new DOMException("The event is already being dispatched.", "InvalidStateError");
    const list = this.#listeners.get(event.type);
    setEventTarget(event, this, this, 2, false);
    if (list) {
      for (const l of list.slice()) {
        if (l.removed) continue;
        if (l.once) this.removeEventListener(event.type, l.callback, { capture: l.capture });
        if (setEventTarget(event, undefined, this, 2, l.passive)) break;
        try {
          if (typeof l.callback === "function") l.callback.call(this, event);
          else if (typeof l.callback.handleEvent === "function") l.callback.handleEvent(event);
          else throw new TypeError("the listener has no handleEvent method");
        } catch (e) {
          report(e);
        }
        if (setEventTarget(event, undefined, this, 2, false)) break;
      }
    }
    setEventTarget(event, undefined, null, 0, false);
    return !event.defaultPrevented;
  }
}

/** `onX` properties on a prototype, each a listener added when first set, as the web's are. */
function eventHandlers(proto, names) {
  const slots = new WeakMap();
  for (const name of names) {
    Object.defineProperty(proto, "on" + name, {
      configurable: true,
      enumerable: true,
      get() {
        const s = slots.get(this);
        return (s && s[name] && s[name].value) || null;
      },
      set(value) {
        if (typeof value !== "function" && (typeof value !== "object" || value === null)) value = null;
        let s = slots.get(this);
        if (!s) slots.set(this, (s = {}));
        const slot = s[name];
        if (slot) {
          slot.value = value;
          if (value === null) {
            this.removeEventListener(name, slot.listener);
            delete s[name];
          }
          return;
        }
        if (value === null) return;
        const rec = { value, listener: null };
        rec.listener = function (event) {
          if (typeof rec.value === "function" && rec.value.call(this, event) === false) event.preventDefault();
        };
        s[name] = rec;
        this.addEventListener(name, rec.listener);
      },
    });
  }
}

// ---------------------------------------------------------------------------------------------
// AbortController and AbortSignal

const ILLEGAL = Symbol("illegal");
let signalAbort;

class AbortSignal extends EventTarget {
  #aborted = false;
  #reason = undefined;

  constructor(key) {
    if (key !== ILLEGAL) throw new TypeError("Illegal constructor");
    super();
  }
  get aborted() {
    return this.#aborted;
  }
  get reason() {
    return this.#reason;
  }
  throwIfAborted() {
    if (this.#aborted) throw this.#reason;
  }

  static abort(reason) {
    const s = new AbortSignal(ILLEGAL);
    s.#abort(reason, false);
    return s;
  }

  static timeout(ms) {
    const s = new AbortSignal(ILLEGAL);
    setTimeout(() => s.#abort(new DOMException("The signal timed out.", "TimeoutError"), true), ms);
    return s;
  }

  static any(signals) {
    const s = new AbortSignal(ILLEGAL);
    const list = [...signals];
    for (const src of list) if (!(src instanceof AbortSignal)) throw new TypeError("AbortSignal.any: not an AbortSignal");
    const done = list.find((src) => src.aborted);
    if (done) {
      s.#abort(done.reason, false);
      return s;
    }
    for (const src of list) src.addEventListener("abort", () => s.#abort(src.reason, true), { once: true });
    return s;
  }

  #abort(reason, dispatch) {
    if (this.#aborted) return;
    this.#aborted = true;
    this.#reason = reason === undefined ? new DOMException("This operation was aborted", "AbortError") : reason;
    if (dispatch) this.dispatchEvent(new Event("abort"));
  }

  static {
    signalAbort = (s, reason) => s.#abort(reason, true);
  }
}
eventHandlers(AbortSignal.prototype, ["abort"]);

class AbortController {
  #signal = new AbortSignal(ILLEGAL);
  get signal() {
    return this.#signal;
  }
  abort(reason) {
    signalAbort(this.#signal, reason);
  }
}

// ---------------------------------------------------------------------------------------------
// URL: the WHATWG URL Standard's parser. Hosts that are not ASCII get lowercase, NFKC and
// Punycode, not the full UTS #46 mapping and its checks.

const SPECIAL = { "ftp:": 21, "file:": null, "http:": 80, "https:": 443, "ws:": 80, "wss:": 443 };
const isSpecialScheme = (s) => Object.prototype.hasOwnProperty.call(SPECIAL, s + ":");
const defaultPort = (s) => (isSpecialScheme(s) ? SPECIAL[s + ":"] : null);

const ALPHA = /^[A-Za-z]$/;
const ALNUM_PLUS = /^[A-Za-z0-9+\-.]$/;
const DIGIT = /^[0-9]$/;
const FORBIDDEN_HOST = /[\u0000\u0009\u000a\u000d #/:<>?@[\\\]^|]/;
const FORBIDDEN_DOMAIN = /[\u0000-\u001f #%/:<>?@[\\\]^|\u007f]/;

const isWindowsDriveLetter = (s) => s.length === 2 && ALPHA.test(s[0]) && (s[1] === ":" || s[1] === "|");
const isNormalizedDriveLetter = (s) => s.length === 2 && ALPHA.test(s[0]) && s[1] === ":";
function startsWithDriveLetter(cps, p) {
  if (cps.length - p < 2) return false;
  if (!isWindowsDriveLetter(cps[p] + cps[p + 1])) return false;
  if (cps.length - p === 2) return true;
  const c = cps[p + 2];
  return c === "/" || c === "\\" || c === "?" || c === "#";
}
const isSingleDot = (s) => s === "." || s.toLowerCase() === "%2e";
const isDoubleDot = (s) => {
  s = s.toLowerCase();
  return s === ".." || s === ".%2e" || s === "%2e." || s === "%2e%2e";
};

function newRecord() {
  return { scheme: "", username: "", password: "", host: null, port: null, path: [], query: null, fragment: null };
}
const hasOpaquePath = (u) => typeof u.path === "string";
const includesCredentials = (u) => u.username !== "" || u.password !== "";
const cannotHaveCredentials = (u) => u.host === null || u.host === "" || u.scheme === "file";

function shortenPath(u) {
  if (u.scheme === "file" && u.path.length === 1 && isNormalizedDriveLetter(u.path[0])) return;
  u.path.pop();
}

// Host parsing

function parseIPv4Number(s) {
  if (s === "") return null;
  let radix = 10;
  if (s.length >= 2 && (s.startsWith("0x") || s.startsWith("0X"))) {
    s = s.slice(2);
    radix = 16;
  } else if (s.length >= 2 && s[0] === "0") {
    s = s.slice(1);
    radix = 8;
  }
  if (s === "") return 0;
  const ok = radix === 10 ? /^[0-9]+$/ : radix === 16 ? /^[0-9A-Fa-f]+$/ : /^[0-7]+$/;
  if (!ok.test(s)) return null;
  return parseInt(s, radix);
}

function endsInANumber(s) {
  const parts = s.split(".");
  if (parts[parts.length - 1] === "") {
    if (parts.length === 1) return false;
    parts.pop();
  }
  const last = parts[parts.length - 1];
  if (last !== "" && /^[0-9]+$/.test(last)) return true;
  return parseIPv4Number(last) !== null;
}

function parseIPv4(s) {
  const parts = s.split(".");
  if (parts[parts.length - 1] === "" && parts.length > 1) parts.pop();
  if (parts.length > 4) return null;
  const nums = [];
  for (const p of parts) {
    const n = parseIPv4Number(p);
    if (n === null) return null;
    nums.push(n);
  }
  for (let i = 0; i < nums.length - 1; i++) if (nums[i] > 255) return null;
  if (nums[nums.length - 1] >= 256 ** (5 - nums.length)) return null;
  let ip = nums[nums.length - 1];
  for (let i = 0; i < nums.length - 1; i++) ip += nums[i] * 256 ** (3 - i);
  const out = [];
  for (let i = 0; i < 4; i++) {
    out.unshift(ip % 256);
    ip = Math.floor(ip / 256);
  }
  return out.join(".");
}

function parseIPv6(input) {
  const addr = [0, 0, 0, 0, 0, 0, 0, 0];
  let piece = 0;
  let compress = null;
  let p = 0;
  const cps = Array.from(input);
  const at = (i) => cps[i];
  const hex = (c) => c !== undefined && /^[0-9A-Fa-f]$/.test(c);
  if (at(p) === ":") {
    if (at(p + 1) !== ":") return null;
    p += 2;
    compress = ++piece;
  }
  while (at(p) !== undefined) {
    if (piece === 8) return null;
    if (at(p) === ":") {
      if (compress !== null) return null;
      p++;
      compress = ++piece;
      continue;
    }
    let value = 0;
    let length = 0;
    while (length < 4 && hex(at(p))) {
      value = value * 16 + parseInt(at(p), 16);
      p++;
      length++;
    }
    if (at(p) === ".") {
      if (length === 0) return null;
      p -= length;
      if (piece > 6) return null;
      let seen = 0;
      while (at(p) !== undefined) {
        let v4 = null;
        if (seen > 0) {
          if (at(p) === "." && seen < 4) p++;
          else return null;
        }
        if (!DIGIT.test(at(p) || "")) return null;
        while (DIGIT.test(at(p) || "")) {
          const n = parseInt(at(p), 10);
          if (v4 === null) v4 = n;
          else if (v4 === 0) return null;
          else v4 = v4 * 10 + n;
          if (v4 > 255) return null;
          p++;
        }
        addr[piece] = addr[piece] * 0x100 + v4;
        seen++;
        if (seen === 2 || seen === 4) piece++;
      }
      if (seen !== 4) return null;
      break;
    } else if (at(p) === ":") {
      p++;
      if (at(p) === undefined) return null;
    } else if (at(p) !== undefined) return null;
    addr[piece] = value;
    piece++;
  }
  if (compress !== null) {
    let swaps = piece - compress;
    piece = 7;
    while (piece !== 0 && swaps > 0) {
      const t = addr[compress + swaps - 1];
      addr[compress + swaps - 1] = addr[piece];
      addr[piece] = t;
      piece--;
      swaps--;
    }
  } else if (piece !== 8) return null;
  return addr;
}

function serializeIPv6(addr) {
  // The first longest run of two or more zero pieces becomes `::`.
  let best = -1;
  let bestLen = 1;
  for (let i = 0; i < 8; ) {
    if (addr[i] !== 0) {
      i++;
      continue;
    }
    let j = i;
    while (j < 8 && addr[j] === 0) j++;
    if (j - i > bestLen) {
      best = i;
      bestLen = j - i;
    }
    i = j;
  }
  let out = "";
  let ignore0 = false;
  for (let i = 0; i < 8; i++) {
    if (ignore0 && addr[i] === 0) continue;
    ignore0 = false;
    if (best === i) {
      out += i === 0 ? "::" : ":";
      ignore0 = true;
      continue;
    }
    out += addr[i].toString(16);
    if (i !== 7) out += ":";
  }
  return out;
}

function punycode(label) {
  const cps = Array.from(label, (c) => c.codePointAt(0));
  const digit = (d) => String.fromCharCode(d < 26 ? 97 + d : 22 + d);
  const adapt = (delta, points, first) => {
    delta = first ? Math.floor(delta / 700) : Math.floor(delta / 2);
    delta += Math.floor(delta / points);
    let k = 0;
    while (delta > 455) {
      delta = Math.floor(delta / 35);
      k += 36;
    }
    return k + Math.floor((36 * delta) / (delta + 38));
  };
  let n = 128;
  let delta = 0;
  let bias = 72;
  let out = cps
    .filter((c) => c < 128)
    .map((c) => String.fromCharCode(c))
    .join("");
  const b = out.length;
  let h = b;
  if (b > 0) out += "-";
  while (h < cps.length) {
    let m = Infinity;
    for (const c of cps) if (c >= n && c < m) m = c;
    delta += (m - n) * (h + 1);
    n = m;
    for (const c of cps) {
      if (c < n) delta++;
      if (c === n) {
        let q = delta;
        for (let k = 36; ; k += 36) {
          const t = k <= bias ? 1 : k >= bias + 26 ? 26 : k - bias;
          if (q < t) break;
          out += digit(t + ((q - t) % (36 - t)));
          q = Math.floor((q - t) / (36 - t));
        }
        out += digit(q);
        bias = adapt(delta, h + 1, h === b);
        delta = 0;
        h++;
      }
    }
    delta++;
    n++;
  }
  return out;
}

// Code points UTS #46 maps to nothing.
const IGNORED = /[\u00ad\u034f\u180b-\u180d\u200b\u2060\ufe00-\ufe0f\ufeff]/g;
// Some it disallows: U+FFFD (what an invalid byte decodes to) and the noncharacters.
const DISALLOWED = /[\ufffd\ufdd0-\ufdef\ufffe\uffff]|[\ud83f\ud87f\ud8bf\ud8ff\ud93f\ud97f\ud9bf\ud9ff\uda3f\uda7f\udabf\udaff\udb3f\udb7f\udbbf\udbff][\udffe\udfff]/;

function domainToASCII(domain) {
  let d = domain;
  if (/[^\x00-\x7f]/.test(d)) {
    if (DISALLOWED.test(d)) return null;
    d = d.replace(IGNORED, "");
    try {
      d = d.normalize("NFKC");
    } catch {}
    d = d.toLowerCase().replace(/[\u3002\uff0e\uff61]/g, ".");
    try {
      d = d.normalize("NFC");
    } catch {}
    d = d
      .split(".")
      .map((l) => (/[^\x00-\x7f]/.test(l) ? "xn--" + punycode(l) : l))
      .join(".");
  } else d = d.toLowerCase();
  if (d === "" || FORBIDDEN_DOMAIN.test(d)) return null;
  return d;
}

/** A host serialized, or null on failure. */
function parseHost(input, opaque) {
  if (input.startsWith("[")) {
    if (!input.endsWith("]")) return null;
    const a = parseIPv6(input.slice(1, -1));
    return a ? "[" + serializeIPv6(a) + "]" : null;
  }
  if (opaque) {
    if (FORBIDDEN_HOST.test(input)) return null;
    return encodeString(input, C0_SET);
  }
  const ascii = domainToASCII(utf8Decode(percentDecode(input)));
  if (ascii === null) return null;
  if (endsInANumber(ascii)) return parseIPv4(ascii);
  return ascii;
}

// The basic URL parser

const S_SCHEME_START = 1;
const S_SCHEME = 2;
const S_NO_SCHEME = 3;
const S_SPECIAL_RELATIVE_OR_AUTHORITY = 4;
const S_PATH_OR_AUTHORITY = 5;
const S_RELATIVE = 6;
const S_RELATIVE_SLASH = 7;
const S_SPECIAL_AUTHORITY_SLASHES = 8;
const S_SPECIAL_AUTHORITY_IGNORE_SLASHES = 9;
const S_AUTHORITY = 10;
const S_HOST = 11;
const S_HOSTNAME = 12;
const S_PORT = 13;
const S_FILE = 14;
const S_FILE_SLASH = 15;
const S_FILE_HOST = 16;
const S_PATH_START = 17;
const S_PATH = 18;
const S_OPAQUE_PATH = 19;
const S_QUERY = 20;
const S_FRAGMENT = 21;

const FAILURE = null;

/** The URL record for `input`, or null. With `url` and `override`, a setter's partial parse into `url`. */
function parse(input, base, url, override) {
  if (!url) {
    url = newRecord();
    input = input.replace(/^[\u0000-\u0020]+|[\u0000-\u0020]+$/g, "");
  }
  input = input.replace(/[\t\n\r]/g, "");
  let state = override || S_SCHEME_START;
  let buffer = "";
  let atSignSeen = false;
  let insideBrackets = false;
  let passwordTokenSeen = false;
  const cps = Array.from(input);
  const remainingStarts = (p, s) => cps[p + 1] === s;
  for (let p = 0; ; p++) {
    const c = cps[p];
    const special = isSpecialScheme(url.scheme);
    switch (state) {
      case S_SCHEME_START:
        if (c !== undefined && ALPHA.test(c)) {
          buffer += c.toLowerCase();
          state = S_SCHEME;
        } else if (!override) {
          state = S_NO_SCHEME;
          p--;
        } else return FAILURE;
        break;

      case S_SCHEME:
        if (c !== undefined && ALNUM_PLUS.test(c)) buffer += c.toLowerCase();
        else if (c === ":") {
          if (override) {
            if (isSpecialScheme(url.scheme) !== isSpecialScheme(buffer)) return url;
            if ((includesCredentials(url) || url.port !== null) && buffer === "file") return url;
            if (url.scheme === "file" && url.host === "") return url;
          }
          url.scheme = buffer;
          if (override) {
            if (url.port === defaultPort(url.scheme)) url.port = null;
            return url;
          }
          buffer = "";
          if (url.scheme === "file") state = S_FILE;
          else if (isSpecialScheme(url.scheme) && base && base.scheme === url.scheme) state = S_SPECIAL_RELATIVE_OR_AUTHORITY;
          else if (isSpecialScheme(url.scheme)) state = S_SPECIAL_AUTHORITY_SLASHES;
          else if (remainingStarts(p, "/")) {
            state = S_PATH_OR_AUTHORITY;
            p++;
          } else {
            url.path = "";
            state = S_OPAQUE_PATH;
          }
        } else if (!override) {
          buffer = "";
          state = S_NO_SCHEME;
          p = -1;
        } else return FAILURE;
        break;

      case S_NO_SCHEME:
        if (!base || (hasOpaquePath(base) && c !== "#")) return FAILURE;
        if (hasOpaquePath(base) && c === "#") {
          url.scheme = base.scheme;
          url.path = base.path;
          url.query = base.query;
          url.fragment = "";
          state = S_FRAGMENT;
        } else if (base.scheme !== "file") {
          state = S_RELATIVE;
          p--;
        } else {
          state = S_FILE;
          p--;
        }
        break;

      case S_SPECIAL_RELATIVE_OR_AUTHORITY:
        if (c === "/" && remainingStarts(p, "/")) {
          state = S_SPECIAL_AUTHORITY_IGNORE_SLASHES;
          p++;
        } else {
          state = S_RELATIVE;
          p--;
        }
        break;

      case S_PATH_OR_AUTHORITY:
        if (c === "/") state = S_AUTHORITY;
        else {
          state = S_PATH;
          p--;
        }
        break;

      case S_RELATIVE:
        url.scheme = base.scheme;
        if (c === "/") state = S_RELATIVE_SLASH;
        else if (isSpecialScheme(url.scheme) && c === "\\") state = S_RELATIVE_SLASH;
        else {
          url.username = base.username;
          url.password = base.password;
          url.host = base.host;
          url.port = base.port;
          url.path = base.path.slice();
          url.query = base.query;
          if (c === "?") {
            url.query = "";
            state = S_QUERY;
          } else if (c === "#") {
            url.fragment = "";
            state = S_FRAGMENT;
          } else if (c !== undefined) {
            url.query = null;
            shortenPath(url);
            state = S_PATH;
            p--;
          }
        }
        break;

      case S_RELATIVE_SLASH:
        if (special && (c === "/" || c === "\\")) state = S_SPECIAL_AUTHORITY_IGNORE_SLASHES;
        else if (c === "/") state = S_AUTHORITY;
        else {
          url.username = base.username;
          url.password = base.password;
          url.host = base.host;
          url.port = base.port;
          state = S_PATH;
          p--;
        }
        break;

      case S_SPECIAL_AUTHORITY_SLASHES:
        state = S_SPECIAL_AUTHORITY_IGNORE_SLASHES;
        if (c === "/" && remainingStarts(p, "/")) p++;
        else p--;
        break;

      case S_SPECIAL_AUTHORITY_IGNORE_SLASHES:
        if (c !== "/" && c !== "\\") {
          state = S_AUTHORITY;
          p--;
        }
        break;

      case S_AUTHORITY:
        if (c === "@") {
          if (atSignSeen) buffer = "%40" + buffer;
          atSignSeen = true;
          for (const b of buffer) {
            if (b === ":" && !passwordTokenSeen) {
              passwordTokenSeen = true;
              continue;
            }
            const e = encodeCodePoint(b, USERINFO_SET);
            if (passwordTokenSeen) url.password += e;
            else url.username += e;
          }
          buffer = "";
        } else if (c === undefined || c === "/" || c === "?" || c === "#" || (special && c === "\\")) {
          if (atSignSeen && buffer === "") return FAILURE;
          p -= Array.from(buffer).length + 1;
          buffer = "";
          state = S_HOST;
        } else buffer += c;
        break;

      case S_HOST:
      case S_HOSTNAME:
        if (override && url.scheme === "file") {
          p--;
          state = S_FILE_HOST;
        } else if (c === ":" && !insideBrackets) {
          if (buffer === "") return FAILURE;
          if (override === S_HOSTNAME) return FAILURE;
          const host = parseHost(buffer, !special);
          if (host === null) return FAILURE;
          url.host = host;
          buffer = "";
          state = S_PORT;
        } else if (c === undefined || c === "/" || c === "?" || c === "#" || (special && c === "\\")) {
          p--;
          if (special && buffer === "") return FAILURE;
          if (override && buffer === "" && (includesCredentials(url) || url.port !== null)) return FAILURE;
          const host = parseHost(buffer, !special);
          if (host === null) return FAILURE;
          url.host = host;
          buffer = "";
          state = S_PATH_START;
          if (override) return url;
        } else {
          if (c === "[") insideBrackets = true;
          if (c === "]") insideBrackets = false;
          buffer += c;
        }
        break;

      case S_PORT:
        if (c !== undefined && DIGIT.test(c)) buffer += c;
        else if (c === undefined || c === "/" || c === "?" || c === "#" || (special && c === "\\") || override) {
          if (buffer !== "") {
            const port = parseInt(buffer, 10);
            if (port > 65535) return FAILURE;
            url.port = port === defaultPort(url.scheme) ? null : port;
            buffer = "";
            if (override) return url;
          }
          if (override) return FAILURE;
          state = S_PATH_START;
          p--;
        } else return FAILURE;
        break;

      case S_FILE:
        url.scheme = "file";
        url.host = "";
        if (c === "/" || c === "\\") state = S_FILE_SLASH;
        else if (base && base.scheme === "file") {
          url.host = base.host;
          url.path = base.path.slice();
          url.query = base.query;
          if (c === "?") {
            url.query = "";
            state = S_QUERY;
          } else if (c === "#") {
            url.fragment = "";
            state = S_FRAGMENT;
          } else if (c !== undefined) {
            url.query = null;
            if (!startsWithDriveLetter(cps, p)) shortenPath(url);
            else url.path = [];
            state = S_PATH;
            p--;
          }
        } else {
          state = S_PATH;
          p--;
        }
        break;

      case S_FILE_SLASH:
        if (c === "/" || c === "\\") state = S_FILE_HOST;
        else {
          if (base && base.scheme === "file") {
            url.host = base.host;
            if (!startsWithDriveLetter(cps, p) && base.path.length > 0 && isNormalizedDriveLetter(base.path[0])) url.path.push(base.path[0]);
          }
          state = S_PATH;
          p--;
        }
        break;

      case S_FILE_HOST:
        if (c === undefined || c === "/" || c === "\\" || c === "?" || c === "#") {
          p--;
          if (!override && isWindowsDriveLetter(buffer)) state = S_PATH;
          else if (buffer === "") {
            url.host = "";
            if (override) return url;
            state = S_PATH_START;
          } else {
            let host = parseHost(buffer, !isSpecialScheme(url.scheme));
            if (host === null) return FAILURE;
            if (host === "localhost") host = "";
            url.host = host;
            if (override) return url;
            buffer = "";
            state = S_PATH_START;
          }
        } else buffer += c;
        break;

      case S_PATH_START:
        if (special) {
          state = S_PATH;
          if (c !== "/" && c !== "\\") p--;
        } else if (!override && c === "?") {
          url.query = "";
          state = S_QUERY;
        } else if (!override && c === "#") {
          url.fragment = "";
          state = S_FRAGMENT;
        } else if (c !== undefined) {
          state = S_PATH;
          if (c !== "/") p--;
        } else if (override && url.host === null) url.path.push("");
        break;

      case S_PATH: {
        const slash = c === "/" || (special && c === "\\");
        if (c === undefined || slash || (!override && (c === "?" || c === "#"))) {
          if (isDoubleDot(buffer)) {
            shortenPath(url);
            if (!slash) url.path.push("");
          } else if (isSingleDot(buffer) && !slash) url.path.push("");
          else if (!isSingleDot(buffer)) {
            if (url.scheme === "file" && url.path.length === 0 && isWindowsDriveLetter(buffer)) buffer = buffer[0] + ":";
            url.path.push(buffer);
          }
          buffer = "";
          if (c === "?") {
            url.query = "";
            state = S_QUERY;
          }
          if (c === "#") {
            url.fragment = "";
            state = S_FRAGMENT;
          }
        } else buffer += encodeCodePoint(c, PATH_SET);
        break;
      }

      case S_OPAQUE_PATH:
        if (c === "?") {
          url.query = "";
          state = S_QUERY;
        } else if (c === "#") {
          url.fragment = "";
          state = S_FRAGMENT;
        } else if (c === " ") {
          const next = cps[p + 1];
          url.path += next === "?" || next === "#" ? "%20" : " ";
        } else if (c !== undefined) url.path += encodeCodePoint(c, C0_SET);
        break;

      case S_QUERY:
        if ((!override && c === "#") || c === undefined) {
          url.query += encodeString(buffer, special ? SPECIAL_QUERY_SET : QUERY_SET);
          buffer = "";
          if (c === "#") {
            url.fragment = "";
            state = S_FRAGMENT;
          }
        } else buffer += c;
        break;

      case S_FRAGMENT:
        if (c !== undefined) url.fragment += encodeCodePoint(c, FRAGMENT_SET);
        break;
    }
    if (p >= cps.length) break;
  }
  return url;
}

const serializePath = (u) => (hasOpaquePath(u) ? u.path : u.path.map((s) => "/" + s).join(""));

function serialize(u, excludeFragment) {
  let out = u.scheme + ":";
  if (u.host !== null) {
    out += "//";
    if (includesCredentials(u)) {
      out += u.username;
      if (u.password !== "") out += ":" + u.password;
      out += "@";
    }
    out += u.host;
    if (u.port !== null) out += ":" + u.port;
  } else if (!hasOpaquePath(u) && u.path.length > 1 && u.path[0] === "") out += "/.";
  out += serializePath(u);
  if (u.query !== null) out += "?" + u.query;
  if (!excludeFragment && u.fragment !== null) out += "#" + u.fragment;
  return out;
}

function serializeOrigin(u) {
  if (u.scheme === "blob") {
    const inner = parse(serializePath(u));
    return inner && (inner.scheme === "http" || inner.scheme === "https") ? serializeOrigin(inner) : "null";
  }
  if (!isSpecialScheme(u.scheme) || u.scheme === "file") return "null";
  return u.scheme + "://" + u.host + (u.port !== null ? ":" + u.port : "");
}

// application/x-www-form-urlencoded

function formParse(s) {
  const out = [];
  for (const seq of s.split("&")) {
    if (seq === "") continue;
    const i = seq.indexOf("=");
    const name = i < 0 ? seq : seq.slice(0, i);
    const value = i < 0 ? "" : seq.slice(i + 1);
    out.push([utf8Decode(percentDecode(name.replace(/\+/g, " "))), utf8Decode(percentDecode(value.replace(/\+/g, " ")))]);
  }
  return out;
}

function formEncode(s) {
  let out = "";
  for (const b of utf8Encode(s)) {
    if (b === 0x20) out += "+";
    else if (b === 0x2a || b === 0x2d || b === 0x2e || b === 0x5f || (b >= 0x30 && b <= 0x39) || (b >= 0x41 && b <= 0x5a) || (b >= 0x61 && b <= 0x7a)) out += String.fromCharCode(b);
    else out += pct(b);
  }
  return out;
}

const formSerialize = (list) => list.map(([n, v]) => formEncode(n) + "=" + formEncode(v)).join("&");

/** Converts to a USVString: lone surrogates become U+FFFD. */
const usv = (v) => String(v).replace(/[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/g, "\ufffd");

let linkParams;
let setParamsList;

class URLSearchParams {
  #list = [];
  #url = null;

  constructor(init = "") {
    if (typeof init === "object" && init !== null) {
      if (typeof init[Symbol.iterator] === "function") {
        for (const pair of init) {
          const p = [...pair];
          if (p.length !== 2) throw new TypeError("URLSearchParams: each pair must have exactly two items");
          this.#list.push([usv(p[0]), usv(p[1])]);
        }
      } else {
        for (const k of Reflect.ownKeys(init)) {
          const d = Reflect.getOwnPropertyDescriptor(init, k);
          if (d && d.enumerable && typeof k === "string") this.#list.push([usv(k), usv(init[k])]);
        }
      }
    } else {
      const s = usv(init);
      this.#list = formParse(s.startsWith("?") ? s.slice(1) : s);
    }
  }
  #update() {
    if (!this.#url) return;
    const s = formSerialize(this.#list);
    this.#url.query = s === "" ? null : s;
  }
  get size() {
    return this.#list.length;
  }
  append(name, value) {
    this.#list.push([usv(name), usv(value)]);
    this.#update();
  }
  delete(name, value) {
    name = usv(name);
    value = value === undefined ? undefined : usv(value);
    this.#list = this.#list.filter(([n, v]) => !(n === name && (value === undefined || v === value)));
    this.#update();
  }
  get(name) {
    name = usv(name);
    const e = this.#list.find(([n]) => n === name);
    return e ? e[1] : null;
  }
  getAll(name) {
    name = usv(name);
    return this.#list.filter(([n]) => n === name).map(([, v]) => v);
  }
  has(name, value) {
    name = usv(name);
    value = value === undefined ? undefined : usv(value);
    return this.#list.some(([n, v]) => n === name && (value === undefined || v === value));
  }
  set(name, value) {
    name = usv(name);
    value = usv(value);
    const i = this.#list.findIndex(([n]) => n === name);
    if (i < 0) this.#list.push([name, value]);
    else {
      this.#list[i][1] = value;
      this.#list = this.#list.filter(([n], j) => j <= i || n !== name);
    }
    this.#update();
  }
  sort() {
    // Stable, by code units.
    this.#list = this.#list
      .map((e, i) => [e, i])
      .sort((a, b) => (a[0][0] < b[0][0] ? -1 : a[0][0] > b[0][0] ? 1 : a[1] - b[1]))
      .map(([e]) => e);
    this.#update();
  }
  forEach(callback, thisArg) {
    if (typeof callback !== "function") throw new TypeError("URLSearchParams.forEach: the callback is not a function");
    for (let i = 0; i < this.#list.length; i++) callback.call(thisArg, this.#list[i][1], this.#list[i][0], this);
  }
  *entries() {
    for (let i = 0; i < this.#list.length; i++) yield [this.#list[i][0], this.#list[i][1]];
  }
  *keys() {
    for (const [n] of this.entries()) yield n;
  }
  *values() {
    for (const [, v] of this.entries()) yield v;
  }
  [Symbol.iterator]() {
    return this.entries();
  }
  toString() {
    return formSerialize(this.#list);
  }

  static {
    linkParams = (params, record) => {
      params.#url = record;
    };
    setParamsList = (params, query) => {
      params.#list = query ? formParse(query) : [];
    };
  }
}
Object.defineProperty(URLSearchParams.prototype, Symbol.toStringTag, { value: "URLSearchParams", configurable: true });

class URL {
  #u;
  #params;

  constructor(url, base) {
    let b = null;
    if (base !== undefined) {
      b = parse(usv(base));
      if (!b) throw new TypeError(`Invalid base URL: '${base}'`);
    }
    const u = parse(usv(url), b);
    if (!u) throw new TypeError(`Invalid URL: '${url}'`);
    this.#u = u;
    this.#params = new URLSearchParams(u.query || "");
    linkParams(this.#params, u);
  }

  static canParse(url, base) {
    let b = null;
    if (base !== undefined) {
      b = parse(usv(base));
      if (!b) return false;
    }
    return parse(usv(url), b) !== null;
  }

  static parse(url, base) {
    try {
      return new URL(url, base);
    } catch {
      return null;
    }
  }

  /** A setter's parse, in place: what it set before a failure stays, as the standard has it. */
  #set(value, state) {
    parse(value, null, this.#u, state);
  }

  get href() {
    return serialize(this.#u);
  }
  set href(value) {
    const u = parse(usv(value));
    if (!u) throw new TypeError(`Invalid URL: '${value}'`);
    for (const k of Object.keys(this.#u)) delete this.#u[k];
    Object.assign(this.#u, u);
    setParamsList(this.#params, u.query);
  }
  get origin() {
    return serializeOrigin(this.#u);
  }
  get protocol() {
    return this.#u.scheme + ":";
  }
  set protocol(value) {
    this.#set(usv(value) + ":", S_SCHEME_START);
  }
  get username() {
    return this.#u.username;
  }
  set username(value) {
    if (cannotHaveCredentials(this.#u)) return;
    this.#u.username = encodeString(usv(value), USERINFO_SET);
  }
  get password() {
    return this.#u.password;
  }
  set password(value) {
    if (cannotHaveCredentials(this.#u)) return;
    this.#u.password = encodeString(usv(value), USERINFO_SET);
  }
  get host() {
    const u = this.#u;
    if (u.host === null) return "";
    return u.port === null ? u.host : u.host + ":" + u.port;
  }
  set host(value) {
    if (hasOpaquePath(this.#u)) return;
    this.#set(usv(value), S_HOST);
  }
  get hostname() {
    return this.#u.host === null ? "" : this.#u.host;
  }
  set hostname(value) {
    if (hasOpaquePath(this.#u)) return;
    this.#set(usv(value), S_HOSTNAME);
  }
  get port() {
    return this.#u.port === null ? "" : String(this.#u.port);
  }
  set port(value) {
    if (cannotHaveCredentials(this.#u)) return;
    value = usv(value);
    if (value === "") this.#u.port = null;
    else this.#set(value, S_PORT);
  }
  get pathname() {
    return serializePath(this.#u);
  }
  set pathname(value) {
    if (hasOpaquePath(this.#u)) return;
    this.#u.path = [];
    this.#set(usv(value), S_PATH_START);
  }
  get search() {
    const q = this.#u.query;
    return q === null || q === "" ? "" : "?" + q;
  }
  set search(value) {
    value = usv(value);
    if (value === "") {
      this.#u.query = null;
      setParamsList(this.#params, null);
      return;
    }
    this.#u.query = "";
    this.#set(value.startsWith("?") ? value.slice(1) : value, S_QUERY);
    setParamsList(this.#params, this.#u.query);
  }
  get searchParams() {
    return this.#params;
  }
  get hash() {
    const f = this.#u.fragment;
    return f === null || f === "" ? "" : "#" + f;
  }
  set hash(value) {
    value = usv(value);
    if (value === "") {
      this.#u.fragment = null;
      return;
    }
    this.#u.fragment = "";
    this.#set(value.startsWith("#") ? value.slice(1) : value, S_FRAGMENT);
  }
  toString() {
    return serialize(this.#u);
  }
  toJSON() {
    return serialize(this.#u);
  }
}
Object.defineProperty(URL.prototype, Symbol.toStringTag, { value: "URL", configurable: true });

// ---------------------------------------------------------------------------------------------
// The hooks core.js takes: the ambient generation, the environment, delivery and dropping

const hooks = {
  /** The ambient generation: timers and sockets made now belong to it. */
  generation: 0,
  setEnv,
  setArgv,
  deliver,
  dropExcept: (gen) => drop((g) => g !== gen),
  dropOnly: (gen) => drop((g) => g === gen),
};

/** Runs `f` with `gen` ambient, restoring the one before. */
function withGeneration(gen, f) {
  const before = hooks.generation;
  hooks.generation = gen;
  try {
    f();
  } catch (e) {
    report(e);
  } finally {
    hooks.generation = before;
  }
}

// ---------------------------------------------------------------------------------------------
// Timers: a binary min-heap by deadline, then creation sequence

const heap = [];
/** Live timers by id; a cleared or fired timeout leaves, and is marked `cancelled` in the heap. */
const timers = new Map();
let nextTimerId = 1;
let nextSeq = 1;
let delivering = false;
let reported;

const before = (a, b) => a.deadline < b.deadline || (a.deadline === b.deadline && a.seq < b.seq);

function heapPush(t) {
  heap.push(t);
  let i = heap.length - 1;
  while (i > 0) {
    const parent = (i - 1) >> 1;
    if (!before(heap[i], heap[parent])) break;
    [heap[i], heap[parent]] = [heap[parent], heap[i]];
    i = parent;
  }
}

function siftDown(i) {
  for (;;) {
    const l = 2 * i + 1;
    const r = l + 1;
    let m = i;
    if (l < heap.length && before(heap[l], heap[m])) m = l;
    if (r < heap.length && before(heap[r], heap[m])) m = r;
    if (m === i) return;
    [heap[i], heap[m]] = [heap[m], heap[i]];
    i = m;
  }
}

function heapPop() {
  const top = heap[0];
  const last = heap.pop();
  if (heap.length > 0) {
    heap[0] = last;
    siftDown(0);
  }
  return top;
}

/** The earliest live deadline, dropping cleared timers off the top. */
function earliest() {
  while (heap.length > 0 && heap[0].cancelled) heapPop();
  return heap.length > 0 ? heap[0].deadline : Infinity;
}

/** Tells Rust the earliest deadline when it changed; delivery reports once at its end. */
function reportDeadline() {
  if (delivering) return;
  const d = earliest();
  if (d !== reported) {
    reported = d;
    core.ops.op_bevyts_timer_deadline(d);
  }
}

function schedule(callback, delay, args, repeat) {
  if (typeof callback !== "function") throw new TypeError("The callback must be a function; a string of code is not supported");
  delay = Number(delay);
  if (!(delay > 0)) delay = 0;
  const id = nextTimerId++;
  const t = { id, deadline: core.ops.op_bevyts_now() + delay, seq: nextSeq++, callback, args, interval: repeat ? delay : -1, gen: hooks.generation, cancelled: false };
  timers.set(id, t);
  heapPush(t);
  reportDeadline();
  return id;
}

function setTimeout(callback, delay = 0, ...args) {
  return schedule(callback, delay, args, false);
}

function setInterval(callback, delay = 0, ...args) {
  return schedule(callback, delay, args, true);
}

function clearTimer(id) {
  const t = timers.get(Number(id));
  if (!t) return;
  timers.delete(t.id);
  t.cancelled = true;
  reportDeadline();
}

function clearTimeout(id) {
  clearTimer(id);
}

function clearInterval(id) {
  clearTimer(id);
}

function fire(t) {
  if (t.interval < 0) {
    timers.delete(t.id);
    t.cancelled = true;
  }
  withGeneration(t.gen, () => t.callback.apply(globalThis, t.args));
  // An interval reschedules from the time it fired, once: no catch-up after a long frame.
  if (t.interval >= 0 && !t.cancelled) {
    t.deadline = core.ops.op_bevyts_now() + t.interval;
    t.seq = nextSeq++;
    heapPush(t);
  }
}

// ---------------------------------------------------------------------------------------------
// Blob: bytes in memory, copied when it is made

const encode = (s) => (core ? core.encode(usv(s)) : new Uint8Array(utf8Encode(usv(s))));
const decode = (bytes) => (core ? core.decode(bytes) : utf8Decode(bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf ? bytes.subarray(3) : bytes));

/** A copy of an `ArrayBuffer`'s or a view's bytes. */
function copyBytes(data) {
  if (data instanceof ArrayBuffer) return new Uint8Array(data.slice(0));
  return new Uint8Array(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
}

/** A `type` as the File API keeps it: lower-case, or `""` with a character outside printable ASCII. */
const blobType = (t) => {
  t = t === undefined ? "" : String(t);
  return /^[\x20-\x7e]*$/.test(t) ? t.toLowerCase() : "";
};

let blobBytes;

class Blob {
  #bytes;
  #type;

  constructor(parts = [], options = {}) {
    if (parts === null || (typeof parts !== "object" && typeof parts !== "function") || typeof parts[Symbol.iterator] !== "function") {
      if (parts !== undefined) throw new TypeError("Blob: the parts must be a sequence");
      parts = [];
    }
    options = options == null ? {} : options;
    const chunks = [];
    let size = 0;
    for (const part of parts) {
      let bytes;
      if (part instanceof Blob) bytes = part.#bytes;
      else if (part instanceof ArrayBuffer || ArrayBuffer.isView(part)) bytes = copyBytes(part);
      else bytes = encode(String(part));
      chunks.push(bytes);
      size += bytes.byteLength;
    }
    const all = new Uint8Array(size);
    let at = 0;
    for (const c of chunks) {
      all.set(c, at);
      at += c.byteLength;
    }
    this.#bytes = all;
    this.#type = blobType(options.type);
  }

  get size() {
    return this.#bytes.byteLength;
  }
  get type() {
    return this.#type;
  }

  slice(start, end, contentType) {
    const n = this.#bytes.byteLength;
    const at = (v, d) => {
      if (v === undefined) return d;
      v = Math.trunc(Number(v)) || 0;
      return v < 0 ? Math.max(n + v, 0) : Math.min(v, n);
    };
    const from = at(start, 0);
    const to = Math.max(at(end, n), from);
    const b = new Blob([], { type: contentType });
    b.#bytes = this.#bytes.slice(from, to);
    return b;
  }

  text() {
    return Promise.resolve(decode(this.#bytes));
  }
  arrayBuffer() {
    return Promise.resolve(this.#bytes.slice().buffer);
  }
  bytes() {
    return Promise.resolve(this.#bytes.slice());
  }

  static {
    blobBytes = (b) => b.#bytes;
  }
}
Object.defineProperty(Blob.prototype, Symbol.toStringTag, { value: "Blob", configurable: true });

// ---------------------------------------------------------------------------------------------
// WebSocket

const TOKEN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
/** Sockets Rust knows, by id; one leaves when its `close` is delivered or its generation drops. */
const sockets = new Map();
let nextSocketId = 1;
let socketEvent;
let socketGen;
let socketForget;

class WebSocket extends EventTarget {
  #id;
  #gen;
  #url;
  #origin;
  #state = 0;
  #protocol = "";
  #extensions = "";
  #buffered = 0;
  #binaryType = "arraybuffer";

  constructor(url, protocols = []) {
    super();
    if (arguments.length === 0) throw new TypeError("WebSocket: the url argument is required");
    let u;
    try {
      u = new URL(url);
    } catch {
      throw new DOMException(`The URL '${url}' is invalid.`, "SyntaxError");
    }
    if (u.protocol === "http:") u.protocol = "ws:";
    else if (u.protocol === "https:") u.protocol = "wss:";
    if (u.protocol !== "ws:" && u.protocol !== "wss:") throw new DOMException(`The URL's scheme must be 'ws' or 'wss'; '${u.protocol.slice(0, -1)}' is not allowed.`, "SyntaxError");
    if (u.href.includes("#")) throw new DOMException(`The URL contains a fragment identifier ('${u.hash}'); fragments are not allowed in WebSocket URLs.`, "SyntaxError");

    // Non-standard, as Deno's: `{ protocols, headers }` in place of the protocols.
    let headers = [];
    if (typeof protocols === "object" && protocols !== null && !Array.isArray(protocols) && typeof protocols[Symbol.iterator] !== "function") {
      headers = headerPairs(protocols.headers);
      protocols = protocols.protocols === undefined ? [] : protocols.protocols;
    }
    const list = typeof protocols === "string" ? [protocols] : [...protocols].map(String);
    for (const p of list) if (!TOKEN.test(p)) throw new DOMException(`The subprotocol '${p}' is invalid.`, "SyntaxError");
    if (new Set(list).size !== list.length) throw new DOMException("The subprotocols contain a duplicate.", "SyntaxError");

    this.#url = u.href;
    this.#origin = u.origin;
    this.#id = nextSocketId++;
    this.#gen = hooks.generation;
    sockets.set(this.#id, this);
    core.ops.op_bevyts_ws_open(this.#id, this.#gen, this.#url, JSON.stringify(list), JSON.stringify(headers));
  }

  get url() {
    return this.#url;
  }
  get readyState() {
    return this.#state;
  }
  get protocol() {
    return this.#protocol;
  }
  get extensions() {
    return this.#extensions;
  }
  get bufferedAmount() {
    return this.#buffered;
  }
  /** `"arraybuffer"` by default, unlike the web's `"blob"`, so stage 1's scripts keep working. */
  get binaryType() {
    return this.#binaryType;
  }
  set binaryType(value) {
    if (value === "blob" || value === "arraybuffer") this.#binaryType = value;
  }

  send(data) {
    if (arguments.length === 0) throw new TypeError("WebSocket.send: the data argument is required");
    if (this.#state === 0) throw new DOMException("Still in CONNECTING state.", "InvalidStateError");
    if (this.#state !== 1) return;
    if (data instanceof ArrayBuffer || ArrayBuffer.isView(data) || data instanceof Blob) {
      const bytes = data instanceof Blob ? blobBytes(data) : copyBytes(data);
      this.#buffered += bytes.byteLength;
      core.ops.op_bevyts_ws_send_binary(this.#id, this.#gen, bytes);
    } else {
      const s = usv(data);
      this.#buffered += utf8Length(s);
      core.ops.op_bevyts_ws_send_text(this.#id, this.#gen, s);
    }
  }

  close(code, reason) {
    if (code !== undefined) {
      code = Number(code);
      code = Number.isNaN(code) ? 0 : Math.min(Math.max(Math.round(code), 0), 0xffff);
      if (code !== 1000 && !(code >= 3000 && code <= 4999)) throw new DOMException(`The close code must be either 1000, or between 3000 and 4999. ${code} is neither.`, "InvalidAccessError");
    }
    if (reason !== undefined) {
      reason = usv(reason);
      if (utf8Length(reason) > 123) throw new DOMException("The close reason must not be greater than 123 UTF-8 bytes.", "SyntaxError");
    }
    if (this.#state >= 2) return;
    this.#state = 2;
    core.ops.op_bevyts_ws_close(this.#id, this.#gen, code === undefined ? -1 : code, reason === undefined ? "" : reason);
  }

  static {
    socketEvent = (ws, ev) => {
      switch (ev[0]) {
        case 0:
          if (ws.#state !== 0) return;
          ws.#state = 1;
          ws.#protocol = String(ev[2] || "");
          ws.#extensions = String(ev[3] || "");
          ws.dispatchEvent(new Event("open"));
          return;
        case 1:
        case 2:
          if (ws.#state !== 1) return;
          ws.dispatchEvent(new MessageEvent("message", { data: ev[0] === 2 && ws.#binaryType === "blob" ? new Blob([ev[2]]) : ev[2], origin: ws.#origin }));
          return;
        case 3:
          ws.dispatchEvent(new Event("error"));
          return;
        case 4:
          ws.#state = 3;
          sockets.delete(ws.#id);
          ws.dispatchEvent(new CloseEvent("close", { wasClean: !!ev[4], code: ev[2], reason: ev[3] }));
          return;
        case 5:
          ws.#buffered = Math.max(0, ws.#buffered - ev[2]);
          return;
      }
    };
    socketGen = (ws) => ws.#gen;
    socketForget = (ws, test) => {
      if (!test(ws.#gen)) return false;
      ws.#state = 3;
      return true;
    };
  }
}
for (const [i, k] of ["CONNECTING", "OPEN", "CLOSING", "CLOSED"].entries()) {
  Object.defineProperty(WebSocket, k, { value: i, enumerable: true });
  Object.defineProperty(WebSocket.prototype, k, { value: i, enumerable: true });
}
eventHandlers(WebSocket.prototype, ["open", "message", "error", "close"]);

/** `headers` as `[name, value]` pairs: from a record, an array of pairs or anything with `entries`. */
function headerPairs(h) {
  if (h == null) return [];
  let pairs;
  if (typeof h[Symbol.iterator] === "function") pairs = [...h].map((p) => [...p]);
  else pairs = Object.keys(h).map((k) => [k, h[k]]);
  return pairs.map((p) => {
    if (p.length !== 2) throw new TypeError("WebSocket: each header must be a [name, value] pair");
    const name = String(p[0]);
    if (!TOKEN.test(name)) throw new TypeError(`WebSocket: '${name}' is not a valid header name`);
    return [name, String(p[1])];
  });
}

// ---------------------------------------------------------------------------------------------
// fetch: Headers, Request, Response, and requests Rust carries out whole

/** HTTP whitespace off both ends of a header value, which must hold no NUL, CR or LF. */
function headerValue(v) {
  v = String(v).replace(/^[\t\n\r ]+|[\t\n\r ]+$/g, "");
  if (/[\0\r\n]/.test(v)) throw new TypeError(`Headers: ${JSON.stringify(v)} is not a valid header value`);
  return v;
}

function headerName(n) {
  n = String(n);
  if (!TOKEN.test(n)) throw new TypeError(`Headers: '${n}' is not a valid header name`);
  return n.toLowerCase();
}

let headersList;
let headersLock;

class Headers {
  /** `[lower-case name, value]` pairs, in the order they were added. */
  #list = [];
  #immutable = false;

  constructor(init) {
    if (init === undefined || init === null) return;
    if (init instanceof Headers) {
      for (const [n, v] of init.#list) this.#list.push([n, v]);
      return;
    }
    if (typeof init !== "object" && typeof init !== "function") throw new TypeError("Headers: init must be an object");
    if (typeof init[Symbol.iterator] === "function") {
      for (const pair of init) {
        const p = [...pair];
        if (p.length !== 2) throw new TypeError("Headers: each header must be a [name, value] pair");
        this.append(p[0], p[1]);
      }
    } else for (const k of Object.keys(init)) this.append(k, init[k]);
  }

  #check() {
    if (this.#immutable) throw new TypeError("Headers are immutable");
  }

  append(name, value) {
    this.#check();
    this.#list.push([headerName(name), headerValue(value)]);
  }
  delete(name) {
    this.#check();
    const n = headerName(name);
    this.#list = this.#list.filter((p) => p[0] !== n);
  }
  get(name) {
    const n = headerName(name);
    const values = this.#list.filter((p) => p[0] === n).map((p) => p[1]);
    return values.length === 0 ? null : values.join(", ");
  }
  getSetCookie() {
    return this.#list.filter((p) => p[0] === "set-cookie").map((p) => p[1]);
  }
  has(name) {
    const n = headerName(name);
    return this.#list.some((p) => p[0] === n);
  }
  set(name, value) {
    this.#check();
    const n = headerName(name);
    const v = headerValue(value);
    const i = this.#list.findIndex((p) => p[0] === n);
    if (i < 0) {
      this.#list.push([n, v]);
      return;
    }
    this.#list[i][1] = v;
    this.#list = this.#list.filter((p, j) => j <= i || p[0] !== n);
  }
  forEach(callback, thisArg) {
    for (const [n, v] of this.entries()) callback.call(thisArg, v, n, this);
  }

  /** Sorted by name, repeated names combined, each `set-cookie` apart. */
  *entries() {
    const names = [...new Set(this.#list.map((p) => p[0]))].sort();
    for (const n of names) {
      if (n === "set-cookie") for (const v of this.getSetCookie()) yield [n, v];
      else yield [n, this.get(n)];
    }
  }
  *keys() {
    for (const [n] of this.entries()) yield n;
  }
  *values() {
    for (const [, v] of this.entries()) yield v;
  }
  [Symbol.iterator]() {
    return this.entries();
  }

  static {
    headersList = (h) => h.#list;
    headersLock = (h) => {
      h.#immutable = true;
      return h;
    };
  }
}
Object.defineProperty(Headers.prototype, Symbol.toStringTag, { value: "Headers", configurable: true });

/** A body init as bytes and its default `content-type` (`null` for none). */
function extractBody(body) {
  if (body instanceof Blob) return { bytes: blobBytes(body), type: body.type || null };
  if (body instanceof ArrayBuffer || ArrayBuffer.isView(body)) return { bytes: copyBytes(body), type: null };
  if (body instanceof URLSearchParams) return { bytes: encode(body.toString()), type: "application/x-www-form-urlencoded;charset=UTF-8" };
  return { bytes: encode(String(body)), type: "text/plain;charset=UTF-8" };
}

/** The web's body readers, on `proto`, over a `{ bytes, used }` state each instance holds. */
function bodyMixin(proto, state, what) {
  const take = (self) => {
    const s = state(self);
    if (s.used) return Promise.reject(new TypeError(`${what}: the body has already been read`));
    s.used = true;
    return Promise.resolve(s.bytes || new Uint8Array(0));
  };
  Object.defineProperties(proto, {
    bodyUsed: {
      get() {
        return state(this).used;
      },
      configurable: true,
      enumerable: true,
    },
    text: {
      value() {
        return take(this).then(decode);
      },
      writable: true,
      configurable: true,
    },
    json: {
      value() {
        return take(this).then((b) => JSON.parse(decode(b)));
      },
      writable: true,
      configurable: true,
    },
    arrayBuffer: {
      value() {
        return take(this).then((b) => b.slice().buffer);
      },
      writable: true,
      configurable: true,
    },
    bytes: {
      value() {
        return take(this).then((b) => b.slice());
      },
      writable: true,
      configurable: true,
    },
    blob: {
      value() {
        const type = this.headers.get("content-type") || "";
        return take(this).then((b) => new Blob([b], { type }));
      },
      writable: true,
      configurable: true,
    },
  });
}

const METHODS = ["DELETE", "GET", "HEAD", "OPTIONS", "POST", "PUT"];
const FORBIDDEN_METHODS = ["CONNECT", "TRACE", "TRACK"];

let requestState;
let requestRedirect;

class Request {
  #url;
  #method = "GET";
  #headers;
  #signal;
  #redirect = "follow";
  #body = { bytes: null, used: false };

  constructor(input, init = {}) {
    if (arguments.length === 0) throw new TypeError("Request: the input argument is required");
    init = init == null ? {} : init;
    let from = null;
    if (input instanceof Request) {
      from = input;
      this.#url = input.#url;
      this.#method = input.#method;
      this.#redirect = input.#redirect;
    } else {
      let u;
      try {
        u = new URL(String(input));
      } catch {
        throw new TypeError(`Request: '${input}' is not an absolute URL`);
      }
      if (u.username || u.password) throw new TypeError("Request: a URL with credentials is not allowed");
      u.hash = "";
      this.#url = u.href;
    }
    if (init.method !== undefined) {
      const m = String(init.method);
      if (!TOKEN.test(m)) throw new TypeError(`Request: '${m}' is not a valid method`);
      const upper = m.toUpperCase();
      if (FORBIDDEN_METHODS.includes(upper)) throw new TypeError(`Request: the method ${upper} is forbidden`);
      this.#method = METHODS.includes(upper) ? upper : m;
    }
    if (init.redirect !== undefined) {
      const r = String(init.redirect);
      if (r !== "follow" && r !== "error" && r !== "manual") throw new TypeError(`Request: '${r}' is not a valid redirect mode`);
      this.#redirect = r;
    }
    const signal = init.signal !== undefined ? init.signal : from ? from.#signal : null;
    if (signal !== null && signal !== undefined && !(signal instanceof AbortSignal)) throw new TypeError("Request: signal is not an AbortSignal");
    this.#signal = signal ? AbortSignal.any([signal]) : new AbortSignal(ILLEGAL);
    this.#headers = new Headers(init.headers !== undefined ? init.headers : from ? from.#headers : undefined);

    let body = null;
    if (init.body !== undefined && init.body !== null) {
      const { bytes, type } = extractBody(init.body);
      body = bytes;
      if (type !== null && !this.#headers.has("content-type")) this.#headers.append("content-type", type);
    } else if (from && from.#body.bytes !== null) {
      if (from.#body.used) throw new TypeError("Request: the input's body has already been read");
      body = from.#body.bytes;
      from.#body.used = true;
    }
    if (body !== null && (this.#method === "GET" || this.#method === "HEAD")) throw new TypeError(`Request: a ${this.#method} request cannot have a body`);
    this.#body.bytes = body;
  }

  get url() {
    return this.#url;
  }
  get method() {
    return this.#method;
  }
  get headers() {
    return this.#headers;
  }
  get signal() {
    return this.#signal;
  }
  get redirect() {
    return this.#redirect;
  }

  clone() {
    if (this.#body.used) throw new TypeError("Request.clone: the body has already been read");
    const r = new Request(this.#url, { method: this.#method, headers: this.#headers, signal: this.#signal, redirect: this.#redirect });
    r.#body.bytes = this.#body.bytes;
    return r;
  }

  static {
    requestState = (r) => r.#body;
    requestRedirect = (r) => r.#redirect;
  }
}
bodyMixin(Request.prototype, (r) => requestState(r), "Request");
Object.defineProperty(Request.prototype, Symbol.toStringTag, { value: "Request", configurable: true });

const NULL_BODY_STATUS = [101, 103, 204, 205, 304];
const REDIRECT_STATUS = [301, 302, 303, 307, 308];

let responseState;
let makeResponse;

class Response {
  #status = 200;
  #statusText = "";
  #headers = new Headers();
  #type = "default";
  #url = "";
  #redirected = false;
  #body = { bytes: null, used: false };

  constructor(body = null, init = {}) {
    init = init == null ? {} : init;
    if (init.status !== undefined) {
      const s = Number(init.status);
      if (!(s >= 200 && s <= 599) || !Number.isInteger(s)) throw new RangeError(`Response: the status ${init.status} is not in the range 200 to 599`);
      this.#status = s;
    }
    if (init.statusText !== undefined) {
      const t = String(init.statusText);
      if (!/^[\t\x20-\x7e\x80-\xff]*$/.test(t)) throw new TypeError("Response: the statusText is not a valid reason phrase");
      this.#statusText = t;
    }
    if (init.headers !== undefined) this.#headers = new Headers(init.headers);
    if (body !== null && body !== undefined) {
      if (NULL_BODY_STATUS.includes(this.#status)) throw new TypeError(`Response: a ${this.#status} response cannot have a body`);
      const { bytes, type } = extractBody(body);
      this.#body.bytes = bytes;
      if (type !== null && !this.#headers.has("content-type")) this.#headers.append("content-type", type);
    }
  }

  get type() {
    return this.#type;
  }
  get url() {
    return this.#url;
  }
  get redirected() {
    return this.#redirected;
  }
  get status() {
    return this.#status;
  }
  get ok() {
    return this.#status >= 200 && this.#status <= 299;
  }
  get statusText() {
    return this.#statusText;
  }
  get headers() {
    return this.#headers;
  }

  clone() {
    if (this.#body.used) throw new TypeError("Response.clone: the body has already been read");
    const r = new Response();
    r.#status = this.#status;
    r.#statusText = this.#statusText;
    r.#headers = new Headers(this.#headers);
    if (this.#type === "basic" || this.#type === "error") headersLock(r.#headers);
    r.#type = this.#type;
    r.#url = this.#url;
    r.#redirected = this.#redirected;
    r.#body.bytes = this.#body.bytes;
    return r;
  }

  static error() {
    const r = new Response();
    r.#status = 0;
    r.#type = "error";
    headersLock(r.#headers);
    return r;
  }

  static redirect(url, status = 302) {
    let u;
    try {
      u = new URL(String(url));
    } catch {
      throw new TypeError(`Response.redirect: '${url}' is not an absolute URL`);
    }
    status = Number(status);
    if (!REDIRECT_STATUS.includes(status)) throw new RangeError(`Response.redirect: ${status} is not a redirect status`);
    const r = new Response(null, { status, headers: { location: u.href } });
    headersLock(r.#headers);
    return r;
  }

  static json(data, init = {}) {
    const text = JSON.stringify(data);
    if (text === undefined) throw new TypeError("Response.json: the data is not JSON-serializable");
    const r = new Response(text, init);
    if (!(init && init.headers && new Headers(init.headers).has("content-type"))) r.#headers.set("content-type", "application/json");
    return r;
  }

  static {
    responseState = (r) => r.#body;
    makeResponse = (status, statusText, pairs, bytes, url, redirected) => {
      const r = new Response();
      r.#status = status;
      r.#statusText = statusText;
      for (const [n, v] of pairs) headersList(r.#headers).push([String(n).toLowerCase(), String(v)]);
      headersLock(r.#headers);
      r.#type = "basic";
      r.#url = url;
      r.#redirected = redirected;
      r.#body.bytes = NULL_BODY_STATUS.includes(status) ? null : bytes;
      return r;
    };
  }
}
bodyMixin(Response.prototype, (r) => responseState(r), "Response");
Object.defineProperty(Response.prototype, Symbol.toStringTag, { value: "Response", configurable: true });

/** Fetches Rust carries out, by id: `{ resolve, reject, gen, signal, onAbort }`. */
const fetches = new Map();
let nextFetchId = 1;
const REDIRECT_MODES = { follow: 0, error: 1, manual: 2 };

function forgetFetch(id) {
  const f = fetches.get(id);
  if (!f) return null;
  fetches.delete(id);
  f.signal.removeEventListener("abort", f.onAbort);
  return f;
}

function fetch(input, init = undefined) {
  return new Promise((resolve, reject) => {
    const request = new Request(input, init);
    const url = new URL(request.url);
    if (url.protocol !== "http:" && url.protocol !== "https:") throw new TypeError(`fetch: the scheme ${url.protocol} is not supported; use http: or https:`);
    const signal = request.signal;
    if (signal.aborted) throw signal.reason;
    const id = nextFetchId++;
    const gen = hooks.generation;
    const onAbort = () => {
      if (!forgetFetch(id)) return;
      reject(signal.reason);
      core.ops.op_bevyts_fetch_abort(id, gen);
    };
    fetches.set(id, { resolve, reject, gen, signal, onAbort });
    signal.addEventListener("abort", onAbort);
    const body = requestState(request).bytes;
    const headers = headersList(request.headers).map(([n, v]) => [n, v]);
    core.ops.op_bevyts_fetch(id, gen, request.method, request.url, JSON.stringify(headers), body || new Uint8Array(0), REDIRECT_MODES[requestRedirect(request)]);
  });
}

/** A fetch event: `[6, id, status, statusText, headersJson, ArrayBuffer, url, redirected]` or `[7, id, reason]`. */
function fetchEvent(ev) {
  const f = forgetFetch(ev[1]);
  if (!f) return;
  if (ev[0] === 6) f.resolve(makeResponse(ev[2], ev[3], JSON.parse(ev[4]), new Uint8Array(ev[5]), ev[6], ev[7]));
  else f.reject(new TypeError(ev[2]));
}

// ---------------------------------------------------------------------------------------------
// Delivery and generations

/** Dispatches `events` in order, then fires the timers due at `now`; answers the next deadline. */
function deliver(now, events) {
  delivering = true;
  try {
    for (const ev of events) {
      if (ev[0] >= 6) {
        fetchEvent(ev);
        continue;
      }
      const ws = sockets.get(ev[1]);
      if (ws) withGeneration(socketGen(ws), () => socketEvent(ws, ev));
    }
    // The due set is taken first, so a timer set while these fire waits for the next delivery.
    const due = [];
    while (heap.length > 0 && heap[0].deadline <= now) {
      const t = heapPop();
      if (!t.cancelled) due.push(t);
    }
    for (const t of due) if (!t.cancelled) fire(t);
  } finally {
    delivering = false;
  }
  reported = earliest();
  core.ops.op_bevyts_timer_deadline(reported);
  return reported;
}

function drop(test) {
  for (const [id, ws] of sockets) if (socketForget(ws, test)) sockets.delete(id);
  for (const [id, f] of fetches) if (test(f.gen)) forgetFetch(id);
  for (const [id, t] of timers) {
    if (!test(t.gen)) continue;
    timers.delete(id);
    t.cancelled = true;
  }
  const keep = heap.filter((t) => !t.cancelled);
  heap.length = 0;
  for (const t of keep) heapPush(t);
  reportDeadline();
}

// ---------------------------------------------------------------------------------------------
// process: `env`, filled once from the variables the project names, then frozen; `argv`, filled
// once in Node's shape (the engine, the entry, then the game's arguments)

const env = {};
let envSet = false;

function setEnv(obj) {
  if (envSet) return;
  envSet = true;
  if (obj && typeof obj === "object") for (const k of Object.keys(obj)) if (typeof obj[k] === "string") env[k] = obj[k];
  Object.freeze(env);
}

const argv = [];
let argvSet = false;

function setArgv(list) {
  if (argvSet) return;
  argvSet = true;
  if (Array.isArray(list)) for (const a of list) argv.push(String(a));
}

const process = {};
Object.defineProperty(process, "env", { value: env, enumerable: true });
Object.defineProperty(process, "argv", { value: argv, enumerable: true });
Object.defineProperty(process, Symbol.toStringTag, { value: "process", configurable: true });

// ---------------------------------------------------------------------------------------------
// Install

if (core) {
  const globals = {
    EventTarget,
    Event,
    CloseEvent,
    MessageEvent,
    DOMException,
    AbortController,
    AbortSignal,
    URL,
    URLSearchParams,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    WebSocket,
    Blob,
    Headers,
    Request,
    Response,
    fetch,
    process,
  };
  for (const [name, value] of Object.entries(globals)) Object.defineProperty(globalThis, name, { value, writable: true, configurable: true, enumerable: false });
  Object.defineProperty(globalThis, "__bevytsWeb", { value: hooks, writable: true, configurable: true, enumerable: false });
}
