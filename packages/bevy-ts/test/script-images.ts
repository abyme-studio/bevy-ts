// Type tests for script-image-bytes (docs/assets.md, images from bytes): `ResMut<Assets<Image>>` is
// the cli's `Images`, `images.add` gives an `AssetRef<Image>` for `Handle<Image>` slots and texture
// props, and a mesh ref fits neither. The block below is what `bevy-ts types` writes into
// native.d.ts for it.
import { assets, Indices, Mesh, PrimitiveTopology, type AssetRef, type Assets, type ResMut } from "bevy-ts";
import { Image, StandardMaterial, TextureFormat } from "bevy-ts/bevy";
import type { MaterialKind } from "bevy-ts/material";

declare module "bevy-ts" {
  interface ResMutClaims {
    "ResMut<Assets<Image>>": [Assets<Image>, import("bevy-ts/assets").Images];
    "ResMut<Assets<Mesh>>": [Assets<Mesh>, import("bevy-ts/assets").Meshes];
  }
}

declare const Town: MaterialKind<{}, { owners: "texture2d" }>;
type TownProps = NonNullable<(typeof Town)["__props"]>;

export function paint(images: ResMut<Assets<Image>>, meshes: ResMut<Assets<Mesh>>) {
  const bytes = new Uint8Array(4 * 2 * 4);
  const r: AssetRef<Image> = images.add(Image.new([4, 2], TextureFormat.Rgba8UnormSrgb, bytes, { sampler: { Descriptor: { mag_filter: "Nearest" } } }));
  const live = images.getMut(r);
  live?.setData(bytes).writeRect(1, 0, 2, 2, new Uint8Array(16));
  const size: [number, number] | undefined = images.get(r)?.size;
  const format: TextureFormat | undefined = images.get(r)?.format;
  const removed: boolean = images.remove(r);
  // A ref from `images.add` goes into a `Handle<Image>` slot and a texture prop.
  assets.add(StandardMaterial, { base_color_texture: r, unlit: true });
  const prop: TownProps["owners"] = r;
  const path: TownProps["owners"] = "owners.png";
  // @ts-expect-error  bytes are a Uint8Array
  Image.new([4, 2], TextureFormat.Rgba8Unorm, new Float32Array(8));
  // @ts-expect-error  a format of TextureFormat
  Image.new([4, 2], "Rgba8Unorm", bytes);
  const mesh = meshes.add(Mesh.new(PrimitiveTopology.TriangleList).insertIndices(Indices.U16(new Uint16Array(3))));
  // @ts-expect-error  a mesh ref is not an image
  images.getMut(mesh);
  // @ts-expect-error  a mesh ref is not a texture
  const wrong: TownProps["owners"] = mesh;
  // @ts-expect-error  a mesh ref is not a base colour texture
  assets.add(StandardMaterial, { base_color_texture: mesh });
  void size, format, removed, prop, path, wrong;
}
