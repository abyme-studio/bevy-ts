# Gizmos from TS: coverage

Every public gizmo item in Bevy 0.20, its TS form, its status and the test that will hold it.
Researched 2026-10-25 from `bevy_gizmos-0.20.0`, `bevy_gizmos_render-0.20.0` and `bevy_light-0.20.0`
(`~/dev/refs/bevy-0.20`, tag `b3bdd75`), against bevy-ts main at `0bf5e27a`. The plan is the
OpenSpec change `script-gizmos`; its design numbers are cited as "design N". When the change lands,
`docs/gizmos.md` carries this table with statuses moved to "works".

Today TS has the `Show*Gizmo` markers and `Gizmo` as reflected components (`packages/bevy-ts/bevy.d.ts:294-310,469`,
`crates/bevy-ts-v8/js/bevy.js:135-145,205`) and nothing else. Exported params can write and defer
since writable-native-params (`docs/native.md:344`); `Gizmos` is named there as a deferred param, and
`crates/bevy-ts-cli/tests/it/native_writes.rs:866` already applies a Rust `Gizmos` export after a run.

Paths below are under `crates/bevy_gizmos/src/` unless named. Status: "new" means this change adds
it; "works" means it works today; "no TS form" gives the reason. Tests are in
`crates/bevy-ts-cli/tests/it/gizmos.rs` unless named; "same-buffer" means the table-driven test
`ts_draws_what_rust_draws`, which runs each call from TS and the same call in Rust and compares the
two `GizmoBuffer`s vertex for vertex (design 9).

## The param

| Bevy | TS | Status | Test |
|---|---|---|---|
| `Gizmos<'w, 's, Config = DefaultGizmoConfigGroup, Clear = ()>` (`gizmos.rs:143`) | a system param `gizmos: Gizmos`, or `Gizmos<LightGizmoConfigGroup>`; one export per registered group standing for that spelling (design 1, 2) | new | `a_system_draws_lines`, `each_group_draws_into_its_own_storage` |
| `Clear` contexts (`gizmos.rs:92` `Swap`, `lib.rs:224-275`) | none: TS names no context; a TS system in `FixedUpdate` draws into the fixed context as a Rust one does, since Bevy swaps the storage, not the param | works through the param | `fixed_update_gizmos_last_until_the_next_fixed_step` |
| `Gizmos::config`, `config_ext` (`gizmos.rs:150-152`) | `gizmos.config()`, `gizmos.configExt()`: the group's `GizmoConfig` and group value as JSON through `reflect_value::read` (design 6) | new | `config_reads_through_reflection` |
| A system that does not take the param | throws `Gizmos.line: this system does not take Gizmos` (`crates/bevy-ts/src/host.rs:697`) | works for every export; new test for gizmos | `a_gizmos_kept_from_another_system_throws` |
| Global `gizmo()` (`global.rs:46`) | no TS form: TS draws only through a declared param, so the throw above holds | — | — |
| `GizmoBuffer` fields, `clear`, `buffer()` (`gizmos.rs:281-399`) | no TS form: buffer internals; a drawing system never reads them | — | — |
| `GizmoStorage`, `append_storage`, `start/end/collect/clear/propagate_gizmo_context` (`gizmos.rs:33-80`, `lib.rs:224-290`) | no TS form: Bevy's scheduling internals | — | — |

## Lines, rays, strips

| Bevy | TS | Status | Test |
|---|---|---|---|
| `line(start, end, color)` (`gizmos.rs:403`) | `gizmos.line(start, end, color)` | new | same-buffer |
| `line_gradient(start, end, start_color, end_color)` (`gizmos.rs:424`) | `lineGradient(...)` | new | same-buffer |
| `ray(start, vector, color)` (`gizmos.rs:451`) | `ray(...)` | new | same-buffer |
| `ray_gradient` (`gizmos.rs:471`) | `rayGradient(...)` | new | same-buffer |
| `linestrip(positions, color)` (`gizmos.rs:497`) | `linestrip(positions, color)`; `positions` a `Vec3[]` or a `Float32Array` of 3 per point (design 4) | new | same-buffer, `a_long_strip_crosses_as_a_slice` |
| `lineloop` (`gizmos.rs:525`) | `lineloop(positions, color)` | new | same-buffer |
| `linestrip_gradient(points)` (`gizmos.rs:564`) | `linestripGradient([[p, color], ...])` (design 4) | new | same-buffer |
| `line_2d`, `line_gradient_2d`, `linestrip_2d`, `lineloop_2d`, `linestrip_gradient_2d`, `ray_2d`, `ray_gradient_2d` (`gizmos.rs:722-867`) | `line2d`, `lineGradient2d`, `linestrip2d`, `lineloop2d`, `linestripGradient2d`, `ray2d`, `rayGradient2d`; 2D positions `Vec2[]` or 2 floats per point | new | same-buffer |

