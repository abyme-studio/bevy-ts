/// <reference path="./wgsl.d.ts" />
// Types for `bevy-ts`: the ruled surface (operator, 2026-10-20; docs/research/dx.md). The runtime
// is built into the engine binary.
//
// Systems are plain exported functions whose parameter types say what they use, as in Rust.
// bevy-ts's oxc compile step reads those annotations from syntax alone (bench/dx/read-annotations.ts)
// and attaches the declarations. A system whose param types are computed (ReturnType, Pick, ...)
// uses the setup-closure fallback, `system(() => { ...declare; return () => { ...run } })`.

// ---------- Field types ----------
// A class field's type is one of these (`hp: f32 = 100`), read by the compile step from syntax.
// In TS they are plain number, boolean and vector types. The same names are values for the
// setup closure's `local({ n: u32 })`.

export type FieldKind =
  | "f32" | "f64" | "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "bool" | "vec2" | "vec3" | "vec4" | "quat" | "entity" | "enum"
  | "array";

export interface Field<K extends FieldKind, E extends string = never> {
  readonly kind: K;
  readonly variants?: readonly E[];
  /** A default for new values and for new fields in a migration. */
  default(v: unknown): Field<K, E>;
}
export declare const f32: Field<"f32">;
export type f32 = number;
export declare const f64: Field<"f64">;
export type f64 = number;
export declare const i32: Field<"i32">;
export type i32 = number;
export declare const u32: Field<"u32">;
export type u32 = number;
export declare const u8: Field<"u8">;
export type u8 = number;
export declare const i8: Field<"i8">;
export type i8 = number;
export declare const i16: Field<"i16">;
export type i16 = number;
export declare const u16: Field<"u16">;
export type u16 = number;
export declare const bool: Field<"bool">;
export type bool = boolean;
export declare const vec2: Field<"vec2"> & ((x?: number, y?: number) => Vec3);
export declare const vec4: Field<"vec4">;
/** A scratch-vector constructor: `vec3(0, 1, 0)`. */
export declare const vec3: Field<"vec3"> & ((x?: number, y?: number, z?: number) => Vec3);
export type vec3 = Vec3;
export declare const quat: Field<"quat"> & ((x?: number, y?: number, z?: number, w?: number) => Quat);
export type quat = Quat;
export declare const entity: Field<"entity">;
export declare function enumOf<const E extends string>(...variants: E[]): Field<"enum", E>;

export type Schema = Record<string, Field<FieldKind, string>>;

// ---------- Vectors: in-place, glam-like, allocation-free ----------

declare const entityBrand: unique symbol;
/** An entity id: `Entity::to_bits` as a number. */
export type Entity = number & { readonly [entityBrand]?: true };

export interface ReadonlyVec3 {
  readonly x: number;
  readonly y: number;
  readonly z: number;
  length(): number;
  dot(v: ReadonlyVec3): number;
  distance(v: ReadonlyVec3): number;
}
/** A view into a column, or a scratch vector from `vec3()`. Every method writes `this`. */
export interface Vec3 extends ReadonlyVec3 {
  x: number;
  y: number;
  z: number;
  set(x: number, y: number, z: number): this;
  copy(v: ReadonlyVec3): this;
  add(v: ReadonlyVec3): this;
  sub(v: ReadonlyVec3): this;
  scale(s: number): this;
  addScaled(v: ReadonlyVec3, s: number): this;
  normalize(): this;
  lerp(v: ReadonlyVec3, t: number): this;
  /** this = a - b */
  subVectors(a: ReadonlyVec3, b: ReadonlyVec3): this;
  rotate(q: ReadonlyQuat): this;
}
export interface ReadonlyQuat {
  readonly x: number;
  readonly y: number;
  readonly z: number;
  readonly w: number;
}
export interface Quat extends ReadonlyQuat {
  x: number;
  y: number;
  z: number;
  w: number;
  copy(q: ReadonlyQuat): this;
  lookTo(dir: ReadonlyVec3, up: ReadonlyVec3): this;
  rotateY(radians: number): this;
  slerp(q: ReadonlyQuat, t: number): this;
}
/** A `Vec2` field is a vector view whose `z` reads 0. */
export type Vec2 = Vec3;
export type Vec4 = Quat;
/** A vector prop or initial value: a tuple or a vector. */
export type Vec3Like = readonly [number, number, number] | ReadonlyVec3;

