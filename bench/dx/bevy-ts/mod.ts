// The proposed bevy-ts surface, types only (`declare`), so `deno check` can prove inference.
// Nothing here runs. See docs/research/dx.md.

// ---------- Fields and schemas ----------

export type FieldKind =
  | "f32" | "f64" | "i32" | "u32" | "u8" | "bool" | "vec2" | "vec3" | "quat" | "entity" | "enum";

export interface Field<K extends FieldKind, E extends string = never> {
  readonly kind: K;
  readonly variants?: readonly E[];
  default(v: unknown): Field<K, E>;
}
export declare const f32: Field<"f32">;
export declare const f64: Field<"f64">;
export declare const i32: Field<"i32">;
export declare const u32: Field<"u32">;
export declare const u8: Field<"u8">;
export declare const bool: Field<"bool">;
export declare const vec2: Field<"vec2">;
/** A field type in schemas, and a scratch-vector constructor in code: `vec3(0, 1, 0)`. */
export declare const vec3: Field<"vec3"> & ((x?: number, y?: number, z?: number) => Vec3);
export declare const quat: Field<"quat"> & (() => Quat);
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
  /** this = rotation that looks along `dir` with `up` */
  lookTo(dir: ReadonlyVec3, up: ReadonlyVec3): this;
  rotateY(radians: number): this;
  slerp(q: ReadonlyQuat, t: number): this;
}
export declare const Vec3: { readonly ZERO: ReadonlyVec3; readonly Y: ReadonlyVec3 };

type RefField<F> =
  F extends Field<"vec2" | "vec3"> ? ReadonlyVec3
  : F extends Field<"quat"> ? ReadonlyQuat
  : F extends Field<"bool"> ? boolean
  : F extends Field<"entity"> ? Entity
  : F extends Field<"enum", infer E> ? E
  : number;
type MutField<F> =
  F extends Field<"vec2" | "vec3"> ? Vec3
  : F extends Field<"quat"> ? Quat
  : F extends Field<"bool"> ? boolean
  : F extends Field<"entity"> ? Entity
  : F extends Field<"enum", infer E> ? E
  : number;

type VectorKey<S> = { [K in keyof S]: S[K] extends Field<"vec2" | "vec3" | "quat"> ? K : never }[keyof S];
/** Read access: every field readonly, vectors without mutating methods. */
export type Ref<S> = { readonly [K in keyof S]: RefField<S[K]> };
/** Write access: scalars assignable; vector fields are views you mutate in place. */
export type MutRef<S> =
  & { -readonly [K in Exclude<keyof S, VectorKey<S>>]: MutField<S[K]> }
  & { readonly [K in VectorKey<S>]: MutField<S[K]> };
export type Value<S> = { [K in keyof S]?: S[K] extends Field<"vec2" | "vec3"> ? readonly [number, number, number] | ReadonlyVec3 : RefField<S[K]> };

// ---------- Components ----------

/**
 * `layout`: a column a TS system may read and write (TS components, ported Rust components).
 * `reflect`: typed in TSX and the protocol, usable in `with`/`without`, never as query data.
 */
export type Tier = "layout" | "reflect";
export interface ComponentType<N extends string = string, S extends Schema = Schema, T extends Tier = Tier> {
  readonly name: N;
  readonly schema: S;
  readonly tier: T;
}
export type LayoutComponent = ComponentType<string, Schema, "layout">;

/** A prop is a value, or a binding `() => value` (tsx decisions 3 and 11). */
type PropValue<F> = RefField<F> | (F extends Field<"vec2" | "vec3"> ? readonly [number, number, number] : never);
export type ComponentProps<S> =
  & { [K in keyof S]?: PropValue<S[K]> | (() => PropValue<S[K]>) }
  & { [K in keyof S & string as `initial${Capitalize<K>}`]?: PropValue<S[K]> };