## Shapes

| Bevy | TS | Status | Test |
|---|---|---|---|
| `rect(isometry, size, color)` (`gizmos.rs:610`), `rect_2d` (`gizmos.rs:898`) | `rect`, `rect2d`; an isometry is a `Vec3`, a `Quat` or `{ rotation, translation }` (design 3) | new | same-buffer, `isometry_takes_bevys_into_forms` |
| `cube(transform, color)` (`gizmos.rs:632`) | `cube(transform, color)`; a `Transform`-shaped value or a `GlobalTransform` (design 3) | new | same-buffer |
| `aabb_3d(aabb, transform, color)` (`gizmos.rs:672`) | `aabb3d({ min, max }, transform, color)` | new | same-buffer |
| `cross`, `cross_2d` (`cross.rs:32,65`) | `cross`, `cross2d` | new | same-buffer |
| `circle`, `circle_2d`, `ellipse`, `ellipse_2d` (`circles.rs:50-167`) + `.resolution(n)` (`circles.rs:242,283`) | same names, `.resolution(n)` chained (design 5) | new | same-buffer, `a_builder_setting_redraws_the_shape` |
| `sphere` (`circles.rs:207`) + `.resolution` (`circles.rs:331`) | `sphere(...).resolution(n)` | new | same-buffer |
| `arc_2d`, `arc_3d`, `short_arc_3d_between`, `long_arc_3d_between`, `short_arc_2d_between`, `long_arc_2d_between` (`arcs.rs:46-402`) + `.resolution` (`arcs.rs:84,472`) | `arc2d`, `arc3d`, `shortArc3dBetween`, `longArc3dBetween`, `shortArc2dBetween`, `longArc2dBetween` | new | same-buffer |
| `arrow`, `arrow_2d` (`arrows.rs:129,158`) + `.with_tip_length`, `.with_double_end` (`arrows.rs:48,55`) | `arrow(...).withTipLength(l).withDoubleEnd()` | new | same-buffer |
| `axes`, `axes_2d` (`arrows.rs:197,228`) | `axes(transform, baseLength)`, `axes2d` | new | same-buffer |
| `grid`, `grid_3d`, `grid_2d` (`grid.rs:217-319`) + `.skew_x/_y/_z`, `.skew`, `.outer_edges_x/_y/_z`, `.outer_edges` (`grid.rs:46-133`) | same names camelCased; `cellCount` a `UVec2`/`UVec3` as `[x, y]` | new | same-buffer |
| `rounded_rect`, `rounded_rect_2d`, `rounded_cuboid` (`rounded_box.rs:267-373`) + `.corner_radius`, `.arc_resolution`, `.edge_radius` (`rounded_box.rs:47-67`) | `roundedRect(...).cornerRadius(r).arcResolution(n)`, `roundedCuboid(...).edgeRadius(r)` | new | same-buffer |

## Primitives

`primitive_2d(&P, isometry, color)` (`primitives/dim2.rs:30`) and `primitive_3d` (`primitives/dim3.rs:27`):
TS `gizmos.primitive2d(shape, isometry, color)` and `primitive3d`, where `shape` is the primitive's
reflected value under its type name, `{ Cuboid: { half_size: [0.5, 0.5, 0.5] } }`, written through
`reflect_value::write` (design 7). Builder settings chain as for shapes.

