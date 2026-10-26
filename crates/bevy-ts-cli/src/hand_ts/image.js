/**
 * Bevy's `Image`: `Image.load(path, settings)`, `Image.of({ sampler })`, and `Image.new(size,
 * format, data, { sampler })`, an image from TS bytes that `images.add` makes in one host call
 * (change script-image-bytes). `TextureFormat` names its formats.
 */
import { imageNew } from "bevy-ts";
export { TextureFormat } from "bevy-ts";
export const Image = Object.assign(asset("bevy_image::image::Image", { load: true }), { new: imageNew });
