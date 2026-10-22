// The web globals the bevy-ts engine installs for scripts (docs/networking.md), and nothing else.
//
// Use with `"types": ["bevy-ts/globals"]` and a `lib` without `dom` (`"lib": ["es2023"]`).
//
// Classes are declared as `interface X` plus `var X: { prototype: X; new(...): X }`, the shape
// lib.dom.d.ts uses, and `process` through `NodeJS.Process`/`NodeJS.ProcessEnv` as @types/node
// declares it, so a project whose packages pull in @types/node still type-checks.
//
// @types/node declares `DOMException`, `URL`, `URLSearchParams` and `WebSocket` with its own
// constructor types, and a `var` declared twice must have one type. Those four therefore take
// @types/node's constructor when it is present (`global` exists only there) and this file's
// otherwise. That works when @types/node comes first, as it does when an imported package
// references it; a tsconfig naming both in `types` lists `node` first. This file is a module
// with `declare global` so its globals merge after @types/node's, which are declared the same way.
// Tested against @types/node 22 and 26 (test/types-node).

export {};

declare global {
  // Console (installed by the engine before any module)

  interface Console {
    debug(...data: any[]): void;
    error(...data: any[]): void;
    info(...data: any[]): void;
    log(...data: any[]): void;
    trace(...data: any[]): void;
    warn(...data: any[]): void;
  }
  var console: Console;

  // Events

  interface EventInit {
    bubbles?: boolean;
    cancelable?: boolean;
    composed?: boolean;
  }

  interface Event {
    readonly type: string;
    readonly target: EventTarget | null;
    readonly currentTarget: EventTarget | null;
    readonly bubbles: boolean;
    readonly cancelable: boolean;
    readonly defaultPrevented: boolean;
    readonly isTrusted: boolean;
    readonly timeStamp: number;
    preventDefault(): void;
    stopImmediatePropagation(): void;
    stopPropagation(): void;
  }
  var Event: {
    prototype: Event;
    new (type: string, eventInitDict?: EventInit): Event;
  };

  interface EventListener {
    (evt: Event): void;
  }
  interface EventListenerObject {
    handleEvent(object: Event): void;
  }
  type EventListenerOrEventListenerObject = EventListener | EventListenerObject;

  interface EventListenerOptions {
    capture?: boolean;
  }
  interface AddEventListenerOptions extends EventListenerOptions {
    once?: boolean;
    passive?: boolean;
    signal?: AbortSignal;
  }

  interface EventTarget {
    addEventListener(type: string, callback: EventListenerOrEventListenerObject | null, options?: AddEventListenerOptions | boolean): void;
    dispatchEvent(event: Event): boolean;
    removeEventListener(type: string, callback: EventListenerOrEventListenerObject | null, options?: EventListenerOptions | boolean): void;
  }
  var EventTarget: {
    prototype: EventTarget;
    new (): EventTarget;
  };

  interface MessageEventInit<T = any> extends EventInit {
    data?: T;
    origin?: string;
    lastEventId?: string;
  }

  /** A `WebSocket` message: `data` is a string for a text frame, an `ArrayBuffer` for a binary one. */
  interface MessageEvent<T = any> extends Event {
    readonly data: T;
    readonly origin: string;
    readonly lastEventId: string;
  }
  var MessageEvent: {
    prototype: MessageEvent;
    new <T>(type: string, eventInitDict?: MessageEventInit<T>): MessageEvent<T>;
  };

  interface CloseEventInit extends EventInit {
    code?: number;
    reason?: string;
    wasClean?: boolean;
  }

  interface CloseEvent extends Event {
    /** 1000 normal, 1001 going away (a reload, or leaving play), 1005 no code, 1006 dropped, 1009 too big. */
    readonly code: number;
    readonly reason: string;
    readonly wasClean: boolean;
  }
  var CloseEvent: {
    prototype: CloseEvent;
    new (type: string, eventInitDict?: CloseEventInit): CloseEvent;
  };

  interface DOMException extends Error {
    readonly code: number;
    readonly message: string;
    readonly name: string;
  }
  var DOMException: typeof globalThis extends { global: any; DOMException: infer T } ? T : {
    prototype: DOMException;
    new (message?: string, name?: string): DOMException;
  };

  // Abort

  interface AbortSignalEventMap {
    abort: Event;
  }

  interface AbortSignal extends EventTarget {
    readonly aborted: boolean;
    readonly reason: any;
    onabort: ((this: AbortSignal, ev: Event) => any) | null;
    throwIfAborted(): void;
    addEventListener<K extends keyof AbortSignalEventMap>(type: K, listener: (this: AbortSignal, ev: AbortSignalEventMap[K]) => any, options?: boolean | AddEventListenerOptions): void;
    addEventListener(type: string, listener: EventListenerOrEventListenerObject, options?: boolean | AddEventListenerOptions): void;
    removeEventListener<K extends keyof AbortSignalEventMap>(type: K, listener: (this: AbortSignal, ev: AbortSignalEventMap[K]) => any, options?: boolean | EventListenerOptions): void;
    removeEventListener(type: string, listener: EventListenerOrEventListenerObject, options?: boolean | EventListenerOptions): void;
  }
  var AbortSignal: {
    prototype: AbortSignal;
    new (): AbortSignal;
    abort(reason?: any): AbortSignal;
    any(signals: AbortSignal[]): AbortSignal;
    /** Aborts after `milliseconds` of real time, like the timers. */
    timeout(milliseconds: number): AbortSignal;
  };

  interface AbortController {
    readonly signal: AbortSignal;
    abort(reason?: any): void;
  }
  var AbortController: {
    prototype: AbortController;
    new (): AbortController;
  };

  // URL

  interface URL {
    hash: string;
    host: string;
    hostname: string;
    href: string;
    toString(): string;
    readonly origin: string;
    password: string;
    pathname: string;
    port: string;
    protocol: string;
    search: string;
    readonly searchParams: URLSearchParams;
    username: string;
    toJSON(): string;
  }
  var URL: typeof globalThis extends { global: any; URL: infer T } ? T : {
    prototype: URL;
    new (url: string | URL, base?: string | URL): URL;
  };

  interface URLSearchParams {
    readonly size: number;
    append(name: string, value: string): void;
    delete(name: string, value?: string): void;
    get(name: string): string | null;
    getAll(name: string): string[];
    has(name: string, value?: string): boolean;
    set(name: string, value: string): void;
    sort(): void;
    toString(): string;
    forEach(callbackfn: (value: string, key: string, parent: URLSearchParams) => void, thisArg?: any): void;
    [Symbol.iterator](): URLSearchParamsIterator<[string, string]>;
    entries(): URLSearchParamsIterator<[string, string]>;
    keys(): URLSearchParamsIterator<string>;
    values(): URLSearchParamsIterator<string>;
  }
  interface URLSearchParamsIterator<T> extends IteratorObject<T, BuiltinIteratorReturn, unknown> {
    [Symbol.iterator](): URLSearchParamsIterator<T>;
  }
  var URLSearchParams: typeof globalThis extends { global: any; URLSearchParams: infer T } ? T : {
    prototype: URLSearchParams;
    new (init?: string[][] | Record<string, string> | string | URLSearchParams): URLSearchParams;
  };

  // Timers: real time, running while the game is paused; none fire in edit mode.

  /** Returns a positive integer id. A missing or negative delay is 0. */
  function setTimeout<TArgs extends any[]>(handler: (...args: TArgs) => void, timeout?: number, ...args: TArgs): number;
  function clearTimeout(id: number | undefined): void;
  /** Returns a positive integer id. A late interval fires once and reschedules from then. */
  function setInterval<TArgs extends any[]>(handler: (...args: TArgs) => void, timeout?: number, ...args: TArgs): number;
  function clearInterval(id: number | undefined): void;

  // WebSocket: ws: only; wss: is refused. The URL's host must be in net.allow in bevy-ts.json.

  type BinaryType = "arraybuffer" | "blob";

  /**
   * NON-STANDARD: the Deno-shaped second argument of `new WebSocket(url, options)`. No browser
   * accepts it, and nothing in bevy-ts depends on it.
   */
  interface WebSocketOptions {
    protocols?: string | string[];
    /**
     * NON-STANDARD: extra handshake headers, such as `authorization`. Refused unless the URL's host
     * is loopback (`localhost`, `127.0.0.0/8`, `::1`), so a key never crosses the network in clear
     * text; an `Origin` header is always refused (the engine sends none).
     */
    headers?: Record<string, string> | [string, string][];
  }

  interface WebSocketEventMap {
    close: CloseEvent;
    error: Event;
    message: MessageEvent;
    open: Event;
  }

  interface WebSocket extends EventTarget {
    /** Always `"arraybuffer"`: setting `"blob"` logs a warning and keeps it. */
    binaryType: BinaryType;
    readonly bufferedAmount: number;
    readonly extensions: string;
    onclose: ((ev: CloseEvent) => any) | null;
    onerror: ((ev: Event) => any) | null;
    onmessage: ((ev: MessageEvent) => any) | null;
    onopen: ((ev: Event) => any) | null;
    readonly protocol: string;
    readonly readyState: number;
    readonly url: string;
    /** `code` is 1000 or 3000-4999; `reason` at most 123 UTF-8 bytes. */
    close(code?: number, reason?: string): void;
    /** Reaches the socket at the next frame's delivery. */
    send(data: string | ArrayBufferLike | ArrayBufferView): void;
    readonly CONNECTING: 0;
    readonly OPEN: 1;
    readonly CLOSING: 2;
    readonly CLOSED: 3;
    addEventListener<K extends keyof WebSocketEventMap>(type: K, listener: (this: WebSocket, ev: WebSocketEventMap[K]) => any, options?: boolean | AddEventListenerOptions): void;
    addEventListener(type: string, listener: EventListenerOrEventListenerObject, options?: boolean | AddEventListenerOptions): void;
    removeEventListener<K extends keyof WebSocketEventMap>(type: K, listener: (this: WebSocket, ev: WebSocketEventMap[K]) => any, options?: boolean | EventListenerOptions): void;
    removeEventListener(type: string, listener: EventListenerOrEventListenerObject, options?: boolean | EventListenerOptions): void;
  }
  var WebSocket: typeof globalThis extends { global: any; WebSocket: infer T } ? T : {
    prototype: WebSocket;
    /**
     * The web's constructor: `protocols` is a string or an array of them. An `http:` URL is read as
     * `ws:`.
     *
     * NON-STANDARD: `protocols` may instead be `{ protocols, headers }`, as Deno accepts
     * ({@link WebSocketOptions}). Browsers do not.
     */
    new (url: string | URL, protocols?: string | string[] | WebSocketOptions): WebSocket;
    readonly CONNECTING: 0;
    readonly OPEN: 1;
    readonly CLOSING: 2;
    readonly CLOSED: 3;
  };

  // process: `env` only, holding the names in bevy-ts.json's `env` that were set when the engine
  // started. Declared as @types/node declares it, so the two merge.

  namespace NodeJS {
    interface Process {
      env: ProcessEnv;
    }
    interface ProcessEnv {
      [key: string]: string | undefined;
    }
  }
  var process: NodeJS.Process;
}
