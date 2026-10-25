// Type tests for reflected rows in TS systems against the generated `bevy-ts/bevy` (reflect-coverage
// stage 3): docs/native.md's examples verbatim. `tsc -p test` must pass, and every
// `@ts-expect-error` must be an error.
import { type ColorValue, type Mut, type Option, type Query, ref, type With } from "bevy-ts";
import { Camera3d, DirectionalLight, Projection } from "bevy-ts/bevy";
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