/** Read access: every field readonly, vectors without mutating methods. */
export type ReadView<T> = {
  readonly [K in keyof T as K extends symbol ? never : K]: T[K] extends Vec3 ? ReadonlyVec3 : T[K] extends Quat ? ReadonlyQuat : T[K];
};
/** Initial values, as Rust's `Health { hp: 3.0, ..default() }`: vectors take a tuple or a vector. */
export type Init<T> = {
  [K in keyof T as K extends symbol ? never : K]?: T[K] extends Vec3 ? Vec3Like : T[K] extends Quat ? readonly [number, number, number, number] | ReadonlyQuat : T[K];
};

// ---------- Components, resources, messages, events: plain classes (operator ruling 2026-10-20) ----------
// The compile step reads each class's fields (name, type, default) as it reads system signatures:
//   export class Health extends Component { hp: f32 = 100; max: f32 = 100 }
//   export class Damage extends Message { declare target: Entity; declare amount: f32 }
// The name defaults to a Rust-style path from the module (`src/game.ts` -> `game::Health`);
// `static path` pins it.

/**
 * `layout`: a column a TS system may read and write (TS components, ported Rust components).
 * `reflect`: typed in TSX and the protocol, usable in `With`/`Without`, never as query data.
 */
export type Tier = "layout" | "reflect";
declare const tierBrand: unique symbol;
declare const resourceBrand: unique symbol;
declare const messageBrand: unique symbol;
declare const eventBrand: unique symbol;

declare abstract class Data {
  /** Optional stable name; the default is the module's Rust-style path, `game::Health`. */
  static readonly path?: string;
  /** The resolved name (`game::Health`), set when the class is defined. */
  static readonly componentName: string;
  /** `ts` for a TS class, `rust` for a Rust type. */
  static readonly source: "ts" | "rust";
  /** A Rust component with a native layout. */
  static readonly native?: true;
  /** The class's field names, in declaration order. */
  static readonly fieldNames: readonly string[];
  /** Byte offset of each field in a row, once the generation applies. */
  static readonly offsets: Readonly<Record<string, number>>;
  /** Bytes per row, once the generation applies. */
  static readonly size: number;
  /** `Health.of({ hp: 3 })`, Rust's `Health { hp: 3.0, ..default() }`. */
  static of<T>(this: new () => T, init?: Init<T>): T;
}
export declare abstract class Component extends Data { readonly [tierBrand]?: "layout" }
/**
 * A placeable prefab, Bevy's `#[derive(SceneComponent)]`: a component whose scene is spawned with
 * it. Its fields are component fields (patched by `field: v` in `.bsn`); `static scene(props)`
 * builds the scene from props (set by `@prop: v`). The component is always added, so
 * `Query<[Grunt]>` finds instances.
 */
export declare abstract class SceneComponent extends Component {
  /** The scene props type's keys, in order. */
  static readonly sceneProps: readonly string[];
  // `static scene(props)` is not declared here, so a subclass writes it without `override`
  // (deno checks with noImplicitOverride). JSX reads its props type from the subclass.
}
/** A Rust component reachable only through reflection (a ported type the layout rule refuses). */
export declare abstract class ReflectComponent extends Data { readonly [tierBrand]?: "reflect" }
export declare abstract class Resource extends Data { readonly [resourceBrand]?: true }
export declare abstract class Message extends Data { readonly [messageBrand]?: true }
export declare abstract class Event extends Data { readonly [eventBrand]?: "global" }
/** An EntityEvent: targets `entity`; `static propagate = true` bubbles it up `ChildOf`. */
export declare abstract class EntityEvent extends Data {
  readonly [eventBrand]?: "entity";
  entity: Entity;
  static readonly propagate?: boolean;
}
/** A component class: the handle in queries, its TSX tag and, through `of`, its values. */
export type ComponentClass = abstract new () => Component | ReflectComponent;
/** The name tsx's JSX types know a component class by. */
export type ComponentType<_ = any> = ComponentClass & { readonly componentName: string; readonly source: "ts" | "rust" };
/** A Rust component by type path or short name, for `With`/`Without` filters only. */
export declare function rustComponent(name: string): abstract new () => ReflectComponent;
/**
 * A Rust component with a native layout registered in Rust (a port's `Velocity`):
 * `class Thrust extends nativeComponent("Thrust", { power: f32 }) {}`.
 */
