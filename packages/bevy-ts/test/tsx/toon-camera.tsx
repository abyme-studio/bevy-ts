// The toon camera in docs/materials.md type-checks: an orthographic `Projection` set by its
// variant, both prepass markers and no tonemapping.
import { app } from "bevy-ts";
import { Camera3d, DepthPrepass, NormalPrepass, Projection, Tonemapping, Transform } from "bevy-ts/bevy";

app.scene("toon-camera", () => (
  <entity name="camera">
    <Camera3d />
    <Projection value={{ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 13 } } } }} />
    <DepthPrepass />
    <NormalPrepass />
    <Tonemapping value="None" />
    <Transform translation={[0, 20, 14]} rotation={[-0.4617, 0, 0, 0.887]} />
  </entity>
));
