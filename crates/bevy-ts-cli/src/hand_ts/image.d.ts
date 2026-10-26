export { TextureFormat } from "bevy-ts";
/** The loader settings `Image.load` takes. */
export type ImageLoadSettings = { sampler?: Image["sampler"]; is_srgb?: boolean };
/** The options `Image.new` takes: the sampler in the shape `Image.load` and `assets.set` take. */
export interface ImageOptions {
  sampler?: Image["sampler"];
}
declare const newImageBrand: unique symbol;
/** An image `Image.new` made: recorded in JS until `images.add` makes it in one host call. */
export interface NewImage {
  readonly size: [number, number];
  readonly format: import("bevy-ts").TextureFormat;
  readonly data: Uint8Array;
  readonly [newImageBrand]: true;
}
/** Bevy's `Image`. `Image.new` makes one from bytes for `images: ResMut<Assets<Image>>` (docs/assets.md). */
export declare const Image: AssetClass<Image> & Loadable<Image, ImageLoadSettings> & {
  /** A 2D image of `data`, exactly `width × height × bytes per pixel` long; `images.add` copies it once into Bevy's image. */
  "new"(size: readonly [number, number], format: import("bevy-ts").TextureFormat, data: Uint8Array, options?: ImageOptions): NewImage;
};
