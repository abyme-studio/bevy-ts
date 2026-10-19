// Types for `bevy-ts`. The runtime is built into the engine binary; this package carries the
// types, the protocol types and the `bevy-ts` command.

/** An entity id: `Entity::to_bits` as a number. */
export type Entity = number & { readonly __entity?: never };

export interface Hints {
  min?: number;
  max?: number;
  step?: number;
  label?: string;
}

/** A field type. `R` is what reading gives, `W` what writing accepts. */
export interface FieldType<R, W = R, K extends string = string> {
  readonly __read?: R;
  readonly __write?: W;
  readonly __kind?: K;
  /** A default for new components and for new fields in a migration. */
  default(value: W): FieldType<R, W, K>;
  hints(hints: Hints): FieldType<R, W, K>;
  min(v: number): FieldType<R, W, K>;
  max(v: number): FieldType<R, W, K>;
  step(v: number): FieldType<R, W, K>;
  label(v: string): FieldType<R, W, K>;
}

/** A vector field's view: lanes read and write in place. */
export interface Vec2View {
  x: number;
  y: number;
  set(x: number, y: number): this;
  toArray(): [number, number];
}
export interface Vec3View {
  x: number;
  y: number;
  z: number;
  set(x: number, y: number, z: number): this;
  toArray(): [number, number, number];
}
export interface Vec4View {
  x: number;
  y: number;
  z: number;
  w: number;
  set(x: number, y: number, z: number, w: number): this;
  toArray(): [number, number, number, number];
}
export type Vec2Value = [number, number] | { x: number; y: number };
export type Vec3Value = [number, number, number] | { x: number; y: number; z: number };
export type Vec4Value = [number, number, number, number] | { x: number; y: number; z: number; w: number };

export interface ArrayView<T, W> extends Iterable<T> {
  readonly length: number;
  [index: number]: T;
  get(i: number): T;
  set(i: number, v: W): void;
  toArray(): T[];
}

export declare const f32: FieldType<number, number, "f32">;
export declare const f64: FieldType<number, number, "f64">;
export declare const i8: FieldType<number, number, "i8">;
export declare const u8: FieldType<number, number, "u8">;
export declare const i16: FieldType<number, number, "i16">;
export declare const u16: FieldType<number, number, "u16">;
export declare const i32: FieldType<number, number, "i32">;
export declare const u32: FieldType<number, number, "u32">;
export declare const bool: FieldType<boolean, boolean, "bool">;
export declare const vec2: FieldType<Vec2View, Vec2Value, "vec2">;
export declare const vec3: FieldType<Vec3View, Vec3Value, "vec3">;
export declare const vec4: FieldType<Vec4View, Vec4Value, "vec4">;
export declare const quat: FieldType<Vec4View, Vec4Value, "quat">;
export declare const entity: FieldType<Entity, Entity, "entity">;
/** Rejected at load: components hold fixed-size data only. */
export declare const string: FieldType<never, never, "string">;

export declare function enumOf<const V extends readonly string[]>(...variants: V): FieldType<V[number], V[number] | number, "enum">;
export declare function array<R, W>(of: FieldType<R, W>, len: number): FieldType<ArrayView<R, W>, W[], "array">;

export type Fields = Record<string, FieldType<any, any>>;

/** The row accessor of a component: fields read and write the ECS column in place. */
export type Row<F extends Fields> = { -readonly [K in keyof F]: F[K] extends FieldType<infer R, any> ? R : never } & {
  toJSON(): { [K in keyof F]: unknown };
};
/** As `Row`, for a component in `read`: writing throws. */
export type ReadRow<F extends Fields> = { readonly [K in keyof F]: F[K] extends FieldType<infer R, any> ? R : never };
/** Field values to spawn or insert a component with; omitted fields take their default. */
export type Values<F extends Fields> = { [K in keyof F]?: F[K] extends FieldType<any, infer W> ? W : never };

export interface ComponentValue<F extends Fields = Fields> {
  readonly type: ComponentType<F>;
  readonly values: Values<F>;
}

/** A component type. Call it with field values to make a value for `spawn`/`insert`. */
export interface ComponentType<F extends Fields = Fields> {
  (values?: Values<F>): ComponentValue<F>;
  readonly componentName: string;
  readonly source: "ts" | "rust";
  /** Byte offset of each field in a row, once the generation applies. */
  readonly offsets: { readonly [K in keyof F]: number };
  /** Bytes per row. */
  readonly size: number;
}

