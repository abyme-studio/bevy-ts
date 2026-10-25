# Gizmos

A TS system draws Bevy's immediate-mode gizmos by taking `Gizmos`, as a Rust system does:

```ts
import { app, Update } from "bevy-ts";
import type { Gizmos, LightGizmoConfigGroup } from "bevy-ts/gizmos";

export function draw(gizmos: Gizmos) {
  gizmos.line([0, 0, 0], [1, 2, 0], "#f00");
  gizmos.circle({ translation: [0, 1, 0] }, 1, "#0f0").resolution(64);
  gizmos.linestrip(path, "#fff"); // a Vec3[] or a Float32Array of 3 floats per point
}
export function lights(gizmos: Gizmos<LightGizmoConfigGroup>) {
  if (gizmos.configExt().draw_all) gizmos.cross([0, 0, 0], 0.5, "#ff0");
}
app.addSystems(Update, draw).addSystems(Update, lights);
```

The param is Bevy's `Gizmos<C>`: what a frame draws is drawn that frame, a system in `FixedUpdate`
draws into the fixed context, and a system that throws drops what it drew. A `Gizmos` kept past its
system throws `Gizmos.line: this system does not take Gizmos`. The design is the OpenSpec change
`script-gizmos`.

## Groups

`Gizmos` draws into `DefaultGizmoConfigGroup`; `Gizmos<G>` into group `G`. `bevy-ts/gizmos` exports
Bevy's five: `DefaultGizmoConfigGroup`, `AabbGizmoConfigGroup`, `FrustumGizmoConfigGroup`,
`SkinnedMeshBoundsGizmoConfigGroup`, `LightGizmoConfigGroup`. A Rust crate exposes its own group
after `init_gizmo_group`:

```rust
app.init_gizmo_group::<PathGizmos>().ts_gizmo_group::<PathGizmos>();
```

`bevy-ts types` then declares `PathGizmos` in `bevy-ts/gizmos`. A group never `init_gizmo_group`ed
fails the app at start, naming it; a `Gizmos<Nope>` param fails the load, naming the groups there are.

## Argument forms

- Points: `[x, y, z]` or `{ x, y, z }`. Lists of points (`linestrip`, `lineloop`, curve `times`):
  an array of points or a `Float32Array`; the array is packed into one reused buffer, so a long strip
  is one call.
- Colours: every `ColorValue`: `"#f00"`, `srgb(1, 0, 0)`, `[1, 0, 0, 1]`, `{ Hsla: { ... } }`.
- Isometries: a translation (`[x, y, z]`), a rotation (`{ x, y, z, w }`), or
  `{ translation, rotation }`; in 2D a rotation is `{ cos, sin }`.
- `cube` and `axes` take a `Transform` value or a `GlobalTransform` column.
- Gradients and text sections: `[[point, color], ...]`, at most about 60 to a call; past it the call
  throws and says to split it.

## Builders

A Bevy method that returns a builder draws at once and returns its settings:
`gizmos.arrow(a, b, "#fff").withTipLength(0.2).withDoubleEnd()`. A setting redraws the shape, so
call it right after the call that returned the builder; later, it throws
`ArrowBuilder.withTipLength: call it right after the call that returned the builder`.

## Primitives and curves

`primitive2d` and `primitive3d` take a Bevy primitive as a one-key object of its reflected value,
written over the type's default: `gizmos.primitive3d({ Cuboid: { half_size: [1, 1, 1] } }, [0, 0, 0], "#fff")`.
Every primitive Bevy can draw is there; settings chain (`.resolution(16)`), and one the shape lacks
throws. `curve2d`/`curve3d` and their gradients take `EasingCurve`, `SampleAutoCurve`,
`UnevenSampleAutoCurve` or `ConstantCurve` the same way. A curve built from a closure has no TS form.

## Config

`gizmos.config()` and `gizmos.configExt()` read the group's `GizmoConfig` and its own value. A
system taking `ResMut<GizmoConfigStore>` writes them:

```ts
import { DefaultGizmoConfigGroup, GizmoConfigStore, LightGizmoConfigGroup } from "bevy-ts/gizmos";
export function toggle(store: ResMut<GizmoConfigStore>) {
  store.insert(DefaultGizmoConfigGroup, { line: { width: 4, style: { Dashed: { gap_scale: 2, line_scale: 1 } } } }, {});
  store.insert(LightGizmoConfigGroup, {}, { draw_all: true, color: "ByLightType" });
}
```

`insert` writes over the current value (keys left out keep theirs) and changes nothing when a field
is bad. `Res<GizmoConfigStore>` reads (`config`, `groups`) and has no `insert`.

## Cost

Every gizmo call crosses from JS to Rust. `bench/gizmos` measures 1,000 calls a frame on a headless
release engine: the time inside the drawing system, median of 8 runs at a load average of 40 to 65
(the frame totals were within the machine's noise). Rust is criterion's `lines_1000` over `empty`.

| 1,000 lines a frame | ms | ns per line |
|---|---|---|
| Rust, `gizmos.line` | 0.031 | 31 |
| TS, `line(a, b, [1, 0, 0])` | 0.154 | 154 |
| TS, `line(a, b, "#f00")` | 0.255 | 255 |
| TS, `line(a, b, srgb(1, 0, 0))` | 0.259 | 259 |
| TS, one `linestrip` of a 1,000-point `Float32Array` | 0.029 | 29 |
| TS, 1,000 `Keys.pressed` calls (any exported call's floor) | 0.065 | 65 |

So:

- Many segments: one `linestrip` (`linestrip2d`, `lineloop2d`) of a `Float32Array` costs
  what Rust does. Build the array once and rewrite it in place.
- A colour as `[r, g, b]` (or `[r, g, b, a]`) is the cheapest form; a hex string or a `srgb()`
  object costs about 100 ns more a call.
- A few hundred separate calls a frame cost tenths of a millisecond; thousands want a strip.

`bench/gizmos/README.md` says how to run it; `bench/gizmos/results/` has the runs.

## Coverage

Every public gizmo item in Bevy 0.20, its TS form, its status and the test that holds it (from
`docs/research/script-gizmos.md`). Paths are under `bevy_gizmos-0.20.0/src/` unless named. Tests are
in `crates/bevy-ts-cli/tests/it/gizmos.rs`; "same-buffer" is `ts_draws_what_rust_draws`, which makes
each call from TS and in Rust and compares the two buffers vertex for vertex.
`docs_rows_name_real_tests` fails for a row marked works whose test is missing.

## The param

| Bevy | TS | Status | Test |
|---|---|---|---|
| `Gizmos<'w, 's, Config = DefaultGizmoConfigGroup, Clear = ()>` (`gizmos.rs:143`) | a system param `gizmos: Gizmos`, or `Gizmos<LightGizmoConfigGroup>`; one export per registered group standing for that spelling  | works | `a_system_draws_lines`, `each_group_draws_into_its_own_storage` |
| `Clear` contexts (`gizmos.rs:92` `Swap`, `lib.rs:224-275`) | none: TS names no context; a TS system in `FixedUpdate` draws into the fixed context as a Rust one does, since Bevy swaps the storage, not the param | works through the param | `fixed_update_gizmos_last_until_the_next_fixed_step` |
| `Gizmos::config`, `config_ext` (`gizmos.rs:150-152`) | `gizmos.config()`, `gizmos.configExt()`: the group's `GizmoConfig` and group value as JSON through `reflect_value::read` | works | `config_reads_through_reflection` |
| A system that does not take the param | throws `Gizmos.line: this system does not take Gizmos` (`crates/bevy-ts/src/host.rs:697`) | works | `a_gizmos_kept_from_another_system_throws` |
| Global `gizmo()` (`global.rs:46`) | no TS form: TS draws only through a declared param, so the throw above holds | — | — |
| `GizmoBuffer` fields, `clear`, `buffer()` (`gizmos.rs:281-399`) | no TS form: buffer internals; a drawing system never reads them | — | — |
| `GizmoStorage`, `append_storage`, `start/end/collect/clear/propagate_gizmo_context` (`gizmos.rs:33-80`, `lib.rs:224-290`) | no TS form: Bevy's scheduling internals | — | — |

## Lines, rays, strips

| Bevy | TS | Status | Test |
|---|---|---|---|
| `line(start, end, color)` (`gizmos.rs:403`) | `gizmos.line(start, end, color)` | works | same-buffer |
| `line_gradient(start, end, start_color, end_color)` (`gizmos.rs:424`) | `lineGradient(...)` | works | same-buffer |
| `ray(start, vector, color)` (`gizmos.rs:451`) | `ray(...)` | works | same-buffer |
| `ray_gradient` (`gizmos.rs:471`) | `rayGradient(...)` | works | same-buffer |
| `linestrip(positions, color)` (`gizmos.rs:497`) | `linestrip(positions, color)`; `positions` a `Vec3[]` or a `Float32Array` of 3 per point | works | same-buffer, `a_long_strip_crosses_as_a_slice` |
| `lineloop` (`gizmos.rs:525`) | `lineloop(positions, color)` | works | same-buffer |
| `linestrip_gradient(points)` (`gizmos.rs:564`) | `linestripGradient([[p, color], ...])` | works | same-buffer |
| `line_2d`, `line_gradient_2d`, `linestrip_2d`, `lineloop_2d`, `linestrip_gradient_2d`, `ray_2d`, `ray_gradient_2d` (`gizmos.rs:722-867`) | `line2d`, `lineGradient2d`, `linestrip2d`, `lineloop2d`, `linestripGradient2d`, `ray2d`, `rayGradient2d`; 2D positions `Vec2[]` or 2 floats per point | works | same-buffer |

## Shapes

| Bevy | TS | Status | Test |
|---|---|---|---|
| `rect(isometry, size, color)` (`gizmos.rs:610`), `rect_2d` (`gizmos.rs:898`) | `rect`, `rect2d`; an isometry is a `Vec3`, a `Quat` or `{ rotation, translation }` | works | same-buffer, `isometry_takes_bevys_into_forms` |
| `cube(transform, color)` (`gizmos.rs:632`) | `cube(transform, color)`; a `Transform`-shaped value or a `GlobalTransform` | works | same-buffer |
| `aabb_3d(aabb, transform, color)` (`gizmos.rs:672`) | `aabb3d({ min, max }, transform, color)` | works | same-buffer |
| `cross`, `cross_2d` (`cross.rs:32,65`) | `cross`, `cross2d` | works | same-buffer |
| `circle`, `circle_2d`, `ellipse`, `ellipse_2d` (`circles.rs:50-167`) + `.resolution(n)` (`circles.rs:242,283`) | same names, `.resolution(n)` chained | works | same-buffer, `a_builder_setting_redraws_the_shape` |
| `sphere` (`circles.rs:207`) + `.resolution` (`circles.rs:331`) | `sphere(...).resolution(n)` | works | same-buffer |
| `arc_2d`, `arc_3d`, `short_arc_3d_between`, `long_arc_3d_between`, `short_arc_2d_between`, `long_arc_2d_between` (`arcs.rs:46-402`) + `.resolution` (`arcs.rs:84,472`) | `arc2d`, `arc3d`, `shortArc3dBetween`, `longArc3dBetween`, `shortArc2dBetween`, `longArc2dBetween` | works | same-buffer |
| `arrow`, `arrow_2d` (`arrows.rs:129,158`) + `.with_tip_length`, `.with_double_end` (`arrows.rs:48,55`) | `arrow(...).withTipLength(l).withDoubleEnd()` | works | same-buffer |
| `axes`, `axes_2d` (`arrows.rs:197,228`) | `axes(transform, baseLength)`, `axes2d` | works | same-buffer |
| `grid`, `grid_3d`, `grid_2d` (`grid.rs:217-319`) + `.skew_x/_y/_z`, `.skew`, `.outer_edges_x/_y/_z`, `.outer_edges` (`grid.rs:46-133`) | same names camelCased; `cellCount` a `UVec2`/`UVec3` as `[x, y]` | works | same-buffer |
| `rounded_rect`, `rounded_rect_2d`, `rounded_cuboid` (`rounded_box.rs:267-373`) + `.corner_radius`, `.arc_resolution`, `.edge_radius` (`rounded_box.rs:47-67`) | `roundedRect(...).cornerRadius(r).arcResolution(n)`, `roundedCuboid(...).edgeRadius(r)` | works | same-buffer |

## Primitives

`primitive_2d(&P, isometry, color)` (`primitives/dim2.rs:30`) and `primitive_3d` (`primitives/dim3.rs:27`):
TS `gizmos.primitive2d(shape, isometry, color)` and `primitive3d`, where `shape` is the primitive's
reflected value under its type name, `{ Cuboid: { half_size: [0.5, 0.5, 0.5] } }`, written through
`reflect_value::write`. Builder settings chain as for shapes.

| Bevy impl | Builder settings | Status | Test |
|---|---|---|---|
| 2D: `Dir2` (`dim2.rs:40`), `Arc2d` (68), `Circle` (102), `CircularSector` (124), `CircularSegment` (167), `Ellipse` (209), `Annulus` (271), `Rhombus` (331), `Capsule2d` (365), `Line2d` (458), `Plane2d` (516), `Segment2d` (596), `Polyline2d` (649), `Triangle2d` (684), `Rectangle` (714), `Polygon` (750), `RegularPolygon` (785) | `Annulus`: `resolution`, `outerResolution`, `innerResolution` (`dim2.rs:252-265`); `Line2d`, `Segment2d`: `drawArrow` (`dim2.rs:452,590`) | works | same-buffer, one row per impl |
| 3D: `Dir3` (`dim3.rs:37`), `Sphere` (62), `Plane3d` (123), `Line3d` (175), `Segment3d` (210), `Polyline3d` (237), `Triangle3d` (267), `Cuboid` (295), `Cylinder` (390), `Capsule3d` (476), `Cone` (616), `ConicalFrustum` (715), `Torus` (819), `Tetrahedron` (899) | `Plane3d`: `cellCount`, `spacing` (`dim3.rs:111,117`); `Cylinder`, `Capsule3d`, `ConicalFrustum`: `resolution`; `Cone`: `resolution`, `baseResolution`, `heightResolution` (`dim3.rs:591-610`); `Torus`: `minorResolution`, `majorResolution` (`dim3.rs:807,813`) | works | same-buffer, one row per impl |

## Curves and text

| Bevy | TS | Status | Test |
|---|---|---|---|
| `curve_2d(curve, times, color)`, `curve_3d` (`curves.rs:41,77`) | `curve3d(curve, times, color)`; `curve` a reflected Bevy curve, `{ EasingCurve: { start, end, ease_fn } }`, `{ SampleAutoCurve: ... }`, `{ UnevenSampleAutoCurve: ... }`, `{ ConstantCurve: ... }` (`bevy_curve/src/easing.rs:335`, `sample_curves.rs:140,315`, `adaptors.rs:47`); `times` a `number[]` or `Float32Array` | works | same-buffer, `a_curve_outside_the_set_throws_naming_it` |
| `curve_gradient_2d`, `curve_gradient_3d` (`curves.rs:114,159`) | `curveGradient3d(curve, [[t, color], ...])` | works | same-buffer |
| A curve built from a closure (`FunctionCurve`, `map`, `chain`, ...) | no TS form: a closure is Rust code, and a call never calls back into JS (`docs/native.md`, "Method calls are synchronous") | — | — |
| `text(isometry, text, font_size, anchor, color)`, `text_2d` (`stroke_text.rs:211,295`) | `text(isometry, "hp 10", 0.2, [0, 0], color)`, `text2d` | works | same-buffer |
| `text_sections`, `text_sections_2d` (`stroke_text.rs:253,337`) | `textSections(isometry, [["hp ", white], ["10", red]], size, anchor)` | works | same-buffer |
| `StrokeFont`, `StrokeTextLayout` (`stroke_text.rs:10-123`) | no TS form: layout internals of `text` | — | — |

## Config

| Bevy | TS | Status | Test |
|---|---|---|---|
| `GizmoConfigStore` (`config.rs:94`): `config`, `get_config`, `insert`, `iter` (`config.rs:116-212`) | `ResMut<GizmoConfigStore>` (an export standing for it): `store.config(LightGizmoConfigGroup)` gives `[GizmoConfig, LightGizmoConfigGroup]` as JSON; `store.insert(Group, config, ext)` writes both through `reflect_value::write` over the current value; `store.groups()` lists registered group names | works | `config_round_trips_through_the_store`, `insert_merges_over_the_current_config` |
| `config_mut`, `get_config_mut`, `iter_mut`, `*_dyn` (`config.rs:137-200`) | no TS form: they return references; a JS copy cannot write back. Read with `config`, write with `insert` | — | — |
| `GizmoConfig.enabled` (`config.rs:221`) | `config.enabled` | works | `disabled_group_draws_nothing` |
| `GizmoConfig.line` → `GizmoLineConfig` (`config.rs:255`): `width`, `perspective`, `style` (`Solid`, `Dotted`, `Dashed { gap_scale, line_scale }`, `config.rs:38`), `joints` (`None`, `Miter`, `Round(u32)`, `Bevel`, `config.rs:20`), `animation_offset` | `config.line.width`, `style: { Dashed: { gap_scale, line_scale } }`, `joints: { Round: 4 }`, as reflection writes them (`docs/reflection.md`) | works | `config_round_trips_through_the_store`, render: `line_width_and_style_render` |
| `GizmoConfig.depth_bias`, `render_layers` (`config.rs:235,240`) | `config.depth_bias`, `config.render_layers` (`RenderLayers` reflects as a list) | works | `config_round_trips_through_the_store` |
| `GizmoMeshConfig` (`config.rs:286`) | no TS form: a render-side component Bevy fills from the config | — | — |
| `AppGizmoBuilder::init_gizmo_group`, `insert_gizmo_config` (`lib.rs:126-195`) | Rust only: a group is a Rust type. A crate exposes its group with `app.ts_gizmo_group::<G>()` | works | `a_project_group_reaches_ts` |
| `GizmoConfigGroup` derive, `DefaultGizmoConfigGroup`, `ErasedGizmoConfigGroup` (`config.rs:78-91`) | `DefaultGizmoConfigGroup` exported from `bevy-ts/gizmos`; `ErasedGizmoConfigGroup` none (retained gizmos only) | works | `config_round_trips_through_the_store` |

## Groups and markers

| Bevy | TS | Status | Test |
|---|---|---|---|
| `AabbGizmoConfigGroup { draw_all, default_color }` (`aabb.rs:45`) | `Gizmos<AabbGizmoConfigGroup>`, `store.config(AabbGizmoConfigGroup)` | works | `each_group_draws_into_its_own_storage`, `draw_all_draws_every_aabb` |
| `ShowAabbGizmo { color }` (`aabb.rs:63`) | `<ShowAabbGizmo color="#f00" />` | works (reflected component) | `show_aabb_gizmo_draws_its_box` |
| `FrustumGizmoConfigGroup` (`frustum.rs:80`), `ShowFrustumGizmo` (`frustum.rs:98`) | as for Aabb | works | `show_frustum_gizmo_draws` |
| `SkinnedMeshBoundsGizmoConfigGroup` (`skinned_mesh_bounds.rs:54`), `ShowSkinnedMeshBoundsGizmo` (`skinned_mesh_bounds.rs:78`) | as for Aabb | works | `config_round_trips_through_the_store` (the group); marker: none, it needs a skinned glTF, which examples lack |
| `LightGizmoConfigGroup { draw_all, color, point/spot/directional/rect_light_color }` (`bevy_light/src/gizmos.rs:165`), `LightGizmoColor` (`:150-160`) | as for Aabb; `color: "ByLightType"` or `{ Manual: "#fff" }` | works | `show_light_gizmo_draws` |
| `ShowLightGizmo { color }` (`bevy_light/src/gizmos.rs:209`) | `<ShowLightGizmo color="MatchLightColor" />` | works | `show_light_gizmo_draws` |
| bevy-ts's own `EditorGizmos`, `OrientGizmos` (`crates/bevy-ts-viewport/src/edit.rs:50`, `orient.rs:82`) | none: edit tooling, never registered with `ts_gizmo_group` | — | `the_editor_groups_are_not_exposed` |

## Retained gizmos

| Bevy | TS | Status | Test |
|---|---|---|---|
| `GizmoAsset` (`lib.rs:325`), `GizmoAsset::new`, `Default`, the drawing API through `DerefMut` (`retained.rs:16-28`) | `const g = new GizmoAsset(); g.sphere([0, 0, 0], 1, "#f00").resolution(64);` every `Gizmos` drawing method, recorded in JS and replayed in Rust on add | planned: stage 3, after reflect-assets | `a_gizmo_asset_holds_what_gizmos_would_draw` |
| `Assets<GizmoAsset>::add` | `assets.add(GizmoAsset, g)` on reflect-assets' path, a hand-map entry for `GizmoAsset`; returns an `AssetRef<GizmoAsset>`. `assets.set(ref, g2)` replaces its calls | planned: stage 3, after reflect-assets | `a_gizmo_asset_holds_what_gizmos_would_draw`, `set_replaces_a_gizmo_assets_calls` |
| `Gizmo { handle, line_config, depth_bias }` (`retained.rs:67`) | `<Gizmo handle={ref} line_config={{ width: 4 }} depth_bias={-0.1} />`: `depth_bias` a column (works), `line_config` a reflected hole prop, `handle` an asset ref through reflect-assets' handle hook | `depth_bias` works; the rest planned: stage 3 | `a_retained_gizmo_renders`, render frame |
| `GizmoAsset::buffer`, `config_typeid` (`lib.rs:333,348`) | no TS form: internals | — | — |
| `GizmoHandles` (`lib.rs:205`) | no TS form: Bevy's per-group asset handles | — | — |

## Render side and transform gizmo

| Bevy | TS | Status | Test |
|---|---|---|---|
| `GizmoRenderPlugin`, `LineGizmo2dPlugin`, `LineGizmo3dPlugin`, `GizmoRenderSystems`, `LineGizmoEntities` (`bevy_gizmos_render/src/lib.rs:11,85,629`, `pipeline_2d.rs:37`, `pipeline_3d.rs:37`) | none: the engine adds them (`3d` feature, `crates/bevy-ts-cli/Cargo.toml:21`); TS sees their output | works | render: `gizmos_render_headless` |
| `TransformGizmoPlugin`, `TransformGizmoSettings`, `TransformGizmoMode/Space/Axis`, `TransformGizmoSystems`, helper fns (`transform_gizmo.rs:86-697`) | none in games: the engine adds the plugin only in edit mode (`crates/bevy-ts-viewport/src/edit.rs:304`); the studio drives it | out of scope | — |
| `TransformGizmoFocus`, `TransformGizmoCamera` (`transform_gizmo.rs:89,99`), `TransformGizmoState` (`:185`) | reflected components and a layout resource (`bevy.js:143-147`) | works (edit tooling) | — |

## Not this change

- Colour palettes (`bevy::color::palettes::css::RED`): TS writes `"#f00"` or `srgb(1, 0, 0)`; a
  palette module is a colour concern, not gizmos.
- The `Clear` type parameter for custom contexts: a TS system cannot define a Rust schedule pair to
  start and end one.
