// @generated from crates/bevy-ts-material/ts/material.d.ts by scripts/sync-runtime.mjs. Do not edit.
import type { AssetRef } from "bevy-ts";

/** The value a `.wgsl` import gives: the file, by path. Its WGSL stays in the engine. */
export interface ShaderFile {
  readonly kind: "shaderFile";
  readonly path: string;
}

export type Base = "f32" | "i32" | "u32" | "vec2f" | "vec3f" | "vec4f" | "mat4x4f" | "color";
export type ParamType = Base | { readonly array: Base; readonly len: number };

export declare const f32: "f32";
export declare const i32: "i32";
export declare const u32: "u32";
export declare const vec2f: "vec2f";
export declare const vec3f: "vec3f";
export declare const vec4f: "vec4f";
export declare const mat4x4f: "mat4x4f";
/** A `vec4<f32>` in linear RGBA, written as `"#f80"` or sRGB `[r, g, b, a?]`. */
export declare const color: "color";
/** A 2D texture with a filtering sampler: `name` and `name_sampler` in WGSL. */
export declare const texture2d: "texture2d";

/** A fixed array, strided to 16 bytes; scalar and `vec2f` elements read as `.x`/`.xy` of a `vec4`. */
export declare function array<B extends Base, N extends number>(of: B, len: N): { readonly array: B; readonly len: N };

export type AlphaMode = "opaque" | "blend" | "premultiplied" | "add" | "multiply" | "alphaToCoverage" | { mask: number };

/** A param's value in TS: numbers, number tuples, or a color. */
export type ParamValue<T extends ParamType> = T extends "f32" | "i32" | "u32"
  ? number
  : T extends "vec2f"
    ? [number, number]
    : T extends "vec3f"
      ? [number, number, number]
      : T extends "vec4f"
        ? [number, number, number, number]
        : T extends "mat4x4f"
          ? number[]
          : T extends "color"
            ? string | [number, number, number] | [number, number, number, number]
            : T extends { array: infer E extends Base }
              ? ParamValue<E>[]
              : never;

/** A material kind, what `<material kind={...} />` names. */
export interface MaterialKind<
  P extends Record<string, ParamType> = Record<string, ParamType>,
  T extends Record<string, "texture2d"> = {},
  I extends Record<string, ParamType> = {},
> {
  readonly name: string;
  /**
   * Types only: the props `<material>` takes for this kind: params by value, textures as an asset
   * path or an image ref (`images.add(Image.new(...))`), instance params by value or bound
   * (`hover={() => h()}`).
   */
  readonly __props?: { [K in keyof P]?: ParamValue<P[K]> } & { [K in keyof T]?: string | AssetRef<import("bevy-ts/bevy").Image> } & { [K in keyof I]?: ParamValue<I[K]> | (() => ParamValue<I[K]>) };
  /**
   * An owned instance: one asset outside the shared cache, written in place at the same asset id
   * with `assets.set(ref, patch)` and given as `<material asset={ref} />`. `assets.get(ref)` reads
   * `{ kind, ...params, ...textures }`, colours as `{ LinearRgba }` and textures as paths or `null`.
   */
  add(props?: { [K in keyof P]?: ParamValue<P[K]> } & { [K in keyof T]?: string | null }): AssetRef<KindFields<P, T>>;
}

/** What `assets.get` reads from an owned instance. */
export type KindFields<P extends Record<string, ParamType>, T extends Record<string, "texture2d">> = { kind: string } & {
  [K in keyof P]?: P[K] extends "color" ? ParamValue<P[K]> | { LinearRgba: { red: number; green: number; blue: number; alpha: number } } : ParamValue<P[K]>;
} & { [K in keyof T]?: string | null };

/**
 * Declares a script material: its params, laid out by WGSL's uniform rules in Rust; its textures;
 * its per-entity `instance` params (at most 256 bytes); and its shaders, `.wgsl` files imported
 * like modules (`import glow from "./glow.wgsl"`). The WGSL reads the params as `params.<name>`,
 * each texture as `<name>` with `<name>_sampler`, and the instance params as
 * `instance(in.instance_index).<name>`; bevy-ts appends those declarations, so the file does not
 * declare them.
 */
export declare function material<P extends Record<string, ParamType>, T extends Record<string, "texture2d"> = {}, I extends Record<string, ParamType> = {}>(
  name: string,
  decl: { params: P; textures?: T; instance?: I; fragment: ShaderFile; vertex?: ShaderFile; alphaMode?: AlphaMode },
): MaterialKind<P, T, I>;