export declare function nativeComponent<const S extends Schema>(name: string, fields: S): {
  new (): Component & { -readonly [K in keyof S]: S[K] extends Field<"vec2" | "vec3"> ? Vec3 : S[K] extends Field<"quat"> ? Quat : S[K] extends Field<"bool"> ? boolean : S[K] extends Field<"entity"> ? Entity : S[K] extends Field<"enum", infer E> ? E : number };
  readonly componentName: string;
  readonly source: "rust";
  readonly native: true;
  of<T>(this: new () => T, init?: Init<T>): T;
};

/** The scene component a level names (`enemies::Grunt` or `src/enemies.tsx#Grunt`), for tsx's expand entry. */
export declare function sceneComponent(nameOrId: string): (abstract new () => SceneComponent) & typeof SceneComponent | undefined;

declare const stateBrand: unique symbol;
export interface StateValue<N extends string = string, V extends string = string> { readonly state: N; readonly value: V }
/** `class Game extends States("Menu", "Playing") {}`; `Game.Playing` is a value. */
export declare function States<const V extends string>(...values: V[]):
  & { new (): { readonly [stateBrand]: { values: V } }; readonly path?: string }
  & { readonly [K in V]: StateValue<string, K> };
type StateValueOf<G> = G extends { readonly [stateBrand]: { values: infer V extends string } }
  ? { [K in V]: StateValue<string, K> }[V]
  : never;

// ---------- System param types (read by the compile step) ----------

declare const mutBrand: unique symbol;
declare const optBrand: unique symbol;
declare const filterBrand: unique symbol;
export type Mut<T> = { readonly [mutBrand]: T };
export type Option<T> = { readonly [optBrand]: T };
export type With<T> = { readonly [filterBrand]: ["with", T] };
export type Without<T> = { readonly [filterBrand]: ["without", T] };
/** Rows whose component changed since this system last ran (bridge per-row ticks). */
export type Changed<T> = { readonly [filterBrand]: ["changed", T] };
export type Added<T> = { readonly [filterBrand]: ["added", T] };

type NotReflect<T> = T extends ReflectComponent ? never : T;
type Item<D> =
  D extends Entity ? Entity
  : D extends Option<infer X> ? Item<X> | undefined
  : D extends Mut<infer T> ? NotReflect<T>
  : D extends abstract new (...a: never) => infer I ? ReadView<NotReflect<I>> // value form: Mut(C) / C
  : ReadView<NotReflect<D>>;
export type Row<Ds extends readonly unknown[]> = { -readonly [I in keyof Ds]: Item<Ds[I]> };

