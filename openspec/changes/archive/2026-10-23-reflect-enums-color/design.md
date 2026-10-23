## Context

Two TSX paths write Rust components (`crates/bevy-ts-tsx/src/resolve.rs`):

- Layout tier (`Target::Native`): props are encoded to bytes and copied in. Fields bevy-ts cannot
  encode are holes: `split_fields` only knows layout fields, so a prop naming a hole fails to load.
  Every light's `color: Color` is such a hole.
- Reflect tier (`Target::Reflect`, `reflected.rs`): the component starts from `ReflectDefault` and
  each prop is a reflection path `.field`, deserialized whole with `TypedReflectDeserializer` and
  `try_apply`'d. A component that is itself an enum (`Projection`) has no `.field` to aim at, and a
  whole-value deserialize of a struct needs every field.

Colour parsing is copied four times (`ui.rs::color`, `intrinsics.rs::color`, `recipes.rs::color`,
`bevy-ts-material/src/layout.rs::color`), hex only (the material one also takes `[r, g, b, a?]`).

bevy-ui-coverage (branch `bevy-ui-coverage`, commit a597177, not landed) changes `reflected.rs`'s
`set()` once: a one-field tuple struct's TS field `value` maps to reflection's `.0`. It does not
touch enum or `Color` handling, and its UI props take colours only through `ui.rs::color`.

## Goals / Non-Goals

**Goals:**
- Any reflected enum, `Option` and `Color` is writable from TSX, nested at any depth.
- One owner for colour parsing and one for writing a JSON value into a reflected slot.
- Writable layout-tier holes become TSX props without changing TS columns.
- `camera3d` gets `projection`.

**Non-Goals:**
- Holes as TS system columns (enums with data stay invisible to queries).
- Reactive bindings (`prop={() => ...}`) on hole fields or reflect-tier fields; they stay
  layout-field only and fail to load with a message naming the field.
- New `Handle<T>` rules; handles keep the asset-path string.
- UI `style` surface (bevy-ui-coverage owns it).

## Decisions

1. **One writer: `bevy_ts::reflect_value::write(registry, server, slot, base, json)`.** It lives in
   core `bevy-ts` so `recipes.rs` (core) and `reflected.rs` (tsx) share it. It walks the JSON
   against the slot's type info:
   - `Color` slot → `bevy_ts::color::parse`.
   - `Handle<T>` slot → asset path (moved from `reflected.rs::field_value`).
   - `Option<T>` slot: `null` → `None`; anything else → `Some`, merged into `T`'s default.
   - Enum slot: a string names a unit variant; a one-key object `{ Variant: v }` picks a variant
     with data. A newly chosen variant starts from its default: a newtype variant from its inner
     type's `ReflectDefault`; a struct or tuple variant from the enum's own default when that is
     the same variant, else every field must be given. Then `v` is merged into the variant's
     fields. Bevy's enum `apply` switches variants, so `try_apply` writes the result.
   - Struct slot with an object: each key merged into that field; an unknown key is an error
     naming it.
   - Anything else: `TypedReflectDeserializer`, as today.
   *Alternative:* keep whole-value deserialize and ask authors for every field. Rejected: an
   `OrthographicProjection` has six fields and authors want one.

2. **A written value is the JSON over the default, not over the current value.** `write` takes a
   `base`: the slot's value in a fresh `ReflectDefault` of the component, found by the same path.
   So dropping `scale` from a prop restores the default scale, as a controlled field should.
   Without `ReflectDefault` the base is the current value, stated in the docs.
   *Alternative:* merge over the live value. Rejected: edits would leave stale fields behind.

3. **One colour owner: `bevy_ts::color::parse(&Value) -> Result<Color, String>`.** Forms: hex
   string (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`), `[r, g, b, a?]` in sRGB, and Bevy's reflected
   `Color` shape (deserialized by reflection, so every variant works; `alpha` may be left out and
   is 1). Channels other than alpha are required: a colour is a leaf, not merged. `ui.rs`,
   `intrinsics.rs`, `recipes.rs` and `bevy-ts-material` call it (the material converts to linear
   as now). TS helpers `srgb(r, g, b, a?)`, `linear(r, g, b, a?)`, `hsl(h, s, l, a?)` and
   `hex(s)` in `bevy-ts` build the reflected shape (or the string); they are sugar, the wire form
   is the shape. *Alternative:* CSS strings (`"hsl(30 80% 60%)"`). Rejected: a second syntax to
   own; the helpers type-check.

4. **Enum-valued reflect-tier components take `value`.** `<Projection value={...} />` writes the
   whole component (path empty). This extends bevy-ui-coverage's newtype rule (`value` → `.0`) in
   the same `set()`: `value` means the component's single value. The two rules sit side by side
   in `reflected.rs`'s path mapping.

5. **Writable holes are TSX props on layout-tier components.** `NativeComponent` gains the list of
   holes reflection can write (anything but opaque types). `resolve` splits a layout-tier
   element's props: layout fields as today, hole props into a new `reflected` map on `Resolved`.
   `reconcile::insert` inserts the bytes, then writes each hole prop through `reflected::write`;
   `update` writes a hole prop when it changed, with the same provenance rules as a controlled
   reflect-tier field. A component whose holes may own heap data already takes the reflect path
   whole (`!insertable`); nothing changes there.

6. **`camera3d { projection }`** runs `reflect_value::write` on a default `Projection`, so it takes
   exactly what `<Projection value>` takes. The recipe stays a thin convenience.

7. **Types.** `ts_type` maps `bevy_color::color::Color` to `ColorValue` (exported from `bevy-ts`).
   Layout-tier classes in `bevy.d.ts`/`native.d.ts` gain the writable holes as optional
   TSX-only props, so `tsc` accepts `<DirectionalLight color={hex("#ffd7a8")} />` while
   `Query<[DirectionalLight]>` rows do not show `color`. The hole note keeps listing them as
   invisible to systems.

### Shared seam

`crates/bevy-ts-tsx/src/reflected.rs`. Owner of enum, `Option` and `Color` writing and of the
colour parser: this change (`reflect-enums-color`). Owner of the `value` → `.0` newtype mapping
in `set()`: bevy-ui-coverage. Whichever lands second rebases onto the other, keeps both rules, and
(if this change lands second) switches `ui.rs`'s new colour call sites to `bevy_ts::color::parse`.

## Risks / Trade-offs

- [A struct field given partially used to fail; now it fills from defaults] → Documented; a typo'd
  key still fails because unknown keys are errors.
- [A struct or tuple variant with no default must be given whole] → The error names the variant
  and its missing fields.
- [Hole writes go through reflection, slower than bytes] → Only on insert and on a changed prop,
  never per frame; bound props stay on bytes.
- [The rendered test needs a GPU] → It follows `viewport_thumbnail.rs` (macOS, offscreen target,
  no window shown); the value-level tests run anywhere.

## Migration Plan

No migration. Existing hex strings keep working. Rollback is a revert.

## Open Questions

None.
