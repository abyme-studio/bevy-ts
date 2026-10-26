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
  | "array" | "timer" | "stopwatch";

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

// ---------- Timer and Stopwatch: Bevy's, ported; every Duration in seconds ----------

/** Bevy's `TimerMode`. */
export type TimerMode = "Once" | "Repeating";
/** A timer as a read view sees it: Bevy's getters. */
export interface ReadonlyTimer {
  readonly elapsed: f64;
  readonly elapsed_secs: f32;
  readonly duration: f64;
  readonly mode: TimerMode;
  readonly is_paused: boolean;
  readonly is_finished: boolean;
  readonly just_finished: boolean;
  readonly times_finished_this_tick: u32;
  readonly remaining: f64;
  readonly remaining_secs: f32;
  readonly fraction: f32;
  readonly fraction_remaining: f32;
}
/** Bevy's `Timer`, a field of a component, resource or `Local`: `cooldown = Timer.fromSeconds(2, "Repeating")`. */
export interface Timer extends ReadonlyTimer {
  /** Advances by `seconds`; returns the timer, as Bevy's `tick` does. */
  tick(seconds: number): this;
  pause(): void;
  unpause(): void;
  reset(): void;
  finish(): void;
  almostFinish(): void;
  setElapsed(seconds: number): void;
  setDuration(seconds: number): void;
  setMode(mode: TimerMode): void;
}
export declare const Timer: Field<"timer"> & {
  /** Bevy's `Timer::from_seconds`: `seconds` as an `f32`. */
  fromSeconds(seconds: number, mode: TimerMode): Timer;
  /** Bevy's `Timer::new`. */
  "new"(seconds: number, mode: TimerMode): Timer;
};
/** A stopwatch as a read view sees it. */
export interface ReadonlyStopwatch {
  readonly elapsed: f64;
  readonly elapsed_secs: f32;
  readonly is_paused: boolean;
}
/** Bevy's `Stopwatch`: `watch = Stopwatch.new()`. */
export interface Stopwatch extends ReadonlyStopwatch {
  tick(seconds: number): this;
  pause(): void;
  unpause(): void;
  reset(): void;
  setElapsed(seconds: number): void;
}
export declare const Stopwatch: Field<"stopwatch"> & { "new"(): Stopwatch };

/** A list field (`scores: u32[]`, a heap field) as a row gives it: a frozen array, assigned whole,
 * whose vector items are read-only vectors and object items read-only objects. A fixed-length
 * tuple (`array(u16, 3)`) stays itself. */
type HeapList<V> = V extends readonly (infer E)[] ? (number extends V["length"] ? readonly HeapItem<E>[] : V) : V;
type HeapItem<E> = E extends number | string | boolean ? E : E extends Vec3 ? ReadonlyVec3 : E extends Quat ? ReadonlyQuat : E extends object ? { readonly [K in keyof E]: HeapItem<E[K]> } : E;
/** Read access: every field readonly, vectors without mutating methods. */
export type ReadView<T> = {
  readonly [K in keyof T as K extends symbol ? never : K]: T[K] extends Vec3 ? ReadonlyVec3 : T[K] extends Quat ? ReadonlyQuat
    : T[K] extends Timer ? ReadonlyTimer : T[K] extends Stopwatch ? ReadonlyStopwatch : HeapList<T[K]>;
};
/** Write access: list fields are frozen arrays, assigned whole (`tag.scores = [1, 2]`). */
type MutView<T> = { [K in keyof T]: HeapList<T[K]> };
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
 * `reflect`: read and written through reflection: typed in TSX and the protocol, and query data
 * whose row's `value` is the whole value.
 */
export type Tier = "layout" | "reflect";
declare const tierBrand: unique symbol;
declare const valueBrand: unique symbol;
declare const holesBrand: unique symbol;
/** A reflect-tier class's whole value as its row's `value` sees it (an enum's, `Name`'s). */
export type ReflectValue<V> = { readonly [valueBrand]?: V };
/** A layout-tier class's holes: fields read and written through reflection, beside its columns. */
export type Holes<H> = { readonly [holesBrand]?: H };
type HolesOf<T> = T extends { readonly [holesBrand]?: infer H } ? NonNullable<H> : {};
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
  /**
   * `Health.of({ hp: 3 })`, Rust's `Health { hp: 3.0, ..default() }`. A Rust class's writable
   * holes (its `tsxProps`, `DirectionalLight.of({ color })`) are written through reflection.
   */
  static of<T, H = {}>(this: (new () => T) & { readonly tsxProps?: H }, init?: Init<T> & NoInfer<H>): T;
}
/**
 * A TS component: plain fields are columns. A `string` or `T[]` field (`T` a number kind, `bool`
 * or `string`) is a heap field, Rust's `String` or `Vec<T>` in the row: read and assigned whole
 * through the host (`tag.label = "door"`, `tag.scores = [1, 2]` on a `Mut<Tag>` row), a list a
 * frozen `readonly T[]`, and never a raw `q.tables` array.
 */
export declare abstract class Component extends Data {
  readonly [tierBrand]?: "layout";
  /**
   * Rust's `#[require(B, C = ...)]`: `static require = [Health, Shield.of({ v: 2 })]`. Inserting
   * this component, from any path, inserts each one the entity lacks, before the next system;
   * ones it has keep their values. Transitive; a cycle fails the load. A TS component or a Rust
   * component with a native layout, declared before this class.
   */
  static readonly require?: readonly (AnyComponentClass | object)[];
}
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
/** An Event. `entity`, `original`, `propagate`, `toJSON` and `event` are `On<E>`'s own: a field by those names fails the load. */
export declare abstract class Event extends Data { readonly [eventBrand]?: "global" }
/**
 * An EntityEvent: targets `entity`; `static propagate = true` bubbles it up `ChildOf`. A field named
 * `original`, `propagate`, `toJSON` or `event` fails the load: `On<E>` has its own.
 */
export declare abstract class EntityEvent extends Data {
  readonly [eventBrand]?: "entity";
  entity: Entity;
  static readonly propagate?: boolean;
}
/**
 * A colour, in any form bevy-ts takes: a hex string (`"#rgb"`, `"#rgba"`, `"#rrggbb"`,
 * `"#rrggbbaa"`), `[r, g, b, a?]` in sRGB, or Bevy's reflected `Color` (any variant; `alpha` is 1
 * when left out). `srgb()`, `linear()`, `hsl()` and `hex()` build these.
 */
export type ColorValue =
  | string
  | readonly [number, number, number]
  | readonly [number, number, number, number]
  | { readonly Srgba: Channels<"red" | "green" | "blue"> }
  | { readonly LinearRgba: Channels<"red" | "green" | "blue"> }
  | { readonly Hsla: Channels<"hue" | "saturation" | "lightness"> }
  | { readonly Hsva: Channels<"hue" | "saturation" | "value"> }
  | { readonly Hwba: Channels<"hue" | "whiteness" | "blackness"> }
  | { readonly Laba: Channels<"lightness" | "a" | "b"> }
  | { readonly Lcha: Channels<"lightness" | "chroma" | "hue"> }
  | { readonly Oklaba: Channels<"lightness" | "a" | "b"> }
  | { readonly Oklcha: Channels<"lightness" | "chroma" | "hue"> }
  | { readonly Xyza: Channels<"x" | "y" | "z"> }
  | { readonly Okhsla: Channels<"hue" | "saturation" | "lightness"> }
  | { readonly Okhsva: Channels<"hue" | "saturation" | "value"> }
  | { readonly Okhwba: Channels<"hue" | "whiteness" | "blackness"> };
