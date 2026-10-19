// The ruled bevy-ts surface (operator, 2026-10-20), types only (`declare`), so `deno check` can
// prove inference. Nothing here runs. See docs/research/dx.md.
//
// Systems are plain exported functions whose parameter types say what they use, as in Rust.
// bevy-ts's oxc compile step reads those annotations from syntax alone (bench/dx/read-annotations.ts)
// and attaches the declarations. A system whose param types are computed (ReturnType, Pick, ...)
// uses the setup-closure fallback, `system(() => { ...declare; return () => { ...run } })`.

// ---------- Fields ----------

export type FieldKind =
  | "f32" | "f64" | "i32" | "u32" | "u8" | "bool" | "vec2" | "vec3" | "quat" | "entity" | "enum";

export interface Field<K extends FieldKind, E extends string = never> {
  readonly kind: K;
  readonly variants?: readonly E[];
  default(v: unknown): Field<K, E>;
}
// Each field kind is a value (for schemas) and a type (for `Local<{ timer: f32 }>`).
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
export declare const bool: Field<"bool">;
export type bool = boolean;
export declare const vec2: Field<"vec2">;
/** A field type in schemas, and a scratch-vector constructor in code: `vec3(0, 1, 0)`. */
export declare const vec3: Field<"vec3"> & ((x?: number, y?: number, z?: number) => Vec3);
export type vec3 = Vec3;
export declare const quat: Field<"quat"> & (() => Quat);
export type quat = Quat;
export declare const entity: Field<"entity">;
export declare function enumOf<const E extends string>(...variants: E[]): Field<"enum", E>;

export type Schema = Record<string, Field<FieldKind, string>>;

// ---------- Vectors: in-place, glam-like, allocation-free ----------

declare const entityBrand: unique symbol;
export type Entity = { readonly [entityBrand]: true };

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

type MutField<F> =
  F extends Field<"vec2" | "vec3"> ? Vec3
  : F extends Field<"quat"> ? Quat
  : F extends Field<"bool"> ? boolean
  : F extends Field<"entity"> ? Entity
  : F extends Field<"enum", infer E> ? E
  : number;
type VectorKey<S> = { [K in keyof S]: S[K] extends Field<"vec2" | "vec3" | "quat"> ? K : never }[keyof S];
/** A component's data, writable: scalars assignable, vector fields are views mutated in place. */
export type Data<S> =
  & { -readonly [K in Exclude<keyof S, VectorKey<S>>]: MutField<S[K]> }
  & { readonly [K in VectorKey<S>]: MutField<S[K]> };
/** Read access: every field readonly, vectors without mutating methods. */
export type ReadView<T> = {
  readonly [K in keyof T]: T[K] extends Vec3 ? ReadonlyVec3 : T[K] extends Quat ? ReadonlyQuat : T[K];
};
type Init<S> = {
  [K in keyof S]?: S[K] extends Field<"vec2" | "vec3"> ? readonly [number, number, number] | ReadonlyVec3 : MutField<S[K]>;
};

// ---------- Components, resources, messages, events, states: one class each ----------

/**
 * `layout`: a column a TS system may read and write (TS components, ported Rust components).
 * `reflect`: typed in TSX and the protocol, usable in `With`/`Without`, never as query data.
 */
export type Tier = "layout" | "reflect";
declare const tierBrand: unique symbol;

/** `class Enemy extends Component("game::Enemy", { speed: f32 }) {}`: the class is the handle, its instance type is the data. */
export interface ComponentClass<S extends Schema = Schema, T extends Tier = Tier> {
  new (init?: Init<S>): Data<S> & { readonly [tierBrand]?: T };
  readonly schema: S;
  readonly tier: T;
}
export declare function Component<const S extends Schema>(name: string, fields: S): ComponentClass<S, "layout">;
/** A component with no data: a marker for filters. */
export declare function Tag(name: string): ComponentClass<{}, "layout">;
/** A Rust component reachable only through reflection (a ported type the layout rule refuses). */
export declare function ReflectComponent<const S extends Schema>(name: string, fields: S): ComponentClass<S, "reflect">;

declare const resourceBrand: unique symbol;
export declare function Resource<const S extends Schema>(name: string, fields: S): {
  new (init?: Init<S>): Data<S> & { readonly [resourceBrand]: true };
  readonly schema: S;
};
declare const messageBrand: unique symbol;
export declare function Message<const S extends Schema>(name: string, fields: S): {
  new (init?: Init<S>): Data<S> & { readonly [messageBrand]: true };
  readonly schema: S;
};
declare const eventBrand: unique symbol;
/** An EntityEvent: targets an entity; `propagate` bubbles it up `ChildOf`. */
export declare function EntityEvent<const S extends Schema>(name: string, fields: S, opts?: { propagate?: boolean }): {
  new (init?: Init<S>): Data<S> & { readonly [eventBrand]: "entity" };
  readonly schema: S;
};
export declare function Event<const S extends Schema>(name: string, fields: S): {
  new (init?: Init<S>): Data<S> & { readonly [eventBrand]: "global" };
  readonly schema: S;
};

declare const stateBrand: unique symbol;
export interface StateValue<N extends string = string, V extends string = string> { readonly state: N; readonly value: V }
/** `class Game extends States("game::Game", ["Menu", "Playing"]) {}`; `Game.Playing` is a value. */
export declare function States<const N extends string, const V extends string>(name: N, values: readonly V[]):
  & { new (): { readonly [stateBrand]: { name: N; values: V } } }
  & { readonly [K in V]: StateValue<N, K> };
type StateValueOf<G> = G extends { readonly [stateBrand]: { name: infer N extends string; values: infer V extends string } }
  ? { [K in V]: StateValue<N, K> }[V]
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

