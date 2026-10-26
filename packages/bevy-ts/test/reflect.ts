// Type tests for reflected rows in TS systems against the generated `bevy-ts/bevy` (reflect-coverage
// stage 3): docs/native.md's examples verbatim. `tsc -p test` must pass, and every
// `@ts-expect-error` must be an error.
import { type ColorValue, GlobalTransform, type Mut, type Option, type Quat, type Query, ref, type Transform, type Vec3, type With } from "bevy-ts";
import { Camera3d, DirectionalLight, Projection, TextShadow, Vignette } from "bevy-ts/bevy";
import type { ProjectionValue } from "bevy-ts/bevy";

// docs/native.md, "Reflected rows in TS systems".
export function ortho(q: Query<[Mut<Projection>], With<Camera3d>>) {
  q.each((p) => {
    if ("Perspective" in p.value) p.value = { Orthographic: {} };
  });
}

export function inPlace(q: Query<[Mut<Projection>]>) {
  q.each((p) => {
    if ("Orthographic" in p.value) p.value.Orthographic.near = 0;
  });
}

export function warm(q: Query<[Mut<DirectionalLight>]>) {
  q.each((light) => {
    light.color = "#ffd7a8";
    light.illuminance = 8000;
  });
}

export function look(q: Query<[Projection, DirectionalLight, Option<Projection>]>) {
  q.each((p, light, maybe) => {
    const v: ProjectionValue = p.value;
    const c: ColorValue = light.color;
    const m: ProjectionValue | undefined = maybe?.value;
    // @ts-expect-error a read row is read-only
    light.color = "#ffffff";
    // @ts-expect-error a reflect-tier row's value is its enum, not any object
    const bad: { Fixed: number } = p.value;
    void [v, c, m, bad];
  });
}

const sun = ref();
export const sunColor = (): ColorValue => sun.get(DirectionalLight).color;

// A class with a `Vec2` column takes `[x, y]` as an element; `.of()` still takes its holes.
export const vignette = Vignette.of({ color: "#f00", intensity: 0.5 });
export const shadow = TextShadow.of({ color: "#000" });

// reflect-bindable: `GlobalTransform` rows read with Bevy's methods; `bevy-ts/bevy` re-exports it.
export function where(q: Query<[GlobalTransform]>, parents: Query<[GlobalTransform], With<Camera3d>>) {
  q.each((g) => {
    const t: Vec3 = g.translation();
    const r: Quat = g.rotation();
    const [s, r2, t2]: [Vec3, Quat, Vec3] = g.toScaleRotationTranslation();
    const local: Transform = g.computeTransform();
    const m: number[] = g.toMatrix();
    const f: Vec3 = g.forward();
    const p: Vec3 = g.transformPoint([0, 0, 1]);
    const moved: GlobalTransform = g.mulTransform({ translation: [1, 0, 0] });
    parents.each((parent) => void g.reparentedTo(parent));
    // @ts-expect-error a read row's value is read-only
    g.value = [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0];
    void [t, r, s, r2, t2, local, m, f, p, moved];
  });
}
// @ts-expect-error Bevy computes GlobalTransform, so a Mut row has nothing to read or write
export function nudge(q: Query<[Mut<GlobalTransform>]>) { q.each((g) => g.translation()); }