type Channels<K extends string> = { readonly [P in K]: number } & { readonly alpha?: number };
/** An sRGB colour, channels 0 to 1: Bevy's `Color::srgba`. */
export declare function srgb(r: number, g: number, b: number, a?: number): ColorValue;
/** A linear RGB colour, channels 0 to 1: Bevy's `Color::linear_rgba`. */
export declare function linear(r: number, g: number, b: number, a?: number): ColorValue;
/** An HSL colour: hue in degrees, saturation and lightness 0 to 1. Bevy's `Color::hsla`. */
export declare function hsl(h: number, s: number, l: number, a?: number): ColorValue;
/** A hex colour (`"#f80"`, `"#ff8800cc"`). */
export declare function hex(s: string): ColorValue;
/** A component class: the handle in queries, its TSX tag and, through `of`, its values. */
export type ComponentClass = abstract new () => Component | ReflectComponent;
/** The name tsx's JSX types know a component class by. */
export type ComponentType<_ = any> = ComponentClass & { readonly componentName: string; readonly source: "ts" | "rust" };
/**
 * A reflect-tier Rust component by type path or short name: good in `With`/`Without` and TSX, and
 * spawned or inserted as `C.of(value)` (its JSON written over the component's default) or as the
 * bare class (its default).
 */
export type ReflectClass<V = unknown, F = {}> = (abstract new () => ReflectComponent & F) & {
  readonly componentName: string;
  readonly source: "rust";
  /** `Projection.of({ Orthographic: {} })`: the value written over the component's default. */
  of(value?: V): ReflectComponent;
};
export declare function reflectClass<V = unknown>(name: string): ReflectClass<V>;
/** `reflectClass` under its first name. */
export declare function rustComponent<V = unknown>(name: string): ReflectClass<V>;

// ---------- Rust messages and events (rust-events) ----------
// Named by reflect type path; values cross as JSON in reflect-coverage's shape. `bevy-ts/bevy`
// declares Bevy's; a project's own come from `app.ts_message`/`ts_event`/`ts_entity_event`.

declare const rustEnumBrand: unique symbol;
/** What a Rust message or event is in TS. */
export type RustKind = "message" | "event" | "entityEvent";
type RustBase<K extends RustKind> = K extends "message" ? Message : K extends "entityEvent" ? EntityEvent : Event;
/** A Rust message or event whose value is an enum in Bevy's shape: `of(value)` takes the value. */
export type RustEnum<K extends RustKind, V> = RustBase<K> & { readonly [rustEnumBrand]: V };
/** A Rust message or event that is a struct: its fields. */
export type RustStruct<K extends RustKind, V> = RustBase<K> & V;
/** A Rust message or event class: `static path` its type path, `of(value)` a value of it. */
export interface RustClass<T, V> {
  new (): T;
  readonly path: string;
  readonly componentName: string;
  readonly source: "rust";
  readonly native: true;
  of(value: V): T;
}
/** A generic Rust message or event (`AssetEvent<Image>` in a param): its classes by type argument. */
export interface RustGeneric<I> {
  readonly instances: I;
}
/** A message's or event's value as a reader or `On<E>.event` sees it. */
export type ValueOf<M> = M extends { readonly [rustEnumBrand]: infer V } ? (M extends { readonly [assetEventBrand]: unknown } ? V & AssetEventIs : V) : ReadView<M>;
/** What `MessageWriter<M>.write` takes beside an `M`. */
export type WriteValue<M> = M extends { readonly [rustEnumBrand]: infer V } ? V : Partial<M>;
/**
 * A project's Rust message or event class, to extend with its fields, so its name is a type and
 * a value: `class Scored extends rustMessage("game_native::Scored") { declare points: u32 }`.
 */
export interface RustBaseClass<T> {
  new (): T;
  readonly path: string;
  readonly componentName: string;
  readonly source: "rust";
  readonly native: true;
  of<C extends abstract new () => T>(this: C, value: WriteValue<InstanceType<C>>): InstanceType<C>;
}
/** A Rust message by type path. */
export declare function rustMessage(path: string): RustBaseClass<Message>;
/** A global Rust event by type path; `message` when it is a message too. */
export declare function rustEvent(path: string, message?: boolean): RustBaseClass<Event>;
/** A Rust `EntityEvent` by type path; `message` when it is a message too. */
export declare function rustEntityEvent(path: string, message?: boolean): RustBaseClass<EntityEvent>;
/** A Rust asset type, to spell a generic's argument (`AssetEvent<Image>`). */
export declare function rustAsset(path: string): abstract new () => unknown;
/** A generic Rust message or event, its instances by type argument as written. */
export declare function rustGeneric<I>(name: string, instances: I): RustGeneric<I>;
/**
 * A Rust component with a native layout registered in Rust (a port's `Velocity`):
 * `class Thrust extends nativeComponent("Thrust", { power: f32 }) {}`.
 */
export declare function nativeComponent<const S extends Schema>(name: string, fields: S): NativeClass<Component, S>;
/**
 * A Rust resource with a native layout (`GlobalAmbientLight`, a native crate's `Wind`):
 * `class Wind extends nativeResource("game_native::Wind", { speed: f32 }) {}`, for `Res<Wind>`.
 */
export declare function nativeResource<const S extends Schema>(name: string, fields: S): NativeClass<Resource, S>;
type NativeClass<Base, S extends Schema> = {
  new (): Base & { -readonly [K in keyof S]: S[K] extends Field<"vec2" | "vec3"> ? Vec3 : S[K] extends Field<"quat"> ? Quat : S[K] extends Field<"bool"> ? boolean : S[K] extends Field<"entity"> ? Entity : S[K] extends Field<"enum", infer E> ? E : number };
  readonly componentName: string;
  readonly source: "rust";
  readonly native: true;
  of<T>(this: new () => T, init?: Init<T>): T;
};

