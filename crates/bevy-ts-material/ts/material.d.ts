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
export interface MaterialKind<P extends Record<string, ParamType> = Record<string, ParamType>, T extends Record<string, "texture2d"> = {}> {
  readonly name: string;
  /** Types only: the params and textures `<material>` takes for this kind. */
  readonly __props?: { [K in keyof P]?: ParamValue<P[K]> } & { [K in keyof T]?: string };
}

/**
 * Declares a script material: its params, laid out by WGSL's uniform rules in Rust; its textures;
 * and its shaders, `.wgsl` files imported like modules (`import glow from "./glow.wgsl"`). The WGSL
 * reads the params as `params.<name>` and each texture as `<name>` with `<name>_sampler`; bevy-ts
 * appends those declarations, so the file does not declare them.
 */
export declare function material<P extends Record<string, ParamType>, T extends Record<string, "texture2d"> = {}>(
  name: string,
  decl: { params: P; textures?: T; fragment: ShaderFile; vertex?: ShaderFile; alphaMode?: AlphaMode },
): MaterialKind<P, T>;