/** A Rust component without a native layout: for `with`/`without` only. */
export interface RustComponent {
  readonly componentName: string;
  readonly source: "rust";
  readonly __noLayout?: never;
}

export type AnyComponent = ComponentType<any> | RustComponent;

/** Declares a component with native fields; Rust computes its layout. */
export declare function component<const F extends Fields>(name: string, fields: F, options?: object): ComponentType<F>;

type RowsOf<T extends readonly ComponentType<any>[], W extends boolean> = {
  [K in keyof T]: T[K] extends ComponentType<infer F> ? (W extends true ? Row<F> : ReadRow<F>) : never;
};

/** A component's typed arrays over one table's column. */
export interface ColumnArrays<F extends Fields = Fields> {
  readonly buffer: ArrayBuffer;
  readonly f32?: Float32Array;
  readonly f64?: Float64Array;
  readonly i8?: Int8Array;
  readonly u8?: Uint8Array;
  readonly i16?: Int16Array;
  readonly u16?: Uint16Array;
  readonly i32?: Int32Array;
  readonly u32?: Uint32Array;
  readonly offsets: { readonly [K in keyof F]: number };
  readonly size: number;
  /** Elements per row for an element width in bytes. */
  stride(width: 1 | 2 | 4 | 8): number;
}

export interface Table {
  readonly len: number;
  /** (index, generation) pairs. */
  readonly entities: Uint32Array;
  entity(row: number): Entity;
  read<F extends Fields>(T: ComponentType<F>): ColumnArrays<F>;
  /** A `write` component's arrays; every row of this table is marked changed after the run. */
  write<F extends Fields>(T: ComponentType<F>): ColumnArrays<F>;
  /** A `write` component's arrays plus its changed ticks; set `ticks[row] = ctx.tick` yourself. */
  split<F extends Fields>(T: ComponentType<F>): ColumnArrays<F> & { readonly ticks: Uint32Array };
}

export interface Query<Rows extends readonly unknown[]> {
  each(fn: (entity: Entity, ...rows: Rows) => void): void;
  readonly tables: readonly Table[];
  readonly count: number;
}

export interface Commands {
  spawn(...components: (ComponentType<any> | ComponentValue<any>)[]): Entity;
  insert(entity: Entity, ...components: (ComponentType<any> | ComponentValue<any>)[]): void;
  remove(entity: Entity, ...types: ComponentType<any>[]): void;
  despawn(entity: Entity): void;
  setParent(child: Entity, parent: Entity | null): void;
  spawnNative(recipe: "camera3d", params?: { position?: [number, number, number]; lookAt?: [number, number, number] }): Entity;
  spawnNative(
    recipe: "directionalLight",
    params?: { position?: [number, number, number]; lookAt?: [number, number, number]; illuminance?: number; shadows?: boolean },
  ): Entity;
  spawnNative(
    recipe: "cuboid",
    params?: { size?: [number, number, number]; color?: string; position?: [number, number, number]; lookAt?: [number, number, number] },
  ): Entity;
  spawnNative(recipe: string, params?: unknown): Entity;
  /** Appends an op of a Rust plugin's command kind. */
  custom(kind: string, payload: Uint8Array | string | object): void;
}

export interface Context {
  readonly dt: number;
  readonly elapsed: number;
  /** This run's change tick. */
  readonly tick: number;
  readonly frame: number;
  readonly commands: Commands;
}

export type Stage = "preUpdate" | "update" | "postUpdate" | "fixedUpdate";

export interface SystemSpec<R extends readonly ComponentType<any>[], W extends readonly ComponentType<any>[]> {
  stage?: Stage;
  read?: R;
  write?: W;
  with?: readonly AnyComponent[];
  without?: readonly AnyComponent[];
  after?: string | readonly string[];
  run(q: Query<[...RowsOf<R, false>, ...RowsOf<W, true>]>, ctx: Context): void;
}

export declare const app: {
  system<const R extends readonly ComponentType<any>[] = [], const W extends readonly ComponentType<any>[] = []>(
    name: string,
    spec: SystemSpec<R, W>,
  ): void;
  startup(name: string, run: (ctx: Context) => void): void;
  entry(name: string, fn: (payload: Uint8Array, ctx: Context) => Uint8Array | string | object | void): void;
};

/** Writes a manifest section for a Rust plugin. */
export declare function section(name: string, value: Uint8Array | string | object): void;
/** Decodes UTF-8 bytes. */
export declare function text(bytes: Uint8Array): string;

export declare function nativeComponent<const F extends Fields>(name: string, fields: F): ComponentType<F>;
export declare function rustComponent(name: string): RustComponent;
