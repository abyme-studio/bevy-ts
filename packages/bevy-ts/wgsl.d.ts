// A `.wgsl` import is a shader file, for `material()` in `bevy-ts/material`. The engine resolves
// it relative to the importing file and watches it; its WGSL never runs as JS.
declare module "*.wgsl" {
  const shader: { readonly kind: "shaderFile"; readonly path: string };
  export default shader;
}