declare const assetBrand: unique symbol;
declare const assetPath: unique symbol;
/** An asset's fields `F`, tagged with its type path `P` so two assets' refs never type as each other. */
export type AssetFields<P extends string, F> = F & { readonly [assetPath]?: P };
/** A reflected asset type (`Mesh`): its fields `F` as the writer takes them. */
export interface AssetType<F = {}> {
  readonly typePath: string;
  readonly [assetBrand]?: F;
}
/** A reflected asset type with a default (`StandardMaterial`): `assets.add` takes it, and `of` makes values. */
export interface AssetClass<F = {}> extends AssetType<F> {
  /** An asset value: written into a `Handle<T>` slot it becomes one asset shared by every slot given equal props. */
  of(props?: AssetProps<F>): AssetValue<F>;
}
/** Props written over an asset: any fields, nested structs merged field by field. */
export type AssetProps<F> = {
  [K in keyof F]?: F[K] extends readonly unknown[] ? F[K] : F[K] extends { readonly $asset: number } | { readonly $of: string } | { readonly $load: string } ? F[K] : F[K] extends object ? AssetProps<F[K]> : F[K];
};
/** A script-made asset, from `assets.add`, for any `Handle<T>` slot. The ref goes stale when the script reloads or stops; the asset stays for a hot reload's entities, until a restart or stop. */
export interface AssetRef<F = unknown> {
  readonly $asset: number;
  readonly type: string;
  /** The asset's `AssetId<A>`, as an `AssetEvent<A>` carries it: one host call. Read it before `assets.drop`; the value stays valid after. */
  readonly id: AssetId<F>;
  readonly [assetBrand]?: F;
}
/** Bevy's `AssetId<A>` as JSON: compare two with `JSON.stringify`, not `===`. One from `ref.id` or an `AssetEvent<A>` knows its asset type, so `AssetEvent.isAdded` and `server.loadState` take it. */
export type AssetId<F = unknown> = ({ readonly Index: { readonly index?: unknown } } | { readonly Uuid: { readonly uuid?: string } }) & { readonly [assetBrand]?: F };
/** Bevy's `AssetEvent<A>` methods, on each value a `MessageReader<AssetEvent<A>>` gives: whether the event is that variant for that asset. An asset of another type throws. */
export interface AssetEventIs {
  isAdded(asset: AssetRef<any> | AssetId<any>): boolean;
  isModified(asset: AssetRef<any> | AssetId<any>): boolean;
  isRemoved(asset: AssetRef<any> | AssetId<any>): boolean;
  isUnused(asset: AssetRef<any> | AssetId<any>): boolean;
  isLoadedWithDependencies(asset: AssetRef<any> | AssetId<any>): boolean;
}
declare const assetEventBrand: unique symbol;
/** What `bevy-ts/bevy`'s `AssetEvent<A>` adds, so a reader's values have [`AssetEventIs`]. */
export type AssetEventOf<A> = { readonly [assetEventBrand]: A };
/** `T.of(props)`: a plain description of an asset, built when a `Handle<T>` slot is written with it. */
export interface AssetValue<F = unknown> {
  readonly $of: string;
  readonly props: AssetProps<F>;
  readonly [assetBrand]?: F;
}
/** `T.load(path, settings)`: an asset path loaded with loader settings, for a `Handle<T>` slot. */
export interface AssetLoad<F = unknown> {
  readonly $load: string;
  readonly settings?: object;
  readonly [assetBrand]?: F;
}
/** An asset type with loader settings (`Image`): `load` gives a path with them (docs/assets.md). */
export interface Loadable<F, S> {
  /** `Image.load("atlas.png", { sampler: { Descriptor: { address_mode_u: "Repeat", address_mode_v: "Repeat" } } })`. Unset sampler fields keep Bevy's default plugin sampler (linear filters); pixel art names `mag_filter: "Nearest"`. A path loaded again with other settings keeps the first. */
  load(path: string, settings?: S): AssetLoad<F>;
}
/** A Rust asset type by type path; `bevy-ts/bevy` declares every one Bevy reflects. */
export declare function asset<F = {}>(typePath: string): AssetClass<F>;
/** Builds, reads and edits reflected assets, inside a system's run, a TSX render or while the script loads (docs/assets.md). */
export declare const assets: {
  /** The asset from its type's default with `props` written over it. Visible to `get` at once, in `Assets<T>` when the run ends. */
  add<F>(type: AssetClass<F>, props?: AssetProps<F>): AssetRef<F>;
  /** An asset value (`T.of(...)`, a built `AnimationGraph`) made its own asset. */
  add<F>(value: AssetValue<F>): AssetRef<F>;
  /** The asset as JSON, or `undefined` when it is gone. One host call and a full read: keep values in JS rather than reading per frame. */
  get<F>(ref: AssetRef<F>): F | undefined;
  /** Writes `patch` over the asset's current value; a patch that changes nothing marks nothing changed. */
  set<F>(ref: AssetRef<F>, patch: AssetProps<F> | AssetValue<F>): void;
  /** Drops the script's handle: the ref goes stale, and Bevy frees the asset once nothing else holds it. A UUID asset (`assets.add`) stays until restart or stop. */
  drop(ref: AssetRef<unknown>): void;
  /** Bevy's `Assets::remove`: the asset goes when the run ends, even while a component holds it, and the ref goes stale. */
  remove(ref: AssetRef<unknown>): void;
};
/** Bevy's `AssetServer` resource, for `Res<AssetServer>`: an export stands for it (`ScriptAssetServer`). */
export interface AssetServer {
  readonly [assetsBrand]?: "AssetServer";
}
/** Bevy's `LoadState` (and its dependency forms), by variant name. */
export type LoadState = "NotLoaded" | "Loading" | "Loaded" | { readonly Failed: string };
declare const assetsBrand: unique symbol;
/** Bevy's `Assets<A>` resource, for `ResMut<Assets<Mesh>>`: an export stands for it (`Meshes`). */
export interface Assets<A> {
  readonly [assetsBrand]?: A;
}
/** Asset fields by Rust type path, for refs an exported method takes or returns. */
export interface AssetTypes {
  "bevy_mesh::mesh::Mesh": MeshData;
}
/** The ref type of the asset at type path `P`. */
export type AssetRefOf<P extends string> = AssetRef<P extends keyof AssetTypes ? AssetTypes[P] : unknown>;

/** Bevy's `PrimitiveTopology`. */
export declare const PrimitiveTopology: {
  readonly PointList: 0;
  readonly LineList: 1;
  readonly LineStrip: 2;
  readonly TriangleList: 3;
  readonly TriangleStrip: 4;
};
export type PrimitiveTopology = (typeof PrimitiveTopology)[keyof typeof PrimitiveTopology];
/** Bevy's `RenderAssetUsages` bits, combined with `|`. */
export declare const RenderAssetUsages: {
  readonly MAIN_WORLD: 1;
  readonly RENDER_WORLD: 2;
  /** Both worlds, Bevy's default. */
  default(): 3;
  all(): 3;
};
export type RenderAssetUsages = 1 | 2 | 3;
/** One of Bevy's built-in vertex attributes, `Mesh.ATTRIBUTE_*`. */
export interface MeshVertexAttribute<A extends Float32Array | Uint16Array = Float32Array | Uint16Array> {
  /** Bevy's name (`Vertex_Position`). */
  readonly name: string;
  readonly id: number;
  /** Bevy's `VertexFormat` (`Float32x3`). */
  readonly format: string;
  /** Numbers per vertex. */
  readonly components: number;
  readonly array: { new (n: number): A };
}
/** Bevy's `Indices`. */
export type Indices = { readonly U16: Uint16Array } | { readonly U32: Uint32Array };
export declare const Indices: {
  U16(data: Uint16Array): Indices;
  U32(data: Uint32Array): Indices;
};
/** A mesh as asset data: Bevy's names. */
export type MeshData = AssetFields<"bevy_mesh::mesh::Mesh", {
  readonly primitive_topology: keyof typeof PrimitiveTopology;
  readonly attributes: { readonly [name: string]: readonly (readonly number[])[] };
  readonly indices: { readonly U16: readonly number[] } | { readonly U32: readonly number[] } | null;
}>;
/** Bevy's `Mesh` API, on a `Mesh.new` mesh (recorded in JS until `meshes.add`) or a live one from `meshes.get`/`getMut`. Each method is one host call on a live mesh. */
export interface MeshApi {
  readonly primitiveTopology: keyof typeof PrimitiveTopology;
  /** The data, a `Float32Array` (a `Uint16Array` for `ATTRIBUTE_JOINT_INDEX`) of `attr.components` per vertex, copied once into Bevy's own buffer. */
  insertAttribute<A extends Float32Array | Uint16Array>(attr: MeshVertexAttribute<A>, data: A): this;
  removeAttribute(attr: MeshVertexAttribute): this;
  insertIndices(indices: Indices): this;
  removeIndices(): this;
  /** Smooth normals when indexed, flat otherwise; `TriangleList` only. */
  computeNormals(): this;
  /** Needs a `TriangleList` without indices. */
  computeFlatNormals(): this;
  /** Needs an indexed `TriangleList`. */
  computeSmoothNormals(): this;
  computeAreaWeightedNormals(): this;
  /** MikkTSpace tangents; needs positions, normals, `UV_0` and an indexed `TriangleList`. */
  generateTangents(): this;
  duplicateVertices(): this;
  countVertices(): number;
  /** A copy of the attribute's data, or `undefined` when unset. */
  attribute<A extends Float32Array | Uint16Array>(attr: MeshVertexAttribute<A>): A | undefined;
  /** A copy of the indices, or `undefined` when there are none. */
  indices(): Indices | undefined;
}
/** Bevy's `Mesh`. Add one with `meshes: ResMut<Assets<Mesh>>` (docs/assets.md). */
export declare class Mesh implements MeshApi {
  private constructor();
  static readonly typePath: "bevy_mesh::mesh::Mesh";
  static readonly [assetBrand]?: MeshData;
  /** A mesh value from Bevy-named JSON (the shape `assets.get` reads): in a `Handle<Mesh>` slot it is one asset shared by every slot given equal props. */
  static of(props?: AssetProps<MeshData>): AssetValue<MeshData>;
  /** `Mesh::new(topology, usage)`: records its calls in JS until `meshes.add`. */
  static new(topology: PrimitiveTopology, usage?: RenderAssetUsages): Mesh;
  static readonly ATTRIBUTE_POSITION: MeshVertexAttribute<Float32Array>;
  static readonly ATTRIBUTE_NORMAL: MeshVertexAttribute<Float32Array>;
  static readonly ATTRIBUTE_UV_0: MeshVertexAttribute<Float32Array>;
  static readonly ATTRIBUTE_UV_1: MeshVertexAttribute<Float32Array>;
  static readonly ATTRIBUTE_TANGENT: MeshVertexAttribute<Float32Array>;
  static readonly ATTRIBUTE_COLOR: MeshVertexAttribute<Float32Array>;
  static readonly ATTRIBUTE_JOINT_WEIGHT: MeshVertexAttribute<Float32Array>;
  static readonly ATTRIBUTE_JOINT_INDEX: MeshVertexAttribute<Uint16Array>;
  readonly primitiveTopology: keyof typeof PrimitiveTopology;
  insertAttribute<A extends Float32Array | Uint16Array>(attr: MeshVertexAttribute<A>, data: A): this;
  removeAttribute(attr: MeshVertexAttribute): this;
  insertIndices(indices: Indices): this;
  removeIndices(): this;
  computeNormals(): this;
  computeFlatNormals(): this;
  computeSmoothNormals(): this;
  computeAreaWeightedNormals(): this;
  generateTangents(): this;
  duplicateVertices(): this;
  /** Throws after a compute call: Rust makes computed data at `meshes.add`. */
  countVertices(): number;
  attribute<A extends Float32Array | Uint16Array>(attr: MeshVertexAttribute<A>): A | undefined;
  indices(): Indices | undefined;
}
/** A mesh in `Assets<Mesh>`, from `meshes.get` (read-only) or `meshes.getMut`. */
export interface LiveMesh extends MeshApi {
  /** The `RenderAssetUsages` bits. */
  readonly assetUsage: RenderAssetUsages;
}