/** A component's typed arrays over one table's column; only the views its fields need exist. */
export interface ColumnArrays {
  readonly buffer: ArrayBuffer;
  readonly f32?: Float32Array;
  readonly f64?: Float64Array;
  readonly i8?: Int8Array;
  readonly u8?: Uint8Array;
  readonly i16?: Int16Array;
  readonly u16?: Uint16Array;
  readonly i32?: Int32Array;
  readonly u32?: Uint32Array;
  /** Byte offsets of the fields in a row. */
  readonly offsets: Record<string, number>;
  /** Bytes per row. */
  readonly size: number;
  /** `size / 4`: elements per row in a 4-byte view. */
  readonly stride: number;
}
export interface Table {
  readonly len: number;
  /** (index, generation) pairs. */
  readonly entities: Uint32Array;
  /** For `Changed`/`Added` queries: the rows that passed. */
  readonly rows: Uint32Array | null;
  entity(row: number): Entity;
  read(c: ComponentClass): ColumnArrays;
  /** A `Mut` component's arrays; every row of this table is marked changed after the run. */
  write(c: ComponentClass): ColumnArrays;
  /** A `Mut` component's arrays plus its changed ticks; set `ticks[row] = q.tick` yourself. */
  split(c: ComponentClass): ColumnArrays & { readonly ticks: Uint32Array };
}
export interface QueryOf<R extends unknown[]> extends Iterable<R> {
  /** Fastest form: reused accessors, no iterator protocol (2.4x faster than for-of on V8). */
  each(f: (...row: R) => void): void;
  /** Exactly one match, or throws (Bevy's `single`). */
  single(): R;
  get(e: Entity): R | undefined;
  readonly len: number;
  /** This run's change tick, for `split` tables. */
  readonly tick: number;
  readonly tables: readonly Table[];
}
/** `Query<[Enemy, Mut<Transform>], [With<Alive>, Without<Player>]>`, as Bevy's `Query<(&Enemy, &mut Transform), (With<Alive>, Without<Player>)>`. */
export type Query<Ds extends readonly unknown[], _Filter = never> = QueryOf<Row<Ds>>;
export type Res<T> = ReadView<T>;
export type ResMut<T> = T;
/** Per-system state with a layout, `Local<{ timer: f32; wave: u32 }>`. Kept in Rust under the system's id; survives reloads. */
export type Local<T> = T;
export interface MessageReader<M> extends Iterable<ReadView<M>> {
  each(f: (m: ReadView<M>) => void): void;
  readonly len: number;
}
export interface MessageWriter<M> {
  write(value: M | Partial<M>): void;
  /** Allocation-free: a writable view on a fresh slot. */
  next(): M;
}
export interface NextState<G> { set(v: StateValueOf<G>): void }
export interface State<G> { readonly current: StateValueOf<G> }

export interface Commands {
  /** Components (`Health.of({ hp: 3 })`, or a class for its defaults) or a prefab element (`<Grunt speed={3} />`). */
  spawn(...parts: readonly (Component | ReflectComponent | ComponentClass | Element)[]): Entity;
  entity(e: Entity): EntityCommands;
  /** As Rust's `commands.trigger(Explode { entity, radius })`: an entity event carries its target. */
  trigger(event: EntityEvent | Event): void;
  write(message: Message): void;
  /** Spawns through a native Rust recipe. */
  spawnNative(recipe: "camera3d", params?: { position?: [number, number, number]; lookAt?: [number, number, number] }): Entity;
  spawnNative(recipe: "directionalLight", params?: { position?: [number, number, number]; lookAt?: [number, number, number]; illuminance?: number; shadows?: boolean }): Entity;
  spawnNative(recipe: "cuboid", params?: { size?: [number, number, number]; color?: string; position?: [number, number, number]; lookAt?: [number, number, number] }): Entity;
  spawnNative(recipe: string, params?: unknown): Entity;
  /** Mounts an element tree under the tsx runtime root. */
  mount(element: Element, options?: { parent?: Entity | null }): void;
  /** Unmounts a runtime-mounted node by key, or any tsx node by entity. */
  unmount(target: string | Entity): void;
  /** Appends an op of a Rust plugin's command kind. */
  custom(kind: string, payload: Uint8Array | string | object): void;
}
export interface EntityCommands {
  insert(...parts: readonly (Component | ReflectComponent | ComponentClass | Element)[]): EntityCommands;
  remove(...cs: readonly ComponentClass[]): EntityCommands;
  setParent(parent: Entity | null): EntityCommands;
  despawn(): void;
  readonly id: Entity;
}

// ---------- Observers ----------

declare const lifecycleBrand: unique symbol;
export type Add<C> = { readonly [lifecycleBrand]: ["add", C] };
export type Insert<C> = { readonly [lifecycleBrand]: ["insert", C] };
export type Remove<C> = { readonly [lifecycleBrand]: ["remove", C] };
/**
 * An observer's first parameter, as Bevy's `On<Explode>` / `On<Add<Health>>`: the event's fields
 * directly (`explode.radius`, as Rust's `On` derefs to the event), the target as `entity`, and
 * `propagate`.
 */