type NotReflect<T> = T extends { readonly [tierBrand]?: "reflect" } ? never : T;
type Item<D> =
  D extends Entity ? Entity
  : D extends Option<infer X> ? Item<X> | undefined
  : D extends Mut<infer T> ? NotReflect<T>
  : D extends abstract new (...a: never) => infer I ? ReadView<NotReflect<I>> // value form: Mut(C) / C
  : ReadView<NotReflect<D>>;
export type Row<Ds extends readonly unknown[]> = { -readonly [I in keyof Ds]: Item<Ds[I]> };

export interface Table {
  readonly len: number;
  readonly entities: Uint32Array;
  read(c: ComponentClass): { f32: Float32Array; u32: Uint32Array; stride: number; offsets: Record<string, number> };
  write(c: ComponentClass): { f32: Float32Array; u32: Uint32Array; stride: number; offsets: Record<string, number> };
  split(c: ComponentClass): { f32: Float32Array; u32: Uint32Array; stride: number; offsets: Record<string, number>; ticks: Uint32Array };
}
export interface QueryOf<R extends unknown[]> extends Iterable<R> {
  /** Fastest form: reused accessors, no iterator protocol (2.4x faster than for-of on V8). */
  each(f: (...row: R) => void): void;
  /** Exactly one match, or throws (Bevy's `single`). */
  single(): R;
  get(e: Entity): R | undefined;
  readonly len: number;
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
  /** Components (`new Health({ hp: 3 })`) or a prefab element (`<Grunt speed={3} />`). */
  spawn(...parts: readonly (object | Element)[]): Entity;
  entity(e: Entity): EntityCommands;
  trigger(event: { readonly [eventBrand]: "global" }): void;
  trigger(target: Entity, event: { readonly [eventBrand]: "entity" }): void;
  write(message: { readonly [messageBrand]: true }): void;
}
export interface EntityCommands {
  insert(...parts: readonly (object | Element)[]): EntityCommands;
  remove(...cs: readonly ComponentClass[]): EntityCommands;
  despawn(): void;
  readonly id: Entity;
}

// ---------- Observers ----------

declare const lifecycleBrand: unique symbol;
export type Add<C> = { readonly [lifecycleBrand]: ["add", C] };
export type Insert<C> = { readonly [lifecycleBrand]: ["insert", C] };
export type Remove<C> = { readonly [lifecycleBrand]: ["remove", C] };
/** An observer's first parameter, as Bevy's `On<Explode>` / `On<Add<Health>>`. */
export interface On<E> {
  readonly event: E extends { readonly [lifecycleBrand]: unknown } ? {} : ReadView<E>;
  readonly entity: Entity;
  /** Stop bubbling. */
  propagate(on: boolean): void;
}
/** bevy_picking 0.20's `PointerClick`, an entity event that bubbles. */
export declare class PointerClick extends EntityEvent("bevy_picking::events::PointerClick", { x: f32, y: f32 }) {}

// ---------- Systems, schedules, ordering ----------

/** A system: an exported function whose parameter types the compile step read. */
export type SystemFn = (...params: never[]) => void;
/** A condition evaluated in Rust where it can be (state, resource compare), so a skipped system costs no JS call. */
export interface Condition { readonly condition: true }
export declare function inState<N extends string, V extends string>(v: StateValue<N, V>): Condition;
export declare function resourceExists(r: abstract new (...a: never) => unknown): Condition;
export declare function onMessage(m: abstract new (...a: never) => unknown): Condition;
export declare function not(c: Condition): Condition;

export interface SystemSet { readonly set: string }
export declare function systemSet(name: string): SystemSet;

type Systemish = SystemFn | System | SystemConfig;
/** Bevy's tuple of systems and its config methods: `systems(a, b).after(x).runIf(c)`. */
export interface SystemConfig {
  after(...x: readonly (Systemish | SystemSet)[]): SystemConfig;
  before(...x: readonly (Systemish | SystemSet)[]): SystemConfig;
  runIf(c: Condition): SystemConfig;
  inSet(s: SystemSet): SystemConfig;
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

// ---------- Rust types bevy-ts ships typed ----------

export declare class Transform extends Component("bevy_transform::components::transform::Transform", {
  translation: vec3, rotation: quat, scale: vec3,
}) {}
export declare class Time extends Resource("bevy_time::Time", { delta: f32, elapsed: f64 }) {}

// ---------- App and plugins ----------

export interface App {
  addSystems(schedule: Schedule, ...systems: readonly Systemish[]): App;
  addObserver(o: (on: On<never>, ...params: never[]) => void): App;
  addPlugins(...p: readonly Plugin[]): App;
  initState(initial: StateValue): App;
  insertResource(r: { readonly [resourceBrand]: true }): App;
  addMessage(m: abstract new (...a: never) => { readonly [messageBrand]: true }): App;
  scene(name: string, root: () => Element): App;
}
/** A plugin: an exported function taking the app, as Bevy's `fn build(&self, app: &mut App)`. */
export type Plugin = (app: App) => void;
export declare const app: App;

// ---------- TSX ----------

export interface Element { readonly type: unknown; readonly props: unknown; readonly key?: string }
export type Child = Element | string | number | (() => string | number) | readonly Child[] | null | false | undefined;

/** Read another entity's component inside a binding: resolved by tsx decision 11's per-site rule. */
export interface NodeRef {
  get<C extends abstract new (...a: never) => unknown>(c: C): ReadView<InstanceType<C>>;
  readonly entity: Entity;
}
export declare function ref(): NodeRef;
export declare function px(n: number): string;
/** tsx as specced before the ruling, kept to show the hook form side by side. */
export declare function useComponent<C extends abstract new (...a: never) => unknown>(target: NodeRef, c: C): () => ReadView<InstanceType<C>>;