/** The scene component a level names (`enemies::Grunt` or `src/enemies.tsx#Grunt`), for tsx's expand entry. */
export declare function sceneComponent(nameOrId: string): (abstract new () => SceneComponent) & typeof SceneComponent | undefined;

declare const stateBrand: unique symbol;
export interface StateValue<N extends string = string, V extends string = string> { readonly state: N; readonly value: V }
/** `class Game extends States("Menu", "Playing") {}`; `Game.Playing` is a value. */
export declare function States<const V extends string>(...values: V[]):
  & { new (): { readonly [stateBrand]: { values: V } }; readonly path?: string }
  & { readonly [K in V]: StateValue<string, K> };
/** `class Pause extends SubStates(Game.Playing, "Running", "Paused") {}`, as Bevy's `SubStates`: exists only while its source is in that value, at its first value on entering. */
export declare function SubStates<const V extends string>(source: StateValue, ...values: V[]):
  & { new (): { readonly [stateBrand]: { values: V } }; readonly path?: string }
  & { readonly [K in V]: StateValue<string, K> };
/** `class InGame extends ComputedStates(Game, (g) => ..., "Yes") {}`, as Bevy's `ComputedStates`: computed from its sources (an array for several), `undefined` when it does not exist. */
export declare function ComputedStates<const V extends string>(
  sources: (abstract new () => unknown) | readonly (abstract new () => unknown)[],
  compute: (...sources: (StateValue | undefined)[]) => StateValue | undefined,
  ...values: V[]
): & { new (): { readonly [stateBrand]: { values: V } }; readonly path?: string } & { readonly [K in V]: StateValue<string, K> };
/** A Rust state TS names (`app.ts_state::<S>()`), imported from `bevy-ts/states`: `EngineMode.Play`. */
export type RustState<V extends string> = (abstract new () => { readonly [stateBrand]: { values: V } }) & { readonly [K in V]: StateValue<string, K> };
/** Builds a `bevy-ts/states` class; the generated module calls it. */
export declare function rustState<const V extends string>(path: string, values: readonly V[]): RustState<V>;
type StateValueOf<G> = G extends { readonly [stateBrand]: { values: infer V extends string } }
  ? { [K in V]: StateValue<string, K> }[V]
  : never;

// ---------- System param types (read by the compile step) ----------

declare const mutBrand: unique symbol;
declare const optBrand: unique symbol;
declare const refBrand: unique symbol;
declare const hasBrand: unique symbol;
declare const anyOfBrand: unique symbol;
declare const singleBrand: unique symbol;
declare const filterBrand: unique symbol;
declare const resBrand: unique symbol;
export type Mut<T> = { readonly [mutBrand]: T };
/** Query data `Option<C>`; a system param `Option<Res<R>>` is `undefined` when `R` is absent, and
 * `Option<Single<...>>` is zero or one match. */
export type Option<T> = T extends { readonly [resBrand]: true } ? T | undefined : T extends { readonly [singleBrand]: true } ? T | undefined : OptionData<T>;
type OptionData<T> = { readonly [optBrand]: T };
/** Read access plus the row's change ticks, as Bevy's `Ref<T>`: `isAdded()`, `isChanged()`, `lastChanged()`. */
export type Ref<T> = { readonly [refBrand]: T };
/** Whether the row has `T`, as Bevy's `Has<T>`: a `boolean`, no access to `T`, never filters. */
export type Has<T> = { readonly [hasBrand]: T };
/** Each part as `Option`, and at least one present, as Bevy's `AnyOf<(A, B)>`. */
export type AnyOf<Ts extends readonly unknown[]> = { readonly [anyOfBrand]: Ts };
export type With<T> = { readonly [filterBrand]: ["with", T] };
export type Without<T> = { readonly [filterBrand]: ["without", T] };
/** Rows whose component changed since this system last ran (bridge per-row ticks). */
export type Changed<T> = { readonly [filterBrand]: ["changed", T] };
export type Added<T> = { readonly [filterBrand]: ["added", T] };
/** Entities spawned since this system last ran, as Bevy's `Spawned`. */
export type Spawned = { readonly [filterBrand]: ["spawned"] };
/** Any of the filters, as Bevy's `Or<(F1, F2)>`: `Or<[Changed<Health>, Added<Shield>]>`. */
export type Or<Fs extends readonly unknown[]> = { readonly [filterBrand]: ["or", Fs] };

/** The change ticks a `Ref<T>` row carries. */
export interface RefTicks {
  /** Added since this system last ran (Bevy's `Ref::is_added`). */
  isAdded(): boolean;
  /** Changed (or added) since this system last ran (Bevy's `Ref::is_changed`). */
  isChanged(): boolean;
  /** The tick it last changed at (Bevy's `Ref::last_changed`). */
  lastChanged(): number;
}