export type On<E> =
  & (E extends { readonly [lifecycleBrand]: unknown } ? { readonly entity: Entity } : ReadView<E> & { readonly entity: Entity })
  & { /** Stop bubbling. */ propagate(on: boolean): void };
// ---------- Systems, schedules, ordering ----------

/** A system: an exported function whose parameter types the compile step read. */
export type SystemFn = (...params: never[]) => void;
/** A condition evaluated in Rust where it can be (state, resource compare), so a skipped system costs no JS call. */
export interface Condition { readonly condition: true }
export declare function inState<N extends string, V extends string>(v: StateValue<N, V>): Condition;
export declare function resourceExists(r: abstract new (...a: never) => unknown): Condition;
export declare function onMessage(m: abstract new (...a: never) => unknown): Condition;
export declare function not(c: Condition): Condition;

/** `class EnemySet extends SystemSet {}`, as Rust's `#[derive(SystemSet)] struct EnemySet;`. Named like a component. */
export declare abstract class SystemSet { static readonly path?: string; protected readonly __set?: true }
type SetRef = abstract new () => SystemSet;
/** A set a Rust crate exposed by name (`systemSet("rapier:step")`); ports export typed ones. */
export declare function systemSet(name: string): SetRef;

type Systemish = SystemFn | System | SystemConfig;
/** Bevy's tuple of systems and its config methods: `systems(a, b).after(x).runIf(c)`. */
export interface SystemConfig {
  after(...x: readonly (Systemish | SetRef)[]): SystemConfig;
  before(...x: readonly (Systemish | SetRef)[]): SystemConfig;
  runIf(c: Condition): SystemConfig;
  inSet(s: SetRef): SystemConfig;
}
export declare function systems(...s: readonly Systemish[]): SystemConfig;
/** Each runs after the previous, as Bevy's `.chain()`. */
export declare function chain(...s: readonly Systemish[]): SystemConfig;

export interface Schedule { readonly schedule: string }
export declare const Startup: Schedule;
export declare const PreUpdate: Schedule;
export declare const Update: Schedule;
export declare const FixedUpdate: Schedule;
export declare const PostUpdate: Schedule;
export declare function OnEnter<N extends string, V extends string>(v: StateValue<N, V>): Schedule;
export declare function OnExit<N extends string, V extends string>(v: StateValue<N, V>): Schedule;

// ---------- Fallback: the setup closure, for computed param types ----------
// `system(() => { const q = query(Enemy, Mut(Transform)).with(Alive); return () => { ... } })`.
// The outer function runs once per generation; declaring calls outside it are a load error.

export interface System extends SystemConfig { readonly id: string }
export declare function system(setup: () => () => void): System;
export declare function Mut<C extends abstract new (...a: never) => unknown>(c: C): Mut<InstanceType<C>>;
export declare function Option<D>(d: D): Option<D extends abstract new (...a: never) => infer I ? I : D>;
export declare const EntityId: Entity;
type AnyClass = abstract new (...a: never) => unknown;
export interface QueryBuilder<R extends unknown[]> extends QueryOf<R> {
  with(...cs: readonly AnyClass[]): QueryBuilder<R>;
  without(...cs: readonly AnyClass[]): QueryBuilder<R>;
  changed(...cs: readonly AnyClass[]): QueryBuilder<R>;
  added(...cs: readonly AnyClass[]): QueryBuilder<R>;
}
export declare function query<const Ds extends readonly unknown[]>(...data: Ds): QueryBuilder<Row<Ds>>;
export declare function res<R extends abstract new (...a: never) => unknown>(r: R): ReadView<InstanceType<R>>;
export declare function resMut<R extends abstract new (...a: never) => unknown>(r: R): InstanceType<R>;
export declare function commands(): Commands;
export declare function local<const S extends Schema>(fields: S): { -readonly [K in keyof S]: S[K] extends Field<"vec2" | "vec3"> ? Vec3 : S[K] extends Field<"quat"> ? Quat : S[K] extends Field<"bool"> ? boolean : number };

// ---------- Rust types bevy-ts ships typed ----------

