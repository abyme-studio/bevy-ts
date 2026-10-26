import type { AssetId, AssetRef, AssetType, LiveImage, LiveMesh, LoadState, Mesh, MeshData } from "bevy-ts";
import type { Image, NewImage } from "bevy-ts/bevy";
export { Indices, Mesh, PrimitiveTopology, RenderAssetUsages } from "bevy-ts";
/** `ResMut<Assets<Mesh>>`: Bevy's meshes. The other methods are the host calls these make. */
export interface Meshes {
  /** Builds a `Mesh.new` mesh in Rust and adds it: one call per attribute, one for indices, one for the computed data. Returns a strong ref; `assets.drop(ref)` lets it go. A failed step adds nothing. */
  add(mesh: Mesh): AssetRef<MeshData>;
  /** The mesh, read-only, or `undefined` when it is gone. */
  get(ref: AssetRef<MeshData>): LiveMesh | undefined;
  /** The live mesh: each write is one host call into the asset; Bevy sends one `Modified` per frame it changes. */
  getMut(ref: AssetRef<MeshData>): LiveMesh | undefined;
}
/** `ResMut<Assets<Image>>`: Bevy's images, made and rewritten from bytes. The other methods are the host calls these make. */
export interface Images {
  /** Makes an `Image.new` image in one host call, its bytes copied once. Returns a strong ref; `assets.drop(ref)` lets it go. A wrong length adds nothing. */
  add(image: NewImage): AssetRef<Image>;
  /** The image, read-only, or `undefined` when it is gone. */
  get(ref: AssetRef<Image>): LiveImage | undefined;
  /** The live image: `setData` and `writeRect` write into it in place; Bevy sends one `Modified` per frame it changes. */
  getMut(ref: AssetRef<Image>): LiveImage | undefined;
  /** Bevy's `Assets::remove`, at once: whether it removed one. The ref goes stale. */
  remove(ref: AssetRef<Image>): boolean;
}
/** `Res<AssetServer>`: Bevy's loads. Each load-state method takes an `AssetRef` or an `AssetId` (`ref.id`, an `AssetEvent<A>`'s) and is one host call; a failed state makes one more for its text. The other methods are the host calls these make. */
export interface ScriptAssetServer {
  /** Bevy's `load`: a strong ref, typed by `T` or by the loader of the path's extension. A labelled path (`"fox.glb#Mesh0/Primitive0"`) needs `T`. Loading a path again gives the same ref. */
  load(path: string, T: typeof Mesh): AssetRef<MeshData>;
  load<F>(path: string, T: AssetType<F>): AssetRef<F>;
  load(path: string): AssetRef<unknown>;
  loadState(asset: AssetRef<unknown> | AssetId<unknown>): LoadState;
  dependencyLoadState(asset: AssetRef<unknown> | AssetId<unknown>): LoadState;
  recursiveDependencyLoadState(asset: AssetRef<unknown> | AssetId<unknown>): LoadState;
  isLoaded(asset: AssetRef<unknown> | AssetId<unknown>): boolean;
  isLoadedWithDirectDependencies(asset: AssetRef<unknown> | AssetId<unknown>): boolean;
  isLoadedWithDependencies(asset: AssetRef<unknown> | AssetId<unknown>): boolean;
}