// A reflect-tier class's row reads through reflection: `value` is the whole value, and a
// struct's fields are on the row too (`weapon.name`). Unchanged rows are cached across runs.
// A layout-tier class's row has its holes beside its columns.
type ReflectRow<T> = T & {
  value: T extends { readonly [valueBrand]?: infer V } ? (unknown extends V ? FieldsOf<T> : V) : FieldsOf<T>;
};
type FieldsOf<T> = T extends { value: infer V } ? V : Omit<T, typeof tierBrand>;
type LayoutRow<T> = ReadView<T> & Readonly<HolesOf<T>>;
type Item<D> =
  D extends Entity ? Entity
  : D extends OptionData<infer X> ? Item<X> | undefined
  : D extends Has<unknown> ? boolean
  : D extends Ref<infer T> ? (T extends ReflectComponent ? ReadView<ReflectRow<T>> : LayoutRow<T>) & RefTicks
  : D extends AnyOf<infer Ts> ? { -readonly [I in keyof Ts]: Item<Ts[I]> | undefined }
  : D extends Mut<infer T> ? (T extends ReflectComponent ? ReflectRow<T> : MutView<T> & HolesOf<T>)
  : D extends abstract new (...a: never) => infer I ? (I extends ReflectComponent ? ReadView<ReflectRow<I>> : LayoutRow<I>) // value form: Mut(C) / C
  : D extends ReflectComponent ? ReadView<ReflectRow<D>>
  : LayoutRow<D>;
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
  /** The table's length, not the match count: with `rows` set, only those rows match. */
  readonly len: number;
  /** (index, generation) pairs. */
  readonly entities: Uint32Array;
  /**
   * The rows that match, or `null` when every row does. Set for `Changed`/`Added`/`Spawned`, and
   * for any query naming a sparse-set component (filter or `Has`): such a query hands one table
   * per matched archetype, so one table can appear more than once, each time with its own rows.
   */
  readonly rows: Uint32Array | null;
  entity(row: number): Entity;
  read(c: ComponentClass): ColumnArrays;
  /** A `Mut` component's arrays; every row of this table the query matched is marked changed after the run. */
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
  /** Whether `e` matches (Bevy's `contains`). */
  contains(e: Entity): boolean;
  /** Whether nothing matches (Bevy's `is_empty`). */
  isEmpty(): boolean;
  /** The rows of the listed entities that match, in list order (Bevy's `iter_many`). */
  iterMany(entities: Iterable<Entity>): IterableIterator<R>;
  /** The rows of the listed entities; throws naming the first that does not match (Bevy's `get_many`). */
  getMany(entities: Iterable<Entity>): R[];
  /** Every set of `k` distinct rows, once each (Bevy's `iter_combinations`). */
  iterCombinations(k: number): IterableIterator<R[]>;
  /** With `ChildOf` data: `e`'s parent, then its parent, up to the root (Bevy's `iter_ancestors`). */
  iterAncestors(e: Entity): IterableIterator<Entity>;
  /** With `Children` data: `e`'s children, then theirs, breadth first; not `e` (Bevy's `iter_descendants`). */
  iterDescendants(e: Entity): IterableIterator<Entity>;
  readonly len: number;
  /** This run's change tick, for `split` tables. */
  readonly tick: number;
  readonly tables: readonly Table[];
}
/** `Query<[Enemy, Mut<Transform>], [With<Alive>, Without<Player>]>`, as Bevy's `Query<(&Enemy, &mut Transform), (With<Alive>, Without<Player>)>`. */
export type Query<Ds extends readonly unknown[], _Filter = never> = QueryOf<Row<Ds>>;
/**
 * Exports that stand for a `Res<..>` or `ResMut<..>` spelling (`stands_for` on `#[export]`), keyed by
 * the spelling, each `[T, Export]`. `bevy-ts types` fills them in `native.d.ts`, so
 * `ResMut<Time<Virtual>>` is typed as the export that claims it.
 */
export interface ResClaims {}
export interface ResMutClaims {}
/** The export claiming `T` in `C`, or `Else`. */
type Claimed<C, T, Else> = {
  [K in keyof C]: C[K] extends readonly [infer S, infer E] ? ([T] extends [S] ? ([S] extends [T] ? E : never) : never) : never;
}[keyof C] extends infer R
  ? [R] extends [never] ? Else : R
  : Else;
/** The brand (never read) lets `Option` tell a resource param from query data. */
export type Res<T> = Claimed<ResClaims, T, ReadView<T> & { readonly [resBrand]: true }>;
export type ResMut<T> = Claimed<ResMutClaims, T, T & { readonly [resBrand]: true }>;
/**
 * Exactly one match, given as that row, as Bevy's `Single<D, F>`: with none or several the system
 * is skipped (no call, no error). `Option<Single<...>>`: zero or one, the row or `undefined`.
 */
export type Single<Ds extends readonly unknown[], _Filter = never> = Row<Ds> & { readonly [singleBrand]: true };
/** A query with at least one match, as Bevy's `Populated<D, F>`: with none the system is skipped. */
export type Populated<Ds extends readonly unknown[], _Filter = never> = QueryOf<Row<Ds>>;
/** Entities that lost `C` (removed or despawned) since this system last read it, as Bevy's `RemovedComponents<C>`. */
export interface RemovedComponents<_C> extends Iterable<Entity> {
  each(f: (e: Entity) => void): void;
  readonly len: number;
  isEmpty(): boolean;
}
/** This run's tick and the system's last-run tick, as Bevy's `SystemChangeTick`. */
export interface SystemChangeTick {
  readonly thisRun: number;
  readonly lastRun: number;
}
/** Per-system state with a layout, `Local<{ timer: f32; wave: u32 }>`. Kept in Rust under the system's id; survives reloads. */
export type Local<T> = T;
/** A Rust message's reader hands plain values, its JSON parsed once per run. */
export interface MessageReader<M> extends Iterable<ValueOf<M>> {
  each(f: (m: ValueOf<M>) => void): void;
  readonly len: number;
}
export interface MessageWriter<M> {
  write(value: M | WriteValue<M>): void;
  /** Allocation-free: a writable view on a fresh slot. A TS message only. */
  next(): M;
}
/** A one-shot system's input, first param only: an `Event` value `commands.runSystemWith` passed. */
export type In<E> = ReadView<E>;
/**
 * Bevy's `NextState<S>`. `set` always transitions, even to the current value: `OnExit` and
 * `OnEnter` run again and `DespawnOnExit` entities go. `setIfNeq` does nothing when `v` is the
 * current value (Bevy's `set_if_different`).
 */
export interface NextState<G> {
  set(v: StateValueOf<G>): void;
  setIfNeq(v: StateValueOf<G>): void;
}
export interface State<G> { readonly current: StateValueOf<G> }
/** Bevy's `PreviousState<S>`: the value before the machine's last transition, `undefined` before the first. */
export interface PreviousState<G> { readonly previous: StateValueOf<G> | undefined }

declare const stateScopedBrand: unique symbol;
/** A state-scoped component's value, `DespawnOnExit.of(Game.Playing)`, for `spawn`/`insert`. */
export interface StateScopedValue { readonly [stateScopedBrand]: true }
/**
 * Bevy's state-scoped component of a TS state value, run by Bevy's own systems: despawn, insert
 * `Disabled` or remove it on the named exit or enter. `DespawnOnExit.of(Game.Playing)` in
 * commands, `<DespawnOnExit value={Game.Playing} />` in TSX, `DespawnOnExit { value: "game::Game.Playing" }`
 * in a level.
 */