export declare class Transform extends Component {
  static readonly path: "bevy_transform::components::transform::Transform";
  static readonly native: true;
  declare readonly translation: Vec3;
  declare readonly rotation: Quat;
  declare readonly scale: Vec3;
}
/** Bevy's `Time`, its own field names (the fixed step in `FixedUpdate`); its `Duration` fields are not visible. */
export declare class Time extends Resource {
  static readonly path: "bevy_time::Time";
  declare readonly delta_secs: f32;
  declare readonly elapsed_secs: f32;
  declare readonly delta_secs_f64: f64;
  declare readonly elapsed_secs_f64: f64;
}

// ---------- App and plugins ----------

export interface App {
  addSystems(schedule: Schedule, ...systems: readonly Systemish[]): App;
  addObserver(o: (on: never, ...params: never[]) => void): App;
  addPlugins(...p: readonly Plugin[]): App;
  initState(initial: StateValue): App;
  insertResource(r: Resource): App;
  addMessage(m: abstract new () => Message): App;
  /**
   * Declares a scene root, evaluated on every generation and reconciled by key path. `level`
   * names its sidecar level (default `levels/<name>.bsn`), where edits with no literal to
   * rewrite are kept as overrides.
   */
  scene(name: string, root: () => Element, options?: { level?: string }): App;
  /** Declares a level root: a `.bsn` file under the asset root (`"levels/arena.bsn"`), loaded and reconciled by bevy-ts-levels. */
  level(path: `${string}.bsn`): App;
  /** Declares an entry Rust plugins call by name with a byte payload. */
  entry(name: string, fn: (payload: Uint8Array, ctx: { readonly commands: Commands; readonly dt: number; readonly elapsed: number; readonly tick: number; readonly frame: number }) => Uint8Array | string | object | void): App;
}

/** Writes a manifest section for a Rust plugin. */
export declare function section(name: string, value: Uint8Array | string | object): void;
/** The loading generation's `{ items }` list for a JSON section, for a built-in module's per-generation declarations. */
export declare function sectionItems(name: string): unknown[];
/** Decodes UTF-8 bytes. */
export declare function text(bytes: Uint8Array): string;
/** A plugin: an exported function taking the app, as Bevy's `fn build(&self, app: &mut App)`. */
export type Plugin = (app: App) => void;
export declare const app: App;

// ---------- TSX ----------

export type Element = import("bevy-ts/jsx-runtime").Element;
export type Child = import("bevy-ts/jsx-runtime").Child;

/**
 * A component element's props (`<Health initialHp={100} max={100} />`): each field controlled
 * (`f`, written on insert and whenever the declared value changes; `() => value` binds it) or
 * initial (`initialF`, written on insert only). Giving both for one field is a type error for
 * components of up to 12 fields (TS expands the alternatives per field); larger components
 * take both spellings in types and fail at load instead.
 */
/** One field's initial value: vectors take a tuple or a vector. */
type InitOf<V> = V extends Vec3 ? Vec3Like : V extends Quat ? readonly [number, number, number, number] | ReadonlyQuat : V;
export type ElementProps<S> = AtMost12<keyof S & string> extends true ? ExclusiveProps<S> : PlainProps<S>;
type PlainProps<S> = { [K in keyof S]?: InitOf<S[K]> | (() => InitOf<S[K]>) } & {
  [K in keyof S & string as `initial${Capitalize<K>}`]?: InitOf<S[K]>;
};
type OneField<K extends string, V> =
  | ({ [P in K]?: V | (() => V) } & { [P in `initial${Capitalize<K>}`]?: never })
  | ({ [P in K]?: never } & { [P in `initial${Capitalize<K>}`]?: V });
type U2I<U> = (U extends unknown ? (x: U) => void : never) extends (x: infer I) => void ? I : never;
type ExclusiveProps<S> = [keyof S & string] extends [never]
  ? {}
  : U2I<{ [K in keyof S & string]: { v: OneField<K, InitOf<S[K]>> } }[keyof S & string]> extends { v: infer I } ? I : never;
type LastOf<U> = U2I<U extends unknown ? () => U : never> extends () => infer R ? R : never;
type AtMost12<U, N extends unknown[] = []> = [U] extends [never] ? true : N["length"] extends 12 ? false : AtMost12<Exclude<U, LastOf<U>>, [...N, 1]>;