| Bevy impl | Builder settings | Status | Test |
|---|---|---|---|
| 2D: `Dir2` (`dim2.rs:40`), `Arc2d` (68), `Circle` (102), `CircularSector` (124), `CircularSegment` (167), `Ellipse` (209), `Annulus` (271), `Rhombus` (331), `Capsule2d` (365), `Line2d` (458), `Plane2d` (516), `Segment2d` (596), `Polyline2d` (649), `Triangle2d` (684), `Rectangle` (714), `Polygon` (750), `RegularPolygon` (785) | `Annulus`: `resolution`, `outerResolution`, `innerResolution` (`dim2.rs:252-265`); `Line2d`, `Segment2d`: `drawArrow` (`dim2.rs:452,590`) | new | same-buffer, one row per impl |
| 3D: `Dir3` (`dim3.rs:37`), `Sphere` (62), `Plane3d` (123), `Line3d` (175), `Segment3d` (210), `Polyline3d` (237), `Triangle3d` (267), `Cuboid` (295), `Cylinder` (390), `Capsule3d` (476), `Cone` (616), `ConicalFrustum` (715), `Torus` (819), `Tetrahedron` (899) | `Plane3d`: `cellCount`, `spacing` (`dim3.rs:111,117`); `Cylinder`, `Capsule3d`, `ConicalFrustum`: `resolution`; `Cone`: `resolution`, `baseResolution`, `heightResolution` (`dim3.rs:591-610`); `Torus`: `minorResolution`, `majorResolution` (`dim3.rs:807,813`) | new | same-buffer, one row per impl |

## Curves and text

| Bevy | TS | Status | Test |
|---|---|---|---|
| `curve_2d(curve, times, color)`, `curve_3d` (`curves.rs:41,77`) | `curve3d(curve, times, color)`; `curve` a reflected Bevy curve, `{ EasingCurve: { start, end, ease_fn } }`, `{ SampleAutoCurve: ... }`, `{ UnevenSampleAutoCurve: ... }`, `{ ConstantCurve: ... }` (`bevy_curve/src/easing.rs:335`, `sample_curves.rs:140,315`, `adaptors.rs:47`); `times` a `number[]` or `Float32Array` (design 8) | new | same-buffer, `a_curve_outside_the_set_throws_naming_it` |
| `curve_gradient_2d`, `curve_gradient_3d` (`curves.rs:114,159`) | `curveGradient3d(curve, [[t, color], ...])` | new | same-buffer |
| A curve built from a closure (`FunctionCurve`, `map`, `chain`, ...) | no TS form: a closure is Rust code, and a call never calls back into JS (`docs/native.md`, "Method calls are synchronous") | — | — |
| `text(isometry, text, font_size, anchor, color)`, `text_2d` (`stroke_text.rs:211,295`) | `text(isometry, "hp 10", 0.2, [0, 0], color)`, `text2d` | new | same-buffer |
| `text_sections`, `text_sections_2d` (`stroke_text.rs:253,337`) | `textSections(isometry, [["hp ", white], ["10", red]], size, anchor)` | new | same-buffer |
| `StrokeFont`, `StrokeTextLayout` (`stroke_text.rs:10-123`) | no TS form: layout internals of `text` | — | — |

## Config

| Bevy | TS | Status | Test |
|---|---|---|---|
| `GizmoConfigStore` (`config.rs:94`): `config`, `get_config`, `insert`, `iter` (`config.rs:116-212`) | `ResMut<GizmoConfigStore>` (an export standing for it, design 6): `store.config(LightGizmoConfigGroup)` gives `[GizmoConfig, LightGizmoConfigGroup]` as JSON; `store.insert(Group, config, ext)` writes both through `reflect_value::write` over the current value; `store.groups()` lists registered group names | new | `config_round_trips_through_the_store`, `insert_merges_over_the_current_config` |
| `config_mut`, `get_config_mut`, `iter_mut`, `*_dyn` (`config.rs:137-200`) | no TS form: they return references; a JS copy cannot write back. Read with `config`, write with `insert` | — | — |
| `GizmoConfig.enabled` (`config.rs:221`) | `config.enabled` | new | `disabled_group_draws_nothing` |
| `GizmoConfig.line` → `GizmoLineConfig` (`config.rs:255`): `width`, `perspective`, `style` (`Solid`, `Dotted`, `Dashed { gap_scale, line_scale }`, `config.rs:38`), `joints` (`None`, `Miter`, `Round(u32)`, `Bevel`, `config.rs:20`), `animation_offset` | `config.line.width`, `style: { Dashed: { gap_scale, line_scale } }`, `joints: { Round: 4 }`, as reflection writes them (`docs/reflection.md`) | new | `config_round_trips_through_the_store`, render: `line_width_and_style_render` |
| `GizmoConfig.depth_bias`, `render_layers` (`config.rs:235,240`) | `config.depth_bias`, `config.render_layers` (`RenderLayers` reflects as a list) | new | `config_round_trips_through_the_store` |
| `GizmoMeshConfig` (`config.rs:286`) | no TS form: a render-side component Bevy fills from the config | — | — |
| `AppGizmoBuilder::init_gizmo_group`, `insert_gizmo_config` (`lib.rs:126-195`) | Rust only: a group is a Rust type. A crate exposes its group with `app.ts_gizmo_group::<G>()` (design 2) | new | `a_project_group_reaches_ts` |
| `GizmoConfigGroup` derive, `DefaultGizmoConfigGroup`, `ErasedGizmoConfigGroup` (`config.rs:78-91`) | `DefaultGizmoConfigGroup` exported from `bevy-ts/gizmos`; `ErasedGizmoConfigGroup` none (retained gizmos only) | new | `config_round_trips_through_the_store` |