/** A component handle is also its TSX tag and its bundle constructor: `<Health hp={3} />`, `Health({ hp: 3 })`. */
export type ComponentHandle<N extends string, S extends Schema, T extends Tier> =
  & ComponentType<N, S, T>
  & ((props?: ComponentProps<S>) => Element);

export declare function component<const N extends string, const S extends Schema>(
  name: N,
  fields: S,
): ComponentHandle<N, S, "layout">;
/** A tag: no data, filter only (or inserted as a marker). */
export declare function tag<const N extends string>(name: N): ComponentHandle<N, {}, "layout">;

// ---------- Query data ----------

export interface MutOf<C extends LayoutComponent> { readonly kind: "mut"; readonly c: C }
export interface OptOf<D> { readonly kind: "opt"; readonly d: D }
declare const entityToken: unique symbol;
export interface EntityToken { readonly [entityToken]: true }
/** Query data token for the row's entity, as `Entity` is in Bevy's `Query<(Entity, ...)>`. */
export declare const EntityId: EntityToken;
export declare function Mut<C extends LayoutComponent>(c: C): MutOf<C>;
export declare function Option<D extends LayoutComponent | MutOf<LayoutComponent>>(d: D): OptOf<D>;

export type QueryDatum = LayoutComponent | MutOf<LayoutComponent> | OptOf<LayoutComponent | MutOf<LayoutComponent>> | EntityToken;
type Item<D> =
  D extends EntityToken ? Entity
  : D extends MutOf<infer C> ? MutRef<C["schema"]>
  : D extends OptOf<infer X> ? Item<X> | undefined
  : D extends ComponentType<string, infer S, "layout"> ? Ref<S>
  : never;
export type Row<Ds extends readonly unknown[]> = { -readonly [I in keyof Ds]: Item<Ds[I]> };

export interface Filter {
  with?: readonly ComponentType[];
  without?: readonly ComponentType[];
  /** Rows whose component changed since this system last ran (bridge per-row ticks). */
  changed?: readonly LayoutComponent[];
  added?: readonly ComponentType[];
}

export interface Table {
  readonly len: number;
  readonly entities: Uint32Array;
  read(c: LayoutComponent): { f32: Float32Array; u32: Uint32Array; stride: number; offsets: Record<string, number> };
  write(c: LayoutComponent): { f32: Float32Array; u32: Uint32Array; stride: number; offsets: Record<string, number> };
  split(c: LayoutComponent): { f32: Float32Array; u32: Uint32Array; stride: number; offsets: Record<string, number>; ticks: Uint32Array };
}

export interface Query<R extends unknown[]> extends Iterable<R> {
  /** Fastest form: reused accessors, no iterator protocol (2.4x faster than for-of on V8). */
  each(f: (...row: R) => void): void;
  /** Exactly one match, or throws (Bevy's `single`). */
  single(): R;
  get(e: Entity): R | undefined;
  readonly len: number;
  readonly tables: readonly Table[];
}

// ---------- System params ----------

declare const paramBrand: unique symbol;
/** A system parameter descriptor resolving to `T` inside the system body. */
export interface Param<T> { readonly [paramBrand]: T }

export declare function Query<const Ds extends readonly QueryDatum[]>(data: Ds, filter?: Filter): Param<Query<Row<Ds>>>;

export interface ResourceType<N extends string = string, S extends Schema = Schema> {
  readonly name: N;
  readonly schema: S;
  readonly resource: true;
}
export declare function resource<const N extends string, const S extends Schema>(name: N, fields: S): ResourceType<N, S>;
export declare function Res<R extends ResourceType>(r: R): Param<Ref<R["schema"]>>;
export declare function ResMut<R extends ResourceType>(r: R): Param<MutRef<R["schema"]>>;
/** Per-system state with a layout. Lives in Rust, keyed by the system's id, so it survives reloads. */
export declare function Local<const S extends Schema>(fields: S): Param<MutRef<S>>;