export interface StateScoped {
  readonly __stateScoped: true;
  of<N extends string, V extends string>(value: StateValue<N, V>): StateScopedValue;
}
export declare const DespawnOnExit: StateScoped;
export declare const DespawnOnEnter: StateScoped;
export declare const DisableOnExit: StateScoped;
export declare const DisableOnEnter: StateScoped;
export declare const EnableOnExit: StateScoped;
export declare const EnableOnEnter: StateScoped;

type Part = Component | ReflectComponent | ComponentClass;
export interface Commands {
  /**
   * Components (`Health.of({ hp: 3 })`, or a class for its defaults) and at most one element
   * (`<Grunt speed={3} />`), whose root `<entity>` is the entity returned, as Bevy's
   * `spawn_scene` returns the root: the components go on that root, its children under it. A
   * fragment throws.
   */
  spawn(...parts: readonly (Part | Element | StateScopedValue)[]): Entity;
  /** One entity per part list, in one op, as Bevy's `spawn_batch`; the entities in order. */
  spawnBatch(list: Iterable<readonly Part[]>): Entity[];
  /** `[entity, ...parts]` each, in one op, as Bevy's `insert_batch`. */
  insertBatch(list: Iterable<readonly [Entity, ...Part[]]>): void;
  entity(e: Entity): EntityCommands;
  /** As Rust's `commands.trigger(Explode { entity, radius })`: an entity event carries its target. */
  trigger(event: EntityEvent | Event): void;
  write(message: Message): void;
  /** Spawns through a native Rust recipe a plugin registers (`add_ts_native_recipe`). */
  spawnNative(recipe: string, params?: unknown): Entity;
  /** Mounts an element tree under the tsx runtime root. */
  mount(element: Element, options?: { parent?: Entity | null }): void;
  /** Unmounts a runtime-mounted node by key, or any tsx node by entity. */
  unmount(target: string | Entity): void;
  /** Appends an op of a Rust plugin's command kind. */
  custom(kind: string, payload: Uint8Array | string | object): void;
  /** Runs a schedule once when these commands apply, as Bevy's `Commands::run_schedule`. */
  runSchedule(label: ScheduleRef): void;
  /** Runs a one-shot system (its id, or the registered function) when these commands apply. */
  runSystem(target: SystemId | SystemFn): void;
  /** As `runSystem`, with its `In<E>` input. */
  runSystemWith<E extends Event>(target: SystemId | ((input: In<E>, ...params: never[]) => void), input: E): void;
  /**
   * Saves the world to `<name>.scn.ron` in the save folder (docs/saves.md). `name` is 1 to 64 of
   * `A-Z a-z 0-9 _ -`; anything else throws. A `WorldSaved` message says when it is written.
   */
  saveWorld(name: string, options?: SaveWorldOptions): void;
  /** Loads a save: its entities are spawned fresh, under `parent` if given. A `WorldLoaded` message says when. */
  loadWorld(name: string, options?: LoadWorldOptions): void;
}
export interface SaveWorldOptions {
  /** The entities to save: every match of a query, or a list. By default, those with `Save`. */
  entities?: QueryOf<unknown[]> | readonly Entity[];
  /** Only these components (`Save` always goes too). */
  allow?: readonly ComponentClass[];
  /** Not these components, after `allow`. */
  deny?: readonly ComponentClass[];
  /** The resources to save, Rust or TS. None by default. */
  resources?: readonly (abstract new (...a: never) => unknown)[];
}
export interface LoadWorldOptions {
  /** Makes each loaded entity without a parent a child of this one. */
  parent?: Entity | null;
}
/** Bevy's `EntityCommands`; each method applied in order after the run, under Bevy's name. */
export interface EntityCommands {
  /** Components, and at most one element whose root components land on this entity, its children under it. */
  insert(...parts: readonly (Part | Element | StateScopedValue)[]): EntityCommands;
  /** Keeps a value already there (Bevy's `insert_if_new`). */
  insertIfNew(...parts: readonly Part[]): EntityCommands;
  /** Does nothing if the entity is gone (Bevy's `try_insert`). */
  tryInsert(...parts: readonly Part[]): EntityCommands;
  /** On this entity and every descendant through `Children` (Bevy's `insert_recursive`). */
  insertRecursive(...parts: readonly Part[]): EntityCommands;
  remove(...cs: readonly (ComponentClass | StateScoped)[]): EntityCommands;
  /** With every component they require (Bevy's `remove_with_requires`). */
  removeWithRequires(...cs: readonly ComponentClass[]): EntityCommands;
  /** Removes every other component (Bevy's `retain`). */
  retain(...cs: readonly ComponentClass[]): EntityCommands;
  /** From this entity and every descendant through `Children` (Bevy's `remove_recursive`). */
  removeRecursive(...cs: readonly ComponentClass[]): EntityCommands;
  /** Removes every component; the entity stays (Bevy's `clear`). */
  clear(): EntityCommands;
  /** A new entity with copies of this one's components, returned at once (Bevy's `clone_and_spawn`). */
  cloneAndSpawn(): Entity;
  setParent(parent: Entity | null): EntityCommands;
  /**
   * Attaches an observer to this entity alone, as Bevy's `EntityCommands::observe`: an exported
   * function whose first param is `On<E>` of an `EntityEvent`. It runs only for triggers whose
   * current target is this entity, bubbling hops included, and goes when the entity is despawned.
   */
  observe<E extends EntityEvent | { readonly [lifecycleBrand]: unknown }>(observer: (on: On<E>, ...params: never[]) => void): EntityCommands;
  /** Bevy's `add_children`. */
  addChildren(children: Iterable<Entity>): EntityCommands;
  /** `parent.spawn(...)` spawns a child of this entity (Bevy's `with_children`). */
  withChildren(f: (parent: { spawn(...parts: readonly (Part | Element)[]): Entity }) => void): EntityCommands;
  /** Bevy's `detach_all_children`. */
  detachChildren(): EntityCommands;
  /** Bevy's `despawn_children`. */
  despawnChildren(): EntityCommands;
  /** With its children (`ChildOf` is a linked spawn). */
  despawn(): void;
  readonly id: Entity;
}

// ---------- Observers ----------

declare const lifecycleBrand: unique symbol;
export type Add<C> = { readonly [lifecycleBrand]: ["add", C] };
export type Insert<C> = { readonly [lifecycleBrand]: ["insert", C] };
export type Remove<C> = { readonly [lifecycleBrand]: ["remove", C] };
/** Before a value of `C` is replaced or removed (Bevy 0.20's `Discard`). */
export type Discard<C> = { readonly [lifecycleBrand]: ["discard", C] };
/** An entity with `C` is despawned (Bevy's `Despawn`). */
export type Despawn<C> = { readonly [lifecycleBrand]: ["despawn", C] };
declare const onBrand: unique symbol;
/** `On<E>`'s own members; a Rust event's field by one of these names is read on `event`. */
type OnOwn = "entity" | "original" | "propagate" | "toJSON" | "event";
/**
 * An observer's first parameter, as Bevy's `On<Explode>` / `On<Add<Health>>`: the event's fields
 * directly (`explode.radius`, as Rust's `On` derefs to the event), the current target as `entity`,
 * the first target of a bubbling event as `original` (Bevy's `original_event_target`),
 * `propagate`, and the whole value as `event` (Bevy's `On::event()`). A TS event's fields may not
 * be named `entity` (bar an EntityEvent's target), `original`, `propagate`, `toJSON` or `event`;
 * a Rust event's field by those names is read as `on.event.<name>`.
 * `Discard`, `Remove` and `Despawn` give the component's fields as they were when the event fired
 * (TS observers run later, when the value is gone).
 */