## Groups and markers

| Bevy | TS | Status | Test |
|---|---|---|---|
| `AabbGizmoConfigGroup { draw_all, default_color }` (`aabb.rs:45`) | `Gizmos<AabbGizmoConfigGroup>`, `store.config(AabbGizmoConfigGroup)` | new | `each_group_draws_into_its_own_storage`, `draw_all_draws_every_aabb` |
| `ShowAabbGizmo { color }` (`aabb.rs:63`) | `<ShowAabbGizmo color="#f00" />` | works (reflected component) | new: `show_aabb_gizmo_draws_its_box` |
| `FrustumGizmoConfigGroup` (`frustum.rs:80`), `ShowFrustumGizmo` (`frustum.rs:98`) | as for Aabb | group new; marker works | `show_frustum_gizmo_draws` |
| `SkinnedMeshBoundsGizmoConfigGroup` (`skinned_mesh_bounds.rs:54`), `ShowSkinnedMeshBoundsGizmo` (`skinned_mesh_bounds.rs:78`) | as for Aabb | group new; marker works | `config_round_trips_through_the_store` (the group); marker: none, it needs a skinned glTF, which examples lack |
| `LightGizmoConfigGroup { draw_all, color, point/spot/directional/rect_light_color }` (`bevy_light/src/gizmos.rs:165`), `LightGizmoColor` (`:150-160`) | as for Aabb; `color: "ByLightType"` or `{ Manual: "#fff" }` | group new; marker works | `show_light_gizmo_draws` |
| `ShowLightGizmo { color }` (`bevy_light/src/gizmos.rs:209`) | `<ShowLightGizmo color="MatchLightColor" />` | works | `show_light_gizmo_draws` |
| bevy-ts's own `EditorGizmos`, `OrientGizmos` (`crates/bevy-ts-viewport/src/edit.rs:50`, `orient.rs:82`) | none: edit tooling, never registered with `ts_gizmo_group` | — | `the_editor_groups_are_not_exposed` |

## Retained gizmos

| Bevy | TS | Status | Test |
|---|---|---|---|
| `GizmoAsset` (`lib.rs:325`), `GizmoAsset::new`, `Default`, the drawing API through `DerefMut` (`retained.rs:16-28`) | `const g = new GizmoAsset(); g.sphere([0, 0, 0], 1, "#f00").resolution(64);` every `Gizmos` drawing method, recorded in JS and replayed in Rust on add (design 10) | works | `a_gizmo_asset_holds_what_gizmos_would_draw` |
| `Assets<GizmoAsset>::add` | `assets.add(GizmoAsset, g)` on reflect-assets' path, a hand-map entry for `GizmoAsset`; returns an `AssetRef<GizmoAsset>`. `assets.set(ref, g2)` replaces its calls (design 10) | works | `a_gizmo_asset_holds_what_gizmos_would_draw`, `set_replaces_a_gizmo_assets_calls`, `a_gizmo_asset_builder_kept_past_a_call_throws`, `test/gizmos.ts` |
| `Gizmo { handle, line_config, depth_bias }` (`retained.rs:67`) | `<Gizmo handle={ref} line_config={{ width: 4 }} depth_bias={-0.1} />`: `depth_bias` a column (works), `line_config` a reflected hole prop, `handle` an asset ref through reflect-assets' handle hook | works | `a_retained_gizmo_renders`, frame `after-retained.png` |
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
