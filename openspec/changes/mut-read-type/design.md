## Context

`types.rs` types every reflected member with `ts_type`, the shape `reflect_value::write` takes:
struct fields optional, `Color` any form, 64-bit integers `number | string`, handles a path, ref,
value or load. A row reads what `reflect_value::read` gives (the codec decodes the same value), a
narrower shape. Rows reuse the write type, so reads need casts.

## Goals / Non-Goals

**Goals:** every reflected member a TS system reads is typed as read; every assignment is typed
as written; TSX props unchanged.

**Non-Goals:** Rust messages and events (`On<E>`, `MessageReader`): their fields keep the write
type, a follow-up. No runtime change: the decoder's output is unchanged.

## Decisions

1. **One walker, two modes.** `ts_type_inner` takes a mode, `Write` or `Read`; `ts_type` is
   `Write`, a new `ts_read_type` is `Read`. Read differs only where `read` is narrower:
   - struct fields and struct-variant fields required, an uncovered field left out (read omits it);
   - 64-bit and 128-bit integers `number`;
   - `Color` is `ColorRead`, declared by hand in `index.d.ts`;
   - `Handle<T>` is `string | AssetRef<T> | null` (a path, a script asset's ref, or none);
   - arrays and tuples mutable (`T[]`, `[A, B]`): a `Mut` row's value is the run's copy, edited in
     place and written back. A read row stays shallowly `readonly`, as today.
2. **Split members only where they differ.** A member whose read and write types are equal stays
   `name: T`. Otherwise it is `get name(): R; set name(v: W)` in the generated object type. TS 5.1
   allows unrelated accessor types; `Readonly<>` and mapped `ReadView<>` take the getter's type, so
   read rows read `R` and refuse assignment.
3. **Holes.** `Holes<H>`'s `H` is that object type. `MutView<T> & HolesOf<T>` (a `Mut` row) reads
   `R` and assigns `W`; `Readonly<HolesOf<T>>` (a read row, `ref.get`) reads `R`.
4. **Reflect tier.** TSX's `ElementProps<I>` maps the instance's fields, so the instance keeps
   write types. The row type rides beside it: `ReflectClass<V, F, A>`, `A` the generated accessor
   object for the row (its fields and `value`), carried on the instance under a brand.
   `ReflectRow<T>` is `A` when present, else today's shape. Enum components get `A = { get value():
   R; set value(v: V) }`.
5. **Resources.** `Res<T>` adds `Readonly<HolesOf<T>>` and reads a reflect-tier resource's row
   type; `ResMut<T>` adds `HolesOf<T>` and the row type. The runtime already defines hole accessors
   on resource views (`addHoles` on `C.Read`/`C.Write`).
6. **Out-of-range values.** `read` gives a 64-bit integer beyond ±(2^53 − 1) as a decimal string and
   a non-finite float as `"NaN"` or `"Infinity"`. The read type says `number` for both: an atlas
   index, a count or an id is never near 2^53 in a game, and a type of `number | string` forces a
   cast on every read. `docs/native.md` names the exception. Rejected: decoding wide integers as
   `Number` in TS (silently loses precision, and changes the protocol-equal decoder).
7. **`ColorRead` by hand, checked.** It is Bevy's serialized `Color`, ten spaces. A unit test in
   `types.rs` expands `Color` through the registry in read mode and compares it with the text in
   `index.d.ts`, so a Bevy upgrade that changes a space fails the test.

## Risks / Trade-offs

- `bevy.d.ts` grows by the accessor objects (about 250 members). tsc time on `pnpm check` is
  measured before and after.
- A user who assigned a read row's nested value to a write-typed variable keeps compiling: the
  read type is narrower.