export type On<E> =
  & (E extends { readonly [lifecycleBrand]: ["discard" | "remove" | "despawn", infer C] } ? ReadView<C> & { readonly entity: Entity; readonly original: Entity }
    : E extends { readonly [lifecycleBrand]: unknown } ? { readonly entity: Entity; readonly original: Entity }
    : Omit<ValueOf<E> extends object ? ValueOf<E> : {}, OnOwn> & { readonly entity: Entity; readonly original: Entity; readonly event: ValueOf<E> })
  & { /** Stop bubbling. */ propagate(on: boolean): void; /** Never read: lets `observe` infer `E`. */ readonly [onBrand]?: E };
// ---------- Systems, schedules, ordering ----------

/** A system: an exported function whose parameter types the compile step read. */
export type SystemFn = (...params: never[]) => void;
type ResourceClass = abstract new (...a: never) => Resource;
type AnyComponentClass = abstract new (...a: never) => unknown;
/**
 * A run condition, Bevy's under its name, evaluated in Rust: a skipped system costs no JS call.
 * The short-circuit combinators skip `b` when `a` decides; the eager ones and `xor`/`xnor`
 * always evaluate both, so a stateful `b` advances.
 */
export interface Condition {
  readonly condition: true;
  andThen(b: ConditionLike): Condition;
  orElse(b: ConditionLike): Condition;
  nandThen(b: ConditionLike): Condition;
  norElse(b: ConditionLike): Condition;
  xor(b: ConditionLike): Condition;
  xnor(b: ConditionLike): Condition;
  andEager(b: ConditionLike): Condition;
  orEager(b: ConditionLike): Condition;
  nandEager(b: ConditionLike): Condition;
  norEager(b: ConditionLike): Condition;
}
/**
 * A TS predicate: an exported function whose params only read (`Res`, `Query` without `Mut`,
 * `Local`, `MessageReader`, `State`) and which returns `boolean`, as `.runIf(bossReady)`. Each
 * evaluation is one call into JS, about 3 µs, every frame its stage runs; a stage with no other
 * TS system pays about 20 µs more to enter JS at all. Put Rust conditions first
 * (`inState(Game.Playing).andThen(bossReady)`) so frames they rule out cost no call, or test the
 * params in the system and return early instead.
 */
export type Predicate = (...params: never[]) => boolean;
/** A condition or a predicate function. */
export type ConditionLike = Condition | Predicate;
export declare function inState<N extends string, V extends string>(v: StateValue<N, V>): Condition;
export declare function onMessage(m: abstract new (...a: never) => unknown): Condition;
/** Bevy's `state_changed`: the machine transitioned since this condition last ran. */
export declare function stateChanged(s: abstract new (...a: never) => unknown): Condition;
/** Bevy's `state_exists`: the machine has a value (`initState` gave one, or a sub state's source allows it). */
export declare function stateExists(s: abstract new (...a: never) => unknown): Condition;
export declare function not(c: ConditionLike): Condition;
/** True the first time it is evaluated. */
export declare function runOnce(): Condition;
export declare function resourceExists(r: abstract new (...a: never) => unknown): Condition;
/** `pred` gates only while the resource exists: `resourceExists(R).andThen(pred)`, `pred` taking `Res<R>` first. */
export declare function resourceExistsAnd(r: abstract new (...a: never) => unknown, pred: Predicate): Condition;
export declare function resourceAdded(r: ResourceClass): Condition;
/** The resource changed since this condition last ran; false while it does not exist. */
export declare function resourceChanged(r: ResourceClass): Condition;
export declare function resourceExistsAndChanged(r: ResourceClass): Condition;
/** True once, the first evaluation after the resource stopped existing. */
export declare function resourceRemoved(r: ResourceClass): Condition;
export declare function resourceChangedOrRemoved(r: ResourceClass): Condition;
/** The resource's bytes equal the value's (`Score.of({ value: 10 })`), defaults filled in. */
export declare function resourceEquals(value: Resource): Condition;
export declare function resourceExistsAndEquals(value: Resource): Condition;
/** Some entity has the component. */
export declare function anyWithComponent(c: AnyComponentClass): Condition;
/** The component was removed from some entity since this condition last ran. */
export declare function anyComponentRemoved(c: AnyComponentClass): Condition;
declare const filterValueBrand: unique symbol;
/** A filter value for `anyMatchFilter`, named as the filter types. */
export interface FilterValue { readonly [filterValueBrand]: true }
export declare function With(c: AnyComponentClass): FilterValue;
export declare function Without(c: AnyComponentClass): FilterValue;
export declare function Added(c: AnyComponentClass): FilterValue;
export declare function Changed(c: AnyComponentClass): FilterValue;
/** Some entity matches every filter (`Changed`/`Added` against this condition's last run). */
export declare function anyMatchFilter(...filters: readonly FilterValue[]): Condition;
/** True when `c`'s value differs from its last evaluation (false before the first). */
export declare function conditionChanged(c: ConditionLike): Condition;
/** True when `c`'s value changed to `to`. */
export declare function conditionChangedTo(c: ConditionLike, to: boolean): Condition;
/** A condition a Rust crate exposed with `app.ts_condition(name, factory)`; ports export typed wrappers. */
export declare function nativeCondition(name: string, args?: unknown): Condition;
/** Bevy's `on_timer`: true each time `seconds` pass on the schedule's clock (`Time`; the fixed clock in `FixedUpdate`). */
export declare function onTimer(seconds: number): Condition;
/** Bevy's `on_real_timer`: true each time `seconds` pass on `Time<Real>`, paused or not. */
export declare function onRealTimer(seconds: number): Condition;
/** Bevy's `once_after_delay`: true once, when `seconds` have passed on the schedule's clock. */
export declare function onceAfterDelay(seconds: number): Condition;
/** Bevy's `once_after_real_delay`: true once, when `seconds` have passed on `Time<Real>`. */
export declare function onceAfterRealDelay(seconds: number): Condition;
/** Bevy's `repeating_after_delay`: false until `seconds` pass on the schedule's clock, then true. */
export declare function repeatingAfterDelay(seconds: number): Condition;
/** Bevy's `repeating_after_real_delay`: false until `seconds` pass on `Time<Real>`, then true. */
export declare function repeatingAfterRealDelay(seconds: number): Condition;
/** Bevy's `paused`: `Time<Virtual>` is paused. */
export declare function paused(): Condition;

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
  /** Bevy's collective `run_if`: evaluated once per stage run, before the group's first member. A predicate here is one call per stage run (about 3 µs; see `Predicate`). */
  runIf(c: ConditionLike): SystemConfig;
  /** Bevy's `distributive_run_if`: each member gets its own instance, so a predicate is one call per member. */
  distributiveRunIf(c: ConditionLike): SystemConfig;
  inSet(s: SetRef): SystemConfig;
}
export declare function systems(...s: readonly Systemish[]): SystemConfig;
/** Each runs after the previous, as Bevy's `.chain()`. */
export declare function chain(...s: readonly Systemish[]): SystemConfig;