// ---------------------------------------------------------------------------------------------
// TSX reactivity. No hooks: a binding (`() => ...` as a prop or child) reads game state
// directly, and the read is the subscription.

/** A component's value as a binding sees it. */
export type ReadValue<C> = ReadView<C>;

/** An entity a scene made (`ref={player}`), or `ref(entity)`. */
export interface NodeRef {
  /** The component's current value; inside a binding, the binding re-runs when it changes. */
  get<C extends abstract new () => unknown>(c: C): ReadValue<InstanceType<C>>;
  /** The entity, once mounted. */
  readonly entity: Entity | null;
}
export declare function ref(entity?: Entity): NodeRef;

/** UI-local state (a dropdown's open flag); game state lives in components. */
export declare function signal<T>(value: T, options?: { equals?: (a: T, b: T) => boolean }): [() => T, (next: T | ((prev: T) => T)) => T];
export declare function computed<T>(fn: () => T, options?: { equals?: (a: T, b: T) => boolean }): () => T;
/** Runs now (or, in a scene, once mounted) and again when what it read changes; returns a stop. */
export declare function effect(fn: () => void): () => void;
export declare function untrack<T>(fn: () => T): T;
export declare function onCleanup(fn: () => void): void;
export declare function batch<T>(fn: () => T): T;

type JsxElement = import("bevy-ts/jsx-runtime").Element;
type JsxChild = import("bevy-ts/jsx-runtime").Child;

/** Mounts its children while `when` is truthy, `fallback` otherwise. */
export declare function Show(props: { when: boolean | (() => unknown); fallback?: JsxChild | (() => JsxChild); children?: JsxChild | (() => JsxChild) }): JsxElement;
/** One subtree per item, kept by `key` across changes. */
export declare function For<T>(props: { each: readonly T[] | (() => readonly T[]); key?: (item: T, index: number) => string | number; children: (item: T, index: number) => JsxElement }): JsxElement;

export declare function px(n: number): string;
export declare function percent(n: number): string;
export declare function vw(n: number): string;
export declare function vh(n: number): string;
export declare function em(n: number): string;
export declare function rem(n: number): string;

type Length = import("bevy-ts/jsx-runtime").Length;
type Track = import("bevy-ts/jsx-runtime").Track;
type ColorStop = import("bevy-ts/jsx-runtime").ColorStop;
/** A flexible grid track, `GridTrack::fr`: `fr(1)`. */
export declare function fr(n: number): `${number}fr`;
/** A track between a minimum and a maximum size, `GridTrack::minmax`. */
export declare function minmax(min: Length | "min-content" | "max-content", max: Track): Track;
/** Tracks repeated `count` times, or as many as fit, `RepeatedGridTrack::repeat_many`. */
export declare function repeat(count: number | "auto-fill" | "auto-fit", tracks: Track | readonly Track[]): import("bevy-ts/jsx-runtime").RepeatedTrack;
/** A grid item spanning `n` tracks, `GridPlacement::span`. */
export declare function span(n: number): { readonly span: number };
/** Degrees to radians, for `transform.rotation`. */
export declare function deg(n: number): number;
/** `LinearGradient`: `angle` in degrees, 0 up and clockwise, as CSS's `linear-gradient`. */
export declare function linearGradient(angle: number, stops: readonly ColorStop[]): import("bevy-ts/jsx-runtime").Gradient;
/** `RadialGradient`, centred unless `at` (from the top left); `shape` is Bevy's `RadialGradientShape`. */
export declare function radialGradient(
  stops: readonly ColorStop[],
  options?: { at?: readonly [Length, Length]; shape?: "closest-side" | "farthest-side" | "closest-corner" | "farthest-corner" | { circle: Length } | { ellipse: readonly [Length, Length] } },
): import("bevy-ts/jsx-runtime").Gradient;
/** `ConicGradient`: stops are colours or `[colour, degrees]`; `from` is the start angle in degrees. */
export declare function conicGradient(stops: readonly (string | readonly [string, number])[], options?: { from?: number; at?: readonly [Length, Length] }): import("bevy-ts/jsx-runtime").Gradient;