export interface MessageType<N extends string = string, S extends Schema = Schema> {
  readonly name: N;
  readonly schema: S;
  readonly message: true;
}
export declare function message<const N extends string, const S extends Schema>(name: N, fields: S): MessageType<N, S>;
export interface MessageWriterOf<S> {
  write(value: Value<S>): void;
  /** Allocation-free: a writable view on a fresh slot. */
  next(): MutRef<S>;
}
export interface MessageReaderOf<S> extends Iterable<Ref<S>> {
  each(f: (m: Ref<S>) => void): void;
  readonly len: number;
}
export declare function MessageWriter<M extends MessageType>(m: M): Param<MessageWriterOf<M["schema"]>>;
export declare function MessageReader<M extends MessageType>(m: M): Param<MessageReaderOf<M["schema"]>>;

export interface EventType<N extends string = string, S extends Schema = Schema, E extends boolean = boolean> {
  readonly name: N;
  readonly schema: S;
  readonly entityEvent: E;
}
export declare function event<const N extends string, const S extends Schema>(name: N, fields: S): EventType<N, S, false>;
/** An EntityEvent: targets an entity; `propagate` bubbles it up `ChildOf`. */
export declare function entityEvent<const N extends string, const S extends Schema>(
  name: N,
  fields: S,
  opts?: { propagate?: boolean },
): EventType<N, S, true>;

export interface Commands {
  /** Components (`Health({ hp: 3 })`) or a prefab element (`<Enemy speed={3} />`). */
  spawn(...parts: readonly Element[]): Entity;
  entity(e: Entity): EntityCommands;
  trigger<E extends EventType<string, Schema, false>>(ev: E, value: Value<E["schema"]>): void;
  trigger<E extends EventType<string, Schema, true>>(ev: E, target: Entity, value: Value<E["schema"]>): void;
  write<M extends MessageType>(m: M, value: Value<M["schema"]>): void;
}
export interface EntityCommands {
  insert(...parts: readonly Element[]): EntityCommands;
  remove(...cs: readonly ComponentType[]): EntityCommands;
  despawn(): void;
  readonly id: Entity;
}
export declare const Commands: Param<Commands>;

// ---------- Rust resources bevy-ts ships typed ----------

export declare const Time: ResourceType<"bevy_time::Time", { delta: typeof f32; elapsed: typeof f64 }>;
export declare const Transform: ComponentHandle<"bevy_transform::components::transform::Transform", {
  translation: typeof vec3;
  rotation: typeof quat;
  scale: typeof vec3;
}, "layout">;
export declare const GlobalTransform: ComponentHandle<"bevy_transform::components::global_transform::GlobalTransform", {
  translation: typeof vec3;
}, "layout">;

// ---------- States ----------

export interface StateType<N extends string = string, V extends string = string> {
  readonly name: N;
  readonly values: { readonly [K in V]: StateValue<N, K> };
}
export interface StateValue<N extends string, V extends string> { readonly state: N; readonly value: V }
export declare function states<const N extends string, const V extends string>(
  name: N,
  values: readonly V[],
): StateType<N, V> & { readonly [K in V]: StateValue<N, K> };
export declare function State<S extends StateType>(s: S): Param<{ readonly current: keyof S["values"] }>;
export declare function NextState<S extends StateType>(s: S): Param<{ set(v: S["values"][keyof S["values"]]): void }>;

// ---------- Systems, schedules, ordering ----------

/** A condition evaluated in Rust where it can be (state, resource compare), so a skipped system costs no JS call. */
export interface Condition { readonly condition: true }
export declare function inState<N extends string, V extends string>(v: StateValue<N, V>): Condition;
export declare function resourceExists(r: ResourceType): Condition;
export declare function onMessage(m: MessageType): Condition;
export declare function not(c: Condition): Condition;

export interface SystemSet { readonly set: string }
export declare function systemSet(name: string): SystemSet;

