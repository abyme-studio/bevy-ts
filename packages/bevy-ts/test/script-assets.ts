// Type tests for script-assets (docs/assets.md): `Res<AssetServer>` and `ResMut<Assets<Mesh>>` are
// the cli's exports, and `load` is typed by its class. The block below is what `bevy-ts types`
// writes into native.d.ts for them.
import { assets, type AssetRef, Indices, Mesh, PrimitiveTopology, type Res, type ResMut, type AssetServer, type Assets, type LoadState, type MeshData } from "bevy-ts";
import { AssetEvent, Image, type Mesh3d, StandardMaterial } from "bevy-ts/bevy";
import type { MessageReader } from "bevy-ts";

declare module "bevy-ts" {
  interface ResClaims {
    "Res<AssetServer>": [AssetServer, import("bevy-ts/assets").ScriptAssetServer];
  }
  interface ResMutClaims {
    "ResMut<Assets<Mesh>>": [Assets<Mesh>, import("bevy-ts/assets").Meshes];
  }
}

export function loads(server: Res<AssetServer>) {
  const img: AssetRef<Image> = server.load("tex.png", Image);
  const mesh = server.load("fox.glb#Mesh0/Primitive0", Mesh);
  const any: AssetRef<unknown> = server.load("scene.gltf");
  const state: LoadState = server.loadState(img);
  const done: boolean = server.isLoadedWithDependencies(any);
  void mesh, state, done;
  // @ts-expect-error  a loaded image is not a material
  const wrong: AssetRef<StandardMaterial> = server.load("tex.png", Image);
  // @ts-expect-error  the type is an asset class, not its name
  server.load("tex.png", "Image");
  void wrong;
}

export function chunk(meshes: ResMut<Assets<Mesh>>) {
  const m = Mesh.new(PrimitiveTopology.TriangleList)
    .insertAttribute(Mesh.ATTRIBUTE_POSITION, new Float32Array(9))
    .insertIndices(Indices.U32(new Uint32Array([0, 1, 2])));
  const r = meshes.add(m);
  // A ref from `meshes.add` goes into a `Handle<Mesh>` slot.
  const slot: Mesh3d["value"] = r;
  void slot;
  meshes.getMut(r)?.insertAttribute(Mesh.ATTRIBUTE_NORMAL, new Float32Array(9));
  // @ts-expect-error  normals are f32
  meshes.getMut(r)?.insertAttribute(Mesh.ATTRIBUTE_NORMAL, new Uint16Array(9));
  // @ts-expect-error  a material ref is not a mesh
  meshes.get(assets.add(StandardMaterial, {}));
  assets.drop(r);
}

// `Mesh` as asset data: Bevy-named JSON through `assets` and `Mesh.of`.
export function data() {
  const v = Mesh.of({ primitive_topology: "TriangleList", attributes: { Vertex_Position: [[0, 0, 0], [1, 0, 0], [0, 1, 0]] }, indices: { U16: [0, 1, 2] } });
  const slot: Mesh3d["value"] = v;
  const r: AssetRef<MeshData> = assets.add(Mesh, { attributes: { Vertex_Position: [[0, 0, 0]] } });
  const topology: string | undefined = assets.get(r)?.primitive_topology;
  assets.set(r, { indices: null });
  // @ts-expect-error  topology is one of Bevy's names
  Mesh.of({ primitive_topology: "Triangles" });
  void slot, topology;
}

// `AssetEvent<A>` values have Bevy's `is*` methods; their ids reach the load-state methods.
export function events(server: Res<AssetServer>, r: MessageReader<AssetEvent<Image>>) {
  const img = server.load("tex.png", Image);
  r.each((e) => {
    const added: boolean = e.isAdded(img) || e.isModified(img.id) || e.isLoadedWithDependencies(img);
    if ("Added" in e) {
      const state: LoadState = server.loadState(e.Added.id!);
      void state;
    }
    // @ts-expect-error  an asset id, not a path
    e.isRemoved("tex.png");
    void added;
  });
}