export interface Schedule { readonly schedule: string }
export declare const PreStartup: Schedule;
export declare const Startup: Schedule;
export declare const PostStartup: Schedule;
export declare const First: Schedule;
export declare const PreUpdate: Schedule;
export declare const RunFixedMainLoop: Schedule;
export declare const Update: Schedule;
export declare const SpawnScene: Schedule;
export declare const PostUpdate: Schedule;
export declare const Last: Schedule;
export declare const FixedFirst: Schedule;
export declare const FixedPreUpdate: Schedule;
export declare const FixedUpdate: Schedule;
export declare const FixedPostUpdate: Schedule;
export declare const FixedLast: Schedule;
export declare function OnEnter<N extends string, V extends string>(v: StateValue<N, V>): Schedule;
export declare function OnExit<N extends string, V extends string>(v: StateValue<N, V>): Schedule;
/** Bevy's `OnTransition { exited, entered }`: runs on exactly that transition, between exit and enter work. */
export declare function OnTransition<N extends string>(t: { exited: StateValue<N>; entered: StateValue<N> }): Schedule;
/** `class Waves extends ScheduleLabel {}`, as Rust's `#[derive(ScheduleLabel)] struct Waves;`. Named like a component; `bevy_ts::TsSchedule` in Rust. */
export declare abstract class ScheduleLabel { static readonly path?: string; protected readonly __schedule?: true }
/** A custom schedule: a `ScheduleLabel` class, or one a Rust crate exposed. */
export type ScheduleRef = (abstract new () => ScheduleLabel) | Schedule;
/** A schedule a Rust crate exposed by name (`schedule("net:tick")`). */
export declare function schedule(name: string): Schedule;
declare const systemIdBrand: unique symbol;
/** A one-shot system's handle, from `app.registerSystem`, as Bevy's `SystemId`. */
export interface SystemId { readonly [systemIdBrand]: true }

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
export declare function local<const S extends Schema>(fields: S): { -readonly [K in keyof S]: S[K] extends Field<"vec2" | "vec3"> ? Vec3 : S[K] extends Field<"quat"> ? Quat : S[K] extends Field<"bool"> ? boolean : S[K] extends Field<"timer"> ? Timer : S[K] extends Field<"stopwatch"> ? Stopwatch : number };

// ---------- Rust types bevy-ts ships typed ----------

export declare class Transform extends Component {
  static readonly path: "bevy_transform::components::transform::Transform";
  static readonly native: true;
  /**
   * A `Transform` at `eye` looking at `target`, `up` defaulting to `[0, 1, 0]`: Bevy's
   * `Transform::from_translation(eye).looking_at(target, up)`, computed in JS.
   */
  static lookingAt(eye: readonly [number, number, number], target: readonly [number, number, number], up?: readonly [number, number, number]): Transform;
  declare readonly translation: Vec3;
  declare readonly rotation: Quat;
  declare readonly scale: Vec3;
}
/** Marks an entity `commands.saveWorld` saves by default; saved itself, so a loaded entity has it again. */
export declare class Save extends Component {
  static readonly path: "bevy_ts::saves::Save";
  static readonly native: true;
}
/** Bevy's clock markers: `Res<Time<Real>>`, `Res<Time<Virtual>>`, `Res<Time<Fixed>>`. */
export declare class Real { private readonly clock: "Real" }
export declare class Virtual { private readonly clock: "Virtual" }
export declare class Fixed { private readonly clock: "Fixed" }
/** What every clock reads: Bevy's getters, every `Duration` in seconds. */
export interface ClockView {
  readonly delta_secs: f32;
  readonly elapsed_secs: f32;
  readonly elapsed_secs_wrapped: f32;
  readonly delta_secs_f64: f64;
  readonly elapsed_secs_f64: f64;
  readonly elapsed_secs_wrapped_f64: f64;
  readonly wrap_period: f64;
}
/** `Time<Virtual>`: the game clock. Read-only from TS until clock writes land (docs/time.md). */
export interface VirtualClockView extends ClockView {
  readonly is_paused: boolean;
  readonly was_paused: boolean;
  readonly relative_speed: f32;
  readonly relative_speed_f64: f64;
  readonly effective_speed: f32;
  readonly effective_speed_f64: f64;
  readonly max_delta: f64;
}
/** `Time<Fixed>`: the fixed-step clock. */
export interface FixedClockView extends ClockView {
  readonly timestep: f64;
  readonly overstep: f64;
  readonly overstep_fraction: f32;
  readonly overstep_fraction_f64: f64;
}
/**
 * Bevy's `Time<C>`, a read-only view built per run. `Time` alone is `Time<()>`: the virtual clock
 * in `Update`, the fixed one in `FixedUpdate`. `Time<Real>`'s `Instant`s are not visible.
 */
export type Time<C extends Real | Virtual | Fixed | void = void> = Resource &
  ([C] extends [Virtual] ? VirtualClockView : [C] extends [Fixed] ? FixedClockView : ClockView);
export declare const Time: (abstract new () => Time) & { readonly path: "bevy_time::Time" };
// ---------- App and plugins ----------

export interface App {
  addSystems(schedule: Schedule | ScheduleRef, ...systems: readonly Systemish[]): App;
  /** Registers a one-shot system, run with `commands.runSystem`, as Bevy's `register_system`. */
  registerSystem(fn: SystemFn): SystemId;
  /** Creates a custom schedule if absent. */
  initSchedule(label: ScheduleRef): App;
  /** Inserts a custom schedule into `MainScheduleOrder` after (or before) another. */
  insertScheduleAfter(after: Schedule | ScheduleRef, label: ScheduleRef): App;
  insertScheduleBefore(before: Schedule | ScheduleRef, label: ScheduleRef): App;
  /** As above, into `FixedMainScheduleOrder`. */
  insertFixedScheduleAfter(after: Schedule | ScheduleRef, label: ScheduleRef): App;
  insertFixedScheduleBefore(before: Schedule | ScheduleRef, label: ScheduleRef): App;
  /** As above, into the startup schedules. */
  insertStartupScheduleAfter(after: Schedule | ScheduleRef, label: ScheduleRef): App;
  insertStartupScheduleBefore(before: Schedule | ScheduleRef, label: ScheduleRef): App;
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
 * take both spellings in types and fail at load instead. A Rust component that is a tuple struct
 * of one field takes that field as `value` (`<WorldAssetRoot value="models/Fox.glb#Scene0" />`),
 * as `Name` does; a `Handle<T>` field takes an asset path.
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
export type ReadValue<C> = ReadView<C> & Readonly<HolesOf<C>>;

/** An entity a scene made (`ref={player}`), or `ref(entity)`. */
export interface NodeRef {
  /** The component's current value; inside a binding, the binding re-runs when it changes. */
  get<C extends abstract new () => unknown>(c: C): ReadValue<InstanceType<C>>;
  /**
   * Whether the entity carries component `C`, a marker (`Pressed`, `Checked`, `InteractionDisabled`)
   * or any other; inside a binding, the binding re-runs when that changes. Rust compares presence
   * each frame, without calling JS.
   */
  has(c: abstract new (...a: never) => unknown): boolean;
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
/** One subtree per item, kept by `key` across changes, in the items' order among its parent's children. */
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
/** `NodeImageMode::Sliced`: nine-slice, `border` in pixels (one or per side); centre and sides stretch or tile. */
export declare function sliced(options: {
  border: number | { left?: number; right?: number; top?: number; bottom?: number };
  center?: "stretch" | { tile: number };
  sides?: "stretch" | { tile: number };
  maxCornerScale?: number;
}): import("bevy-ts/jsx-runtime").ImageMode;
/** `NodeImageMode::Tiled`: repeats along `x` and `y` (both by default) once stretched past `stretch` (1). */
export declare function tiled(options?: { x?: boolean; y?: boolean; stretch?: number }): import("bevy-ts/jsx-runtime").ImageMode;
/** `ConicGradient`: stops are colours or `[colour, degrees]`; `from` is the start angle in degrees. */
export declare function conicGradient(stops: readonly (string | readonly [string, number])[], options?: { from?: number; at?: readonly [Length, Length] }): import("bevy-ts/jsx-runtime").Gradient;
