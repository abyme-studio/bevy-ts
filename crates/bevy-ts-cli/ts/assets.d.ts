import type { AssetRef, LiveMesh, LoadState, Mesh, MeshData } from "bevy-ts";
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
/** `Res<AssetServer>`: Bevy's loads. Each method is one host call; a failed state makes one more for its text. The other methods are the host calls these make. */
export interface ScriptAssetServer {
  /** Bevy's `load`: a strong ref, typed by `T` or by the loader of the path's extension. A labelled path (`"fox.glb#Mesh0/Primitive0"`) needs `T`. Loading a path again gives the same ref. */
  load<F = unknown>(path: string, T?: { readonly typePath: string; readonly [k: symbol]: unknown } | { readonly typePath: string }): AssetRef<F>;
  loadState(ref: AssetRef<unknown>): LoadState;
  dependencyLoadState(ref: AssetRef<unknown>): LoadState;
  recursiveDependencyLoadState(ref: AssetRef<unknown>): LoadState;
}