export interface SystemConfig {
  after(...x: readonly (System | SystemSet)[]): SystemConfig;
  before(...x: readonly (System | SystemSet)[]): SystemConfig;
  runIf(c: Condition): SystemConfig;
  inSet(s: SystemSet): SystemConfig;
}
export interface System extends SystemConfig {
  /** `module#export`, assigned by the loader: the identity hot reload keys on. */
  readonly id: string;
}
type Resolve<P> = { [K in keyof P]: P[K] extends Param<infer T> ? T : never };

export declare function system<const P extends Record<string, Param<unknown>>>(
  params: P,
  run: (p: Resolve<P>) => void,
): System;
/** Chain: each runs after the previous, as Bevy's `.chain()`. */
export declare function chain(...s: readonly (System | SystemConfig)[]): SystemConfig;

export interface Schedule { readonly schedule: string }
export declare const Startup: Schedule;
export declare const PreUpdate: Schedule;
export declare const Update: Schedule;
export declare const FixedUpdate: Schedule;
export declare const PostUpdate: Schedule;
export declare function OnEnter<N extends string, V extends string>(v: StateValue<N, V>): Schedule;
export declare function OnExit<N extends string, V extends string>(v: StateValue<N, V>): Schedule;

// ---------- Observers ----------

export interface Trigger<S, E extends boolean> {
  readonly event: Ref<S>;
  /** The targeted entity, for an EntityEvent or a lifecycle event. */
  readonly entity: E extends true ? Entity : never;
  /** Stop bubbling. */
  propagate(on: boolean): void;
}
export interface Pattern<S extends Schema, E extends boolean> { readonly pattern: true; readonly _s?: S; readonly _e?: E }
export declare function On<const S extends Schema, const E extends boolean>(ev: EventType<string, S, E>): Pattern<S, E>;
/** Lifecycle patterns, as Bevy 0.20's `On<Add<Health>>`. */
export declare function Add(c: ComponentType): Pattern<{}, true>;
export declare function Insert(c: ComponentType): Pattern<{}, true>;
export declare function Remove(c: ComponentType): Pattern<{}, true>;
/** bevy_picking 0.20 events (`PointerClick`, ...), entity events that bubble. */
export declare const PointerClick: EventType<"bevy_picking::events::PointerClick", { x: typeof f32; y: typeof f32 }, true>;

export interface Observer { readonly id: string }
export declare function observer<S extends Schema, E extends boolean, const P extends Record<string, Param<unknown>>>(
  on: Pattern<S, E>,
  params: P,
  run: (on: Trigger<S, E>, p: Resolve<P>) => void,
): Observer;

// ---------- App and plugins ----------

export interface App {
  addSystems(schedule: Schedule, ...systems: readonly (System | SystemConfig)[]): App;
  addObserver(o: Observer): App;
  addPlugins(...p: readonly Plugin[]): App;
  initState(s: StateType, initial?: StateValue<string, string>): App;
  initResource<R extends ResourceType>(r: R, value?: Value<R["schema"]>): App;
  addMessage(m: MessageType): App;
  configureSets(schedule: Schedule, ...sets: readonly SystemConfig[]): App;
  scene(name: string, root: () => Element): App;
}
export interface Plugin { readonly plugin: true; readonly id: string }
export declare function plugin(build: (app: App) => void): Plugin;
export declare const app: App;

// ---------- TSX ----------

export interface Element { readonly type: unknown; readonly props: unknown; readonly key?: string }
export type Child = Element | string | number | (() => string | number) | readonly Child[] | null | false | undefined;

/** Read another entity's component inside a binding: resolved by tsx decision 11's per-site rule. */
export interface NodeRef {
  get<C extends LayoutComponent>(c: C): Ref<C["schema"]>;
  readonly entity: Entity;
}
export declare function ref(): NodeRef;
export declare function res<R extends ResourceType>(r: R): Ref<R["schema"]>;
export declare function useComponent<C extends LayoutComponent>(target: NodeRef, c: C): () => Ref<C["schema"]>;
export declare function px(n: number): string;
