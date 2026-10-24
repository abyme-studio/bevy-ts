// Type tests for reflected assets (docs/assets.md): `tsc -p test` must pass, and every
// `@ts-expect-error` must be an error.
import { assets, type AssetRef } from "bevy-ts";
import { Image, Mesh, MeshMaterial3dStandardMaterial, StandardMaterial } from "bevy-ts/bevy";

export function make(img: AssetRef<Image>) {
  const mat = assets.add(StandardMaterial, { base_color: "#ff8800", unlit: true, base_color_texture: img });
  const r: AssetRef<StandardMaterial> = mat;
  assets.set(mat, { perceptual_roughness: 0.2 });
  const roughness: number | undefined = assets.get(mat)?.perceptual_roughness;
  void r, roughness;

  // @ts-expect-error  `unlit` is a boolean
  assets.add(StandardMaterial, { unlit: "yes" });
  // @ts-expect-error  no such field
  assets.add(StandardMaterial, { roughnes: 0.5 });
  // @ts-expect-error  a Mesh has no default: `add` cannot build one
  assets.add(Mesh, {});
  // @ts-expect-error  a material ref is not an image
  assets.add(StandardMaterial, { base_color_texture: mat });

  // Values: a plain description, for any handle slot of their type.
  const v = StandardMaterial.of({ unlit: true });
  assets.add(StandardMaterial, { base_color_texture: Image.of({}) });
  // @ts-expect-error  a material value is not an image
  assets.add(StandardMaterial, { base_color_texture: v });
  // @ts-expect-error  Mesh has no `of`
  void Mesh.of;
  void MeshMaterial3dStandardMaterial;
}
